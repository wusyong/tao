// Copyright 2014-2021 The winit contributors
// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0

use crate::{platform::macos::ActivationPolicy, platform_impl::platform::app_state::AppState};

use cocoa::{
  base::{id, NO},
  foundation::NSString,
};
use objc::{
  declare::ClassDecl,
  runtime::{Class, Object, Sel, BOOL},
};
use std::{
  cell::{RefCell, RefMut},
  os::raw::c_void,
};

use cocoa::foundation::{NSArray, NSURL};
use std::ffi::CStr;

static AUX_DELEGATE_STATE_NAME: &str = "auxState";

pub struct AuxDelegateState {
  /// We store this value in order to be able to defer setting the activation policy until
  /// after the app has finished launching. If the activation policy is set earlier, the
  /// menubar is initially unresponsive on macOS 10.15 for example.
  pub activation_policy: ActivationPolicy,

  pub activate_ignoring_other_apps: bool,
}

pub struct AppDelegateClass(pub *const Class);
unsafe impl Send for AppDelegateClass {}
unsafe impl Sync for AppDelegateClass {}

lazy_static! {
  pub static ref APP_DELEGATE_CLASS: AppDelegateClass = unsafe {
    let superclass = class!(NSResponder);
    let mut decl = ClassDecl::new("TaoAppDelegateParent", superclass).unwrap();

    decl.add_class_method(sel!(new), new as extern "C" fn(&Class, Sel) -> id);
    decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));

    decl.add_method(
      sel!(applicationWillFinishLaunching:),
      will_finish_launching as extern "C" fn(&Object, Sel, id),
    );

    decl.add_method(
      sel!(applicationDidFinishLaunching:),
      did_finish_launching as extern "C" fn(&Object, Sel, id),
    );
    decl.add_method(
      sel!(applicationWillTerminate:),
      application_will_terminate as extern "C" fn(&Object, Sel, id),
    );
    decl.add_method(
      sel!(application:openURLs:),
      application_open_urls as extern "C" fn(&Object, Sel, id, id),
    );
    decl.add_method(
      sel!(applicationShouldHandleReopen:hasVisibleWindows:),
      application_should_handle_reopen as extern "C" fn(&Object, Sel, id, BOOL) -> BOOL,
    );
    decl.add_method(
      sel!(applicationSupportsSecureRestorableState:),
      application_supports_secure_restorable_state as extern "C" fn(&Object, Sel, id) -> BOOL,
    );
    decl.add_ivar::<*mut c_void>(AUX_DELEGATE_STATE_NAME);

    AppDelegateClass(decl.register())
  };
}

/// Safety: Assumes that Object is an instance of APP_DELEGATE_CLASS
pub unsafe fn get_aux_state_mut(this: &Object) -> RefMut<'_, AuxDelegateState> {
  let ptr: *mut c_void = *this.get_ivar(AUX_DELEGATE_STATE_NAME);
  // Watch out that this needs to be the correct type
  (*(ptr as *mut RefCell<AuxDelegateState>)).borrow_mut()
}

extern "C" fn new(class: &Class, _: Sel) -> id {
  unsafe {
    let this: id = msg_send![class, alloc];
    let this: id = msg_send![this, init];
    (*this).set_ivar(
      AUX_DELEGATE_STATE_NAME,
      Box::into_raw(Box::new(RefCell::new(AuxDelegateState {
        activation_policy: ActivationPolicy::Regular,
        activate_ignoring_other_apps: true,
      }))) as *mut c_void,
    );
    this
  }
}

extern "C" fn dealloc(this: &Object, _: Sel) {
  unsafe {
    let state_ptr: *mut c_void = *(this.get_ivar(AUX_DELEGATE_STATE_NAME));
    // As soon as the box is constructed it is immediately dropped, releasing the underlying
    // memory
    drop(Box::from_raw(state_ptr as *mut RefCell<AuxDelegateState>));
  }
}

extern "C" fn will_finish_launching(this: &Object, _: Sel, _: id) {
  trace!("Triggered `applicationWillFinishLaunching`");
  AppState::will_launch(this);
  trace!("Completed `applicationWillFinishLaunching`");
}

extern "C" fn did_finish_launching(this: &Object, _: Sel, _: id) {
  trace!("Triggered `applicationDidFinishLaunching`");
  AppState::launched(this);
  trace!("Completed `applicationDidFinishLaunching`");
}

extern "C" fn application_will_terminate(_: &Object, _: Sel, _: id) {
  trace!("Triggered `applicationWillTerminate`");
  AppState::exit();
  trace!("Completed `applicationWillTerminate`");
}

extern "C" fn application_open_urls(_: &Object, _: Sel, _: id, urls: id) -> () {
  trace!("Trigger `application:openURLs:`");

  let urls = unsafe {
    (0..urls.count())
      .map(|i| {
        url::Url::parse(
          &CStr::from_ptr(urls.objectAtIndex(i).absoluteString().UTF8String()).to_string_lossy(),
        )
      })
      .flatten()
      .collect::<Vec<_>>()
  };
  trace!("Get `application:openURLs:` URLs: {:?}", urls);
  AppState::open_urls(urls);
  trace!("Completed `application:openURLs:`");
}

extern "C" fn application_should_handle_reopen(
  _: &Object,
  _: Sel,
  _: id,
  has_visible_windows: BOOL,
) -> BOOL {
  trace!("Triggered `applicationShouldHandleReopen`");
  AppState::reopen(has_visible_windows != NO);
  trace!("Completed `applicationShouldHandleReopen`");
  has_visible_windows
}

extern "C" fn application_supports_secure_restorable_state(_: &Object, _: Sel, _: id) -> BOOL {
  trace!("Triggered `applicationSupportsSecureRestorableState`");
  trace!("Completed `applicationSupportsSecureRestorableState`");
  objc::runtime::YES
}
