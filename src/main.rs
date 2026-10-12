// Release builds on Windows should not open a console next to the window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod engine;
mod model;
mod settings;
mod shortcuts;
mod theme;
mod ui;

use gpui_kit::*;

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);
        theme::install(cx);

        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        open_window(app::window_options(cx), cx, |window, cx| {
            cx.new(|cx| app::ClickityApp::new(window, cx))
        })
        .expect("Clickity could not open its window");
        cx.activate(true);
    });
}
