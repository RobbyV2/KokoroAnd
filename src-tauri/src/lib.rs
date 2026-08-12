pub mod android;
mod commands;

use commands::{
    AppState, battery_exemption, clear_cache, delete_models, demo_synth, dict_export, dict_import,
    download_cancel, download_progress, download_start, engine_init, install_status, server_start,
    server_status, server_stop, settings_get, settings_set,
};
use tauri::Manager;

pub use commands::Tts;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState::new(app.handle())?);
            commands::startup_always_on(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            download_start,
            download_progress,
            download_cancel,
            install_status,
            engine_init,
            demo_synth,
            server_start,
            server_stop,
            server_status,
            settings_get,
            settings_set,
            dict_import,
            dict_export,
            clear_cache,
            delete_models,
            battery_exemption,
        ])
        .build(tauri::generate_context!())
        .expect("tauri build failed")
        .run(|_app, _event| {
            #[cfg(target_os = "android")]
            if let tauri::RunEvent::Resumed = _event {
                if _app.webview_windows().is_empty() {
                    match _app.config().app.windows.first() {
                        Some(cfg) => {
                            if let Err(e) = tauri::WebviewWindowBuilder::from_config(_app, cfg)
                                .and_then(|b| b.build())
                            {
                                eprintln!("window recreate failed: {e}");
                            }
                        }
                        None => eprintln!("no window config to recreate"),
                    }
                }
            }
        })
}
