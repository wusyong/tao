use std::{
  os::raw::{c_int, c_void},
  sync::Once,
};

use crate::{
  event_loop::EventLoopWindowTarget,
  hotkey::HotKey,
  keyboard::{KeyCode, ModifiersState},
  platform::scancode::KeyCodeExtScancode,
};

type KeyCallback = unsafe extern "C" fn(c_int, *mut c_void);

unsafe extern "C" fn trampoline<F>(result: c_int, user_data: *mut c_void)
where
  F: FnMut(c_int) + 'static,
{
  let user_data = &mut *(user_data as *mut F);
  user_data(result);
}

fn get_trampoline<F>() -> KeyCallback
where
  F: FnMut(c_int) + 'static,
{
  trampoline::<F>
}

#[link(name = "carbon_hotkey_binding.a", kind = "static")]
extern "C" {
  fn install_event_handler(cb: KeyCallback, data: *mut c_void) -> *mut c_void;
  fn uninstall_event_handler(handler_ref: *mut c_void) -> c_int;
  fn register_hotkey(id: i32, modifier: i32, key: i32) -> *mut c_void;
  fn unregister_hotkey(hotkey_ref: *mut c_void) -> c_int;
}

pub(crate) struct CarbonRef(pub(crate) *mut c_void);
impl CarbonRef {
  pub fn new(start: *mut c_void) -> Self {
    CarbonRef(start)
  }
}

unsafe impl Sync for CarbonRef {}
unsafe impl Send for CarbonRef {}

struct GlobalAccelerator {
  pub(crate) carbon_ref: CarbonRef,
}

impl GlobalAccelerator {
  fn new(modifiers: ModifiersState, key: KeyCode) -> Self {
    unsafe {
      let mut converted_modifiers: i32 = 0;
      if modifiers.shift_key() {
        converted_modifiers |= 512;
      }
      if modifiers.super_key() {
        converted_modifiers |= 256;
      }
      if modifiers.alt_key() {
        converted_modifiers |= 2048;
      }
      if modifiers.control_key() {
        converted_modifiers |= 4096;
      }
      let scan_code = key.to_scancode().expect("invalid scan code");

      // todo create unique id?
      let handler_ref = register_hotkey(1, converted_modifiers as i32, scan_code as i32);
      let saved_callback = Box::into_raw(Box::new(global_accelerator_handler));
      make_accelerator_callback(saved_callback);

      GlobalAccelerator {
        carbon_ref: CarbonRef::new(handler_ref),
      }
    }
  }
}

fn make_accelerator_callback<F>(handler: *mut F)
where
  F: FnMut(i32) + 'static + Sync + Send,
{
  static INIT: Once = Once::new();
  INIT.call_once(|| unsafe {
    let cb = get_trampoline::<F>();
    install_event_handler(cb, handler as *mut c_void);
    println!("ALLS DONE");
  });
}

fn global_accelerator_handler(item_id: i32) {
  println!("item_id {}", item_id);
}

pub fn register_global_accelerators<T>(
  _window_target: &EventLoopWindowTarget<T>,
  all_hotkeys: Vec<HotKey>,
) {
  for hotkey in all_hotkeys {
    if let Some(hotkeys_keycode) = hotkey.key.to_keycode() {
      // maybe we have multiple keycode for same key?
      // if we do, lets register the same hotkey with different keycode
      // binded to same hotkey id
      for keycode in hotkeys_keycode {
        GlobalAccelerator::new(hotkey.mods.into(), keycode);
      }
    }
  }
}

// todo implement drop?
