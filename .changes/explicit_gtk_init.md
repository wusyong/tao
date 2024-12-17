---
"tao": patch
---

Explicitly call `gtk::init()` as `gtk::Application::new` does not call it reliably.
If `gtk::init()` is not called, 'GTK may only be used from the main thread' error can occur later.
`gtk::init()` checks if already initialized and does nothing if so.
