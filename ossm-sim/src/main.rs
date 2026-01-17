#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod app;
mod motion_control;
mod plotting;

use crate::motion_control::run_motion_control;

use ossm_motion::motion::run_motion;

use crate::plotting::PlotMessage;
use std::sync::mpsc::channel;

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use tokio::runtime;

    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let (tx, rx) = channel::<PlotMessage>();

    let _motion_control = runtime.spawn(run_motion_control(tx));
    let _motion = runtime.spawn(run_motion());

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 220.0]),
        // .with_icon(
        //     // NOTE: Adding an icon is optional
        //     eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon-256.png")[..])
        //         .expect("Failed to load icon"),
        // ),
        ..Default::default()
    };
    eframe::run_native(
        "OSSM-SIM",
        native_options,
        Box::new(|cc| Ok(Box::new(app::OssmSim::new(cc, rx)))),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(eframe_template::TemplateApp::new(cc)))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     env_logger::init();

//     let (sink, rx) = channel_plot();

//     let _motion_control = task::spawn(run_motion_control(sink));
//     let _motion = task::spawn(run_motion());

//     motion_state::set_motion_enabled(true);
//     motion_state::set_motion_depth_pct(100);
//     motion_state::set_motion_length_pct(100);
//     motion_state::set_motion_velocity_pct(50);
//     motion_state::set_motion_pattern(3);

//     run_liveplot(rx, LivePlotConfig::default()).unwrap();
//     Ok(())
// }
