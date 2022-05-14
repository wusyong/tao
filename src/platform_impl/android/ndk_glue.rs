use crate::window::Window;
use crossbeam_channel::*;
use jni::{
  objects::{GlobalRef, JClass, JObject, JString, JValue},
  sys::jobjectArray,
  JNIEnv,
};
use libc::c_void;
use log::Level;
use ndk::{
  input_queue::InputQueue,
  looper::{FdEvent, ForeignLooper, ThreadLooper},
  native_window::NativeWindow,
};
use ndk_sys::{AInputQueue, ANativeActivity, ANativeWindow, ARect};
use once_cell::sync::{Lazy, OnceCell};
use std::{
  ffi::{CStr, CString},
  fs::File,
  io::{BufRead, BufReader},
  os::{raw, unix::prelude::*},
  rc::Rc,
  sync::{Arc, Condvar, Mutex, RwLock, RwLockReadGuard},
  thread,
};

static CHANNEL: Lazy<(Sender<WebViewMessage>, Receiver<WebViewMessage>)> = Lazy::new(|| bounded(8));
static MAIN_PIPE: Lazy<[RawFd; 2]> = Lazy::new(|| {
  let mut pipe: [RawFd; 2] = Default::default();
  unsafe { libc::pipe(pipe.as_mut_ptr()) };
  pipe
});

pub struct MainPipe<'a> {
  env: JNIEnv<'a>,
  activity: GlobalRef,
  scripts: Vec<String>,
  webview: Option<GlobalRef>,
}

impl MainPipe<'_> {
  pub fn send(message: WebViewMessage) {
    let size = std::mem::size_of::<bool>();
    if let Ok(()) = CHANNEL.0.send(message) {
      unsafe { libc::write(MAIN_PIPE[1], &true as *const _ as *const _, size) };
    }
  }

  fn recv(&mut self) -> Result<(), jni::errors::Error> {
    let env = self.env;
    let activity = self.activity.as_obj();
    if let Ok(message) = CHANNEL.1.recv() {
      match message {
        WebViewMessage::CreateWebView(url, mut scripts, devtools) => {
          // Create webview
          let class = env.find_class("android/webkit/WebView")?;
          let webview =
            env.new_object(class, "(Landroid/content/Context;)V", &[activity.into()])?;

          // Enable Javascript
          let settings = env
            .call_method(
              webview,
              "getSettings",
              "()Landroid/webkit/WebSettings;",
              &[],
            )?
            .l()?;
          env.call_method(settings, "setJavaScriptEnabled", "(Z)V", &[true.into()])?;

          // Load URL
          if let Ok(url) = env.new_string(url) {
            env.call_method(webview, "loadUrl", "(Ljava/lang/String;)V", &[url.into()])?;
          }

          // Enable devtools
          env.call_static_method(
            class,
            "setWebContentsDebuggingEnabled",
            "(Z)V",
            &[devtools.into()],
          )?;

          // Initialize scripts
          self.scripts.append(&mut scripts);

          // Set webview client
          let client = env.call_method(
            activity,
            "getClient",
            "()Landroid/webkit/WebViewClient;",
            &[],
          )?;
          env.call_method(
            webview,
            "setWebViewClient",
            "(Landroid/webkit/WebViewClient;)V",
            &[client.into()],
          )?;

          // Add javascript interface (IPC)
          let handler =
            env.call_method(activity, "getIpc", "()Lcom/example/hh/IpcHandler;", &[])?;
          let ipc = env.new_string("ipc")?;
          env.call_method(
            webview,
            "addJavascriptInterface",
            "(Ljava/lang/Object;Ljava/lang/String;)V",
            &[handler.into(), ipc.into()],
          )?;

          env.call_method(
            activity,
            "setContentView",
            "(Landroid/view/View;)V",
            &[webview.into()],
          )?;
          let webview = env.new_global_ref(webview)?;
          self.webview = Some(webview);
        }
        WebViewMessage::Eval => {
          if let Some(webview) = &self.webview {
            for s in self.scripts.drain(..).into_iter() {
              let s = env.new_string(s)?;
              env.call_method(
                webview.as_obj(),
                "evaluateJavascript",
                "(Ljava/lang/String;Landroid/webkit/ValueCallback;)V",
                &[s.into(), JObject::null().into()],
              )?;
            }
          }
        }
        _ => (),
      }
    }
    Ok(())
  }
}

#[derive(Debug)]
pub enum WebViewMessage {
  CreateWebView(String, Vec<String>, bool),
  Eval,
}

pub static IPC: OnceCell<UnsafeIpc> = OnceCell::new();
pub struct UnsafeIpc(*mut c_void, Rc<Window>);
impl UnsafeIpc {
  pub fn new(f: *mut c_void, w: Rc<Window>) -> Self {
    Self(f, w)
  }
}
unsafe impl Send for UnsafeIpc {}
unsafe impl Sync for UnsafeIpc {}

/// `ndk-glue` macros register the reading end of an event pipe with the
/// main [`ThreadLooper`] under this `ident`.
/// When returned from [`ThreadLooper::poll_*`](ThreadLooper::poll_once)
/// an event can be retrieved from [`poll_events()`].
pub const NDK_GLUE_LOOPER_EVENT_PIPE_IDENT: i32 = 0;

/// The [`InputQueue`] received from Android is registered with the main
/// [`ThreadLooper`] under this `ident`.
/// When returned from [`ThreadLooper::poll_*`](ThreadLooper::poll_once)
/// an event can be retrieved from [`input_queue()`].
pub const NDK_GLUE_LOOPER_INPUT_QUEUE_IDENT: i32 = 1;

pub fn android_log(level: Level, tag: &CStr, msg: &CStr) {
  let prio = match level {
    Level::Error => ndk_sys::android_LogPriority_ANDROID_LOG_ERROR,
    Level::Warn => ndk_sys::android_LogPriority_ANDROID_LOG_WARN,
    Level::Info => ndk_sys::android_LogPriority_ANDROID_LOG_INFO,
    Level::Debug => ndk_sys::android_LogPriority_ANDROID_LOG_DEBUG,
    Level::Trace => ndk_sys::android_LogPriority_ANDROID_LOG_VERBOSE,
  };
  unsafe {
    ndk_sys::__android_log_write(prio as raw::c_int, tag.as_ptr(), msg.as_ptr());
  }
}

static NATIVE_WINDOW: Lazy<RwLock<Option<NativeWindow>>> = Lazy::new(|| Default::default());
static INPUT_QUEUE: Lazy<RwLock<Option<InputQueue>>> = Lazy::new(|| Default::default());
static CONTENT_RECT: Lazy<RwLock<Rect>> = Lazy::new(|| Default::default());
static LOOPER: Lazy<Mutex<Option<ForeignLooper>>> = Lazy::new(|| Default::default());

pub fn native_window() -> RwLockReadGuard<'static, Option<NativeWindow>> {
  NATIVE_WINDOW.read().unwrap()
}

pub fn input_queue() -> RwLockReadGuard<'static, Option<InputQueue>> {
  INPUT_QUEUE.read().unwrap()
}

pub fn content_rect() -> Rect {
  CONTENT_RECT.read().unwrap().clone()
}

static PIPE: Lazy<[RawFd; 2]> = Lazy::new(|| {
  let mut pipe: [RawFd; 2] = Default::default();
  unsafe { libc::pipe(pipe.as_mut_ptr()) };
  pipe
});

pub fn poll_events() -> Option<Event> {
  unsafe {
    let size = std::mem::size_of::<Event>();
    let mut event = Event::Start;
    if libc::read(PIPE[0], &mut event as *mut _ as *mut _, size) == size as libc::ssize_t {
      Some(event)
    } else {
      None
    }
  }
}

unsafe fn wake(event: Event) {
  log::trace!("{:?}", event);
  let size = std::mem::size_of::<Event>();
  let res = libc::write(PIPE[1], &event as *const _ as *const _, size);
  assert_eq!(res, size as libc::ssize_t);
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Rect {
  pub left: u32,
  pub top: u32,
  pub right: u32,
  pub bottom: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Event {
  Start,
  Resume,
  SaveInstanceState,
  Pause,
  Stop,
  Destroy,
  ConfigChanged,
  LowMemory,
  WindowLostFocus,
  WindowHasFocus,
  WindowCreated,
  WindowResized,
  WindowRedrawNeeded,
  /// If the window is in use by ie. a graphics API, make sure the lock from
  /// [`native_window()`] is held on to until after freeing those resources.
  ///
  /// After receiving this [`Event`] `ndk_glue` will block until that read-lock
  /// is released before returning to Android and allowing it to free up the window.
  WindowDestroyed,
  InputQueueCreated,
  /// After receiving this [`Event`] `ndk_glue` will block until the read-lock from
  /// [`input_queue()`] is released before returning to Android and allowing it to
  /// free up the input queue.
  InputQueueDestroyed,
  ContentRectChanged,
}

pub unsafe fn on_create(env: JNIEnv, jclass: JClass, jobject: JObject, main: fn()) {
  //-> jobjectArray {
  // Initialize global context
  let activity = env.new_global_ref(jobject).unwrap();
  let vm = env.get_java_vm().unwrap();
  let env = vm.attach_current_thread_as_daemon().unwrap();
  ndk_context::initialize_android_context(
    vm.get_java_vm_pointer() as *mut _,
    activity.as_obj().into_inner() as *mut _,
  );

  let mut main_pipe = MainPipe {
    env,
    activity,
    scripts: vec![],
    webview: None,
  };
  let looper = ThreadLooper::for_thread().unwrap().into_foreign();
  looper
    .add_fd_with_callback(MAIN_PIPE[0], FdEvent::INPUT, move |_| {
      let size = std::mem::size_of::<bool>();
      let mut wake = false;
      if libc::read(MAIN_PIPE[0], &mut wake as *mut _ as *mut _, size) == size as libc::ssize_t {
        match main_pipe.recv() {
          Ok(_) => true,
          Err(_) => false,
        }
      } else {
        false
      }
    })
    .unwrap();

  let mut logpipe: [RawFd; 2] = Default::default();
  libc::pipe(logpipe.as_mut_ptr());
  libc::dup2(logpipe[1], libc::STDOUT_FILENO);
  libc::dup2(logpipe[1], libc::STDERR_FILENO);
  thread::spawn(move || {
    let tag = CStr::from_bytes_with_nul(b"RustStdoutStderr\0").unwrap();
    let file = File::from_raw_fd(logpipe[0]);
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();
    loop {
      buffer.clear();
      if let Ok(len) = reader.read_line(&mut buffer) {
        if len == 0 {
          break;
        } else if let Ok(msg) = CString::new(buffer.clone()) {
          android_log(Level::Info, tag, &msg);
        }
      }
    }
  });

  let looper_ready = Arc::new(Condvar::new());
  let signal_looper_ready = looper_ready.clone();

  thread::spawn(move || {
    let looper = ThreadLooper::prepare();
    let foreign = looper.into_foreign();
    foreign
      .add_fd(
        PIPE[0],
        NDK_GLUE_LOOPER_EVENT_PIPE_IDENT,
        FdEvent::INPUT,
        std::ptr::null_mut(),
      )
      .unwrap();

    {
      let mut locked_looper = LOOPER.lock().unwrap();
      *locked_looper = Some(foreign);
      signal_looper_ready.notify_one();
    }

    main()
  });

  // Don't return from this function (`ANativeActivity_onCreate`) until the thread
  // has created its `ThreadLooper` and assigned it to the static `LOOPER`
  // variable. It will be used from `on_input_queue_created` as soon as this
  // function returns.
  let locked_looper = LOOPER.lock().unwrap();
  let _mutex_guard = looper_ready
    .wait_while(locked_looper, |looper| looper.is_none())
    .unwrap();
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_WryClient_eval(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JString,
) {
  MainPipe::send(WebViewMessage::Eval);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_IpcHandler_ipc(
  env: JNIEnv,
  _jclass: JClass,
  jobject: JString,
) {
  match env.get_string(jobject) {
    Ok(arg) => {
      let arg = arg.to_string_lossy().to_string();
      if let Some(w) = IPC.get() {
        let ipc = w.0;
        if !ipc.is_null() {
          let ipc = &*(ipc as *mut Box<dyn Fn(&Window, String)>);
          ipc(&w.1, arg)
        }
      }
    }
    Err(e) => log::error!("Failed to parse JString: {}", e),
  }
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_resume(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::Resume);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_pause(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::Pause);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_focus(
  _env: JNIEnv,
  _jclass: JClass,
  has_focus: libc::c_int,
) {
  let event = if has_focus == 0 {
    Event::WindowLostFocus
  } else {
    Event::WindowHasFocus
  };
  wake(event);
}

// Events below are not used by event loop yet.
#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_start(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::Start);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_stop(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::Stop);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_save(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::SaveInstanceState);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_destroy(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::Destroy);
}

#[no_mangle]
pub unsafe extern "C" fn Java_com_example_hh_MainActivity_memory(
  _env: JNIEnv,
  _jclass: JClass,
  _jobject: JObject,
) {
  wake(Event::LowMemory);
}

/*
unsafe extern "C" fn on_configuration_changed(activity: *mut ANativeActivity) {
  wake(activity, Event::ConfigChanged);
}

unsafe extern "C" fn on_window_created(activity: *mut ANativeActivity, window: *mut ANativeWindow) {
  *NATIVE_WINDOW.write().unwrap() = Some(NativeWindow::from_ptr(NonNull::new(window).unwrap()));
  wake(activity, Event::WindowCreated);
}

unsafe extern "C" fn on_window_resized(
  activity: *mut ANativeActivity,
  _window: *mut ANativeWindow,
) {
  wake(activity, Event::WindowResized);
}

unsafe extern "C" fn on_window_redraw_needed(
  activity: *mut ANativeActivity,
  _window: *mut ANativeWindow,
) {
  wake(activity, Event::WindowRedrawNeeded);
}

unsafe extern "C" fn on_window_destroyed(
  activity: *mut ANativeActivity,
  window: *mut ANativeWindow,
) {
  wake(activity, Event::WindowDestroyed);
  let mut native_window_guard = NATIVE_WINDOW.write().unwrap();
  assert_eq!(native_window_guard.as_ref().unwrap().ptr().as_ptr(), window);
  *native_window_guard = None;
}

unsafe extern "C" fn on_input_queue_created(
  activity: *mut ANativeActivity,
  queue: *mut AInputQueue,
) {
  let input_queue = InputQueue::from_ptr(NonNull::new(queue).unwrap());
  let locked_looper = LOOPER.lock().unwrap();
  // The looper should always be `Some` after `fn init()` returns, unless
  // future code cleans it up and sets it back to `None` again.
  let looper = locked_looper.as_ref().expect("Looper does not exist");
  input_queue.attach_looper(looper, NDK_GLUE_LOOPER_INPUT_QUEUE_IDENT);
  *INPUT_QUEUE.write().unwrap() = Some(input_queue);
  wake(activity, Event::InputQueueCreated);
}

unsafe extern "C" fn on_input_queue_destroyed(
  activity: *mut ANativeActivity,
  queue: *mut AInputQueue,
) {
  wake(activity, Event::InputQueueDestroyed);
  let mut input_queue_guard = INPUT_QUEUE.write().unwrap();
  assert_eq!(input_queue_guard.as_ref().unwrap().ptr().as_ptr(), queue);
  let input_queue = InputQueue::from_ptr(NonNull::new(queue).unwrap());
  input_queue.detach_looper();
  *input_queue_guard = None;
}

unsafe extern "C" fn on_content_rect_changed(activity: *mut ANativeActivity, rect: *const ARect) {
  let rect = Rect {
    left: (*rect).left as _,
    top: (*rect).top as _,
    right: (*rect).right as _,
    bottom: (*rect).bottom as _,
  };
  *CONTENT_RECT.write().unwrap() = rect;
  wake(activity, Event::ContentRectChanged);
}
*/
