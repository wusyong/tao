use cocoa::{appkit::NSApp, base::nil, foundation::NSString};

pub fn set_badge_label(label: Option<String>) {
  unsafe {
    let label = match label {
      None => nil,
      Some(label) => NSString::alloc(nil).init_str(&label),
    };
    let dock_tile: cocoa::base::id = msg_send![NSApp(), dockTile];
    let _: cocoa::base::id = msg_send![dock_tile, setBadgeLabel: label];
  }
}
