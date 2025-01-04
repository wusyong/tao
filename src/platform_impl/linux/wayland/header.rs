use gtk::{
  glib::{self},
  pango,
  prelude::*,
  Align, ApplicationWindow, EventBox, HeaderBar, Label,
};

pub struct WlHeader;

impl WlHeader {
  pub fn setup(window: &ApplicationWindow, title: &str, min_width: i32) {
    let header = HeaderBar::builder()
      .show_close_button(true)
      .decoration_layout("menu:minimize,maximize,close")
      .build();

    let title_label = Label::new(Some(title));
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_label.set_single_line_mode(true);
    title_label.set_halign(Align::Center);

    let event_box = EventBox::new();
    event_box.set_above_child(true);
    event_box.set_visible(true);
    event_box.set_can_focus(false);

    let header_clone = header.clone();
    let event_box_clone = event_box.clone();
    glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
      let allocated_height = header_clone.allocated_height();
      event_box_clone.set_size_request(min_width, allocated_height);
      header_clone.set_size_request(min_width, allocated_height);
      glib::ControlFlow::Break
    });

    header.set_custom_title(Some(&title_label));
    event_box.add(&header);
    window.set_titlebar(Some(&event_box));

    //Set title font width
    let context = title_label.pango_context();
    let font_description = context.font_description().unwrap();
    let font_size = (font_description.size() / pango::SCALE) as f64;
    let char_width = font_size * 2.0;

    Self::connect_configure_event(window, &title_label, char_width);
    Self::connect_resize_window(&header, window);
  }

  fn connect_configure_event(window: &ApplicationWindow, title_label: &Label, char_width: f64) {
    let title_label_clone = title_label.clone();
    window.connect_configure_event(move |_, event| {
      let (width, _) = event.size();
      let max_chars = (width as f64 / char_width).floor() as i32;
      title_label_clone.set_max_width_chars(if width < 220 { 0 } else { max_chars });
      false
    });
  }

  fn connect_resize_window(header: &HeaderBar, window: &ApplicationWindow) {
    let header_weak = header.downgrade();
    window.connect_resizable_notify(move |window| {
      if let Some(header) = header_weak.upgrade() {
        let is_resizable = window.is_resizable();
        header.set_decoration_layout(if !is_resizable {
          Some("menu:minimize,close")
        } else {
          Some("menu:minimize,maximize,close")
        });
      }
    });
  }
}
