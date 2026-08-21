// A GUI app must not spawn a console window on Windows. Debug builds keep one
// so `println!` and panic output stay visible while developing.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

//! caixonho — a fast, native S3 client.
//!
//! This crate is a thin frontend: it owns the window, the runtime and the
//! rendering, and nothing else. Every decision about credentials, calls and
//! failure causes lives in `caixonho-core`, which this crate reaches only
//! through domain types — no `aws-sdk-s3` type appears here.

mod app;
mod components;
mod scroll;
mod theme;
mod views;

use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_component::{Root, TitleBar};

use app::{CaixonhoApp, World};

fn main() {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            // Before anything else: a failure during startup is exactly the
            // kind that leaves nothing on screen to read afterwards.
            let diagnostics = caixonho_core::diagnostics::start();
            gpui_component::init(cx);
            theme::install(cx);

            // Everything the application needs from this machine, read here
            // and handed over. The window is given its world; it does not go
            // looking for one — which is what lets a test hand it a world it
            // wrote instead (`XONHO-0015`).
            let world = World::from_env();

            cx.spawn(async move |cx| {
                let options = cx.update(|cx| WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1280.), px(800.)),
                        cx,
                    ))),
                    ..TitleBar::window_options()
                });

                cx.open_window(options, |window, cx| {
                    window.set_window_title("caixonho");
                    let view = cx.new(|cx| CaixonhoApp::new(diagnostics, world, window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                })
                .expect("failed to open window");
            })
            .detach();
        });
}
