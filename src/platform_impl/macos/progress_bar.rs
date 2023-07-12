use std::sync::Once;

use cocoa::{
  base::{id, nil},
  foundation::{NSArray, NSPoint, NSRect, NSSize},
};
use objc::{
  declare::ClassDecl,
  runtime::{Class, Object, Sel, NO},
};

use crate::window::{ProgressBarState, ProgressState};

/// Set progress indicator in the Dock.
pub fn set_progress_indicator(progress_state: ProgressBarState) {
  unsafe {
    let ns_app: id = msg_send![class!(NSApplication), sharedApplication];
    let dock_tile: id = msg_send![ns_app, dockTile];
    if dock_tile == nil {
      return;
    }

    // check progress indicator is already set or create new one
    let progress_indicator: id = get_exist_progress_indicator(dock_tile)
      .unwrap_or_else(|| create_progress_indicator(ns_app, dock_tile));

    // set progress indicator state
    if let Some(progress) = progress_state.progress {
      let progress = progress.clamp(0, 100) as f64;
      let _: () = msg_send![progress_indicator, setDoubleValue: progress];
      let _: () = msg_send![progress_indicator, setHidden: NO];
    }
    if let Some(state) = progress_state.state {
      let _: () = msg_send![
        progress_indicator,
        setHidden: matches!(state, ProgressState::None)
      ];
    }

    let _: () = msg_send![dock_tile, display];
  }
}

fn create_progress_indicator(ns_app: id, dock_tile: id) -> id {
  unsafe {
    let mut image_view: id = msg_send![dock_tile, contentView];
    if image_view == nil {
      // create new dock tile view with current app icon
      let app_icon_image: id = msg_send![ns_app, applicationIconImage];
      image_view = msg_send![class!(NSImageView), imageViewWithImage: app_icon_image];
      let _: () = msg_send![dock_tile, setContentView: image_view];
    }

    // create custom progress indicator
    let dock_tile_size: NSSize = msg_send![dock_tile, size];
    let frame = NSRect::new(
      NSPoint::new(0.0, 0.0),
      NSSize::new(dock_tile_size.width, 15.0),
    );
    let progress_class = create_progress_indicator_class();
    let progress_indicator: id = msg_send![progress_class, alloc];
    let progress_indicator: id = msg_send![progress_indicator, initWithFrame: frame];
    let _: () = msg_send![progress_indicator, autorelease];

    // set progress indicator to the dock tile
    let _: () = msg_send![image_view, addSubview: progress_indicator];

    return progress_indicator;
  }
}

fn get_exist_progress_indicator(dock_tile: id) -> Option<id> {
  unsafe {
    let content_view: id = msg_send![dock_tile, contentView];
    if content_view == nil {
      return None;
    }
    let subviews: id /* NSArray */ = msg_send![content_view, subviews];
    if subviews == nil {
      return None;
    }

    for idx in 0..subviews.count() {
      let subview: id = msg_send![subviews, objectAtIndex: idx];

      let is_progress_indicator: bool =
        msg_send![subview, isKindOfClass: class!(NSProgressIndicator)];
      if is_progress_indicator {
        return Some(subview);
      }
    }
  }
  None
}

fn create_progress_indicator_class() -> *const Class {
  static mut APP_CLASS: *const Class = 0 as *const Class;
  static INIT: Once = Once::new();

  INIT.call_once(|| unsafe {
    let superclass = class!(NSProgressIndicator);
    let mut decl = ClassDecl::new("TaoProgressIndicator", superclass).unwrap();

    decl.add_method(
      sel!(drawRect:),
      draw_progress_bar as extern "C" fn(&Object, _, NSRect),
    );

    APP_CLASS = decl.register();
  });

  unsafe { APP_CLASS }
}

extern "C" fn draw_progress_bar(_this: &Object, _: Sel, rect: NSRect) {
  unsafe {
    let bar = NSRect::new(
      NSPoint { x: 0.0, y: 0.0 },
      NSSize {
        width: rect.size.width,
        height: 15.0,
      },
    );
    let bar_inner = bar.inset(0.5, 0.5);
    let mut bar_progress = bar.inset(1.0, 1.0);

    // set progress width
    let current_progress: f64 = msg_send![_this, doubleValue];
    let normalized_progress: f64 = (current_progress / 100.0).clamp(0.0, 1.0);
    bar_progress.size.width *= normalized_progress;

    // draw outer bar
    let white_color_alpha: id = msg_send![class!(NSColor), colorWithWhite:1.0 alpha:0.8];
    let _: () = msg_send![white_color_alpha, set];
    draw_rounded_rect(bar);

    // draw inner bar
    let black_color_alpha: id = msg_send![class!(NSColor), colorWithWhite:0.0 alpha:0.8];
    let _: () = msg_send![black_color_alpha, set];
    draw_rounded_rect(bar_inner);

    // draw progress
    let white_color: id = msg_send![class!(NSColor), whiteColor];
    let _: () = msg_send![white_color, set];
    draw_rounded_rect(bar_progress);
  }
}

fn draw_rounded_rect(rect: NSRect) {
  unsafe {
    let raduis = rect.size.height / 2.0;
    let bezier_path: id =
      msg_send![class!(NSBezierPath), bezierPathWithRoundedRect:rect xRadius:raduis yRadius:raduis];
    let _: () = msg_send![bezier_path, fill];
  }
}