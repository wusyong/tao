// Copyright 2014-2021 The winit contributors
// Copyright 2021-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0

use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

#[allow(clippy::single_match)]
fn main() {
    let event_loop = EventLoop::new();

    let mut window = Some(Window::new(&event_loop).unwrap());

    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // drop the window
                window = None;
            }
            Event::MainEventsCleared => {
                if let Some(w) = &window {
                    w.request_redraw();
                }
            }
            Event::ExitRequested => {
                println!("Exit requested");

                // drop the window
                window = None;
            }
            _ => (),
        }
    });
}
