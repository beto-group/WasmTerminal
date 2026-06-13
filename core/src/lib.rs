#![warn(clippy::all, rust_2018_idioms)]

mod app;
pub use app::TerminalApp;

use eframe::wasm_bindgen::{self, prelude::*};

#[wasm_bindgen]
pub fn start_terminal(canvas_id: String) -> Result<(), wasm_bindgen::JsValue> {
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async move {
        eframe::WebRunner::new()
            .start(
                &canvas_id,
                web_options,
                Box::new(|cc| Box::new(TerminalApp::new(cc))),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}
