mod commands;
mod envres;
mod installs;
mod plugins;
mod procman;
mod profiles;
mod store;

use commands::AppState;
use procman::ProcMap;
use std::collections::HashMap;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            config: tokio::sync::Mutex::new(store::load()),
            procs: ProcMap(tokio::sync::Mutex::new(HashMap::new())),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::env_status,
            commands::dsh_versions,
            commands::node_versions,
            commands::install_node,
            commands::install_dsh,
            commands::install_pnpm,
            commands::profile_list,
            commands::profile_create,
            commands::profile_copy,
            commands::profile_rename,
            commands::profile_delete,
            commands::uninstall_node,
            commands::uninstall_dsh,
            commands::dsh_start,
            commands::dsh_stop,
            commands::dsh_status,
            commands::plugin_list,
            commands::plugin_add,
            commands::plugin_remove,
            commands::market_items,
            commands::open_profile_dir,
            commands::open_data_dir,
            commands::open_in_browser,
            commands::open_webui_window
        ])
        .build(tauri::generate_context!())
        .expect("启动 DshDesk 失败")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // 退出时清掉所有 dsh 子进程树
                use std::os::windows::process::CommandExt;
                if let Some(state) = app.try_state::<AppState>() {
                    if let Ok(map) = state.procs.0.try_lock() {
                        for (_, m) in map.iter() {
                            let _ = std::process::Command::new("taskkill")
                                .args(["/PID", &m.pid.to_string(), "/T", "/F"])
                                .creation_flags(0x0800_0000)
                                .output();
                        }
                    }
                }
            }
        });
}
