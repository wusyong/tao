use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};

pub fn set_badge_count(count: i32) {
  unsafe {
    let ui_application = Class::get("UIApplication").expect("Failed to get UIApplication class");
    let app: *mut Object = msg_send![ui_application, sharedApplication];
    let _: () = msg_send![app, setApplicationIconBadgeNumber:count];
  }
}
