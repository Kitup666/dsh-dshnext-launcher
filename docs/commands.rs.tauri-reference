use crate::envres::{self, DshVersions, EnvStatus};
use crate::installs;
use crate::plugins::{self, MarketItem, PluginInfo};
use crate::profiles::{self, ProfileInfo};
use crate::procman::{self, ProcMap, ProcStatus};
use crate::store::{self, Config};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub struct AppState {
    pub config: tokio::sync::Mutex<Config>,
    pub procs: ProcMap,
}

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<Config, String> {
    Ok(state.config.lock().await.clone())
}

#[tauri::command]
pub async fn set_config(state: State<'_, AppState>, config: Config) -> Result<(), String> {
    store::save(&config)?;
    *state.config.lock().await = config;
    Ok(())
}

#[tauri::command]
pub async fn env_status(state: State<'_, AppState>) -> Result<EnvStatus, String> {
    let cfg = state.config.lock().await.clone();
    Ok(envres::status(&cfg).await)
}

#[tauri::command]
pub async fn dsh_versions(
    state: State<'_, AppState>,
    include_rc: bool,
) -> Result<DshVersions, String> {
    let cfg = state.config.lock().await.clone();
    installs::dsh_versions(cfg, include_rc).await
}

#[tauri::command]
pub async fn node_versions(state: State<'_, AppState>) -> Result<Vec<envres::NodeRelease>, String> {
    let cfg = state.config.lock().await.clone();
    envres::fetch_node_versions(&cfg).await
}

#[tauri::command]
pub async fn install_node(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> Result<(), String> {
    let cfg = state.config.lock().await.clone();
    installs::install_node(app, cfg, version).await
}

#[tauri::command]
pub async fn install_dsh(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> Result<(), String> {
    let cfg = state.config.lock().await.clone();
    installs::install_dsh(app, cfg, version).await
}

#[tauri::command]
pub async fn install_pnpm(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cfg = state.config.lock().await.clone();
    installs::install_pnpm(app, cfg).await
}

#[tauri::command]
pub fn profile_list() -> Result<Vec<ProfileInfo>, String> {
    profiles::list()
}

#[tauri::command]
pub fn profile_create(name: String) -> Result<(), String> {
    profiles::create(&name)
}

#[tauri::command]
pub fn profile_copy(src: String, dst: String) -> Result<(), String> {
    profiles::copy(&src, &dst)
}

#[tauri::command]
pub async fn profile_rename(
    state: State<'_, AppState>,
    src: String,
    dst: String,
) -> Result<(), String> {
    if procman::is_running(&state.procs, &src).await {
        return Err(format!("版本 {src} 正在运行，请先停止"));
    }
    profiles::rename(&src, &dst)
}

#[tauri::command]
pub async fn profile_delete(state: State<'_, AppState>, name: String) -> Result<(), String> {
    if procman::is_running(&state.procs, &name).await {
        return Err(format!("版本 {name} 正在运行，请先停止"));
    }
    profiles::delete(&name)
}

#[tauri::command]
pub async fn uninstall_node() -> Result<(), String> {
    installs::remove_node()
}

#[tauri::command]
pub async fn uninstall_dsh() -> Result<(), String> {
    installs::remove_dsh()
}

#[tauri::command]
pub async fn dsh_start(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    port: Option<u16>,
) -> Result<String, String> {
    let cfg = state.config.lock().await.clone();
    let port = port.unwrap_or(cfg.port);
    procman::start(app, &state.procs, &cfg, &profile, port).await
}

#[tauri::command]
pub async fn dsh_stop(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
) -> Result<(), String> {
    procman::stop(app.clone(), &state.procs, &profile).await?;
    // 服务已停，对应的 WebUI 窗口只会显示连接错误，一并关掉
    if let Some(w) = app.get_webview_window(&format!("webui-{profile}")) {
        let _ = w.close();
    }
    Ok(())
}

#[tauri::command]
pub async fn dsh_status(state: State<'_, AppState>) -> Result<Vec<ProcStatus>, String> {
    Ok(procman::status(&state.procs).await)
}

#[tauri::command]
pub fn plugin_list(profile: String) -> Result<Vec<PluginInfo>, String> {
    plugins::list(&profile)
}

#[tauri::command]
pub async fn plugin_add(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    source: String,
) -> Result<(), String> {
    let cfg = state.config.lock().await.clone();
    plugins::add(app, &cfg, &profile, &source).await
}

#[tauri::command]
pub async fn plugin_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: String,
    name: String,
) -> Result<(), String> {
    let cfg = state.config.lock().await.clone();
    plugins::remove(app, &cfg, &profile, &name).await
}

#[tauri::command]
pub async fn market_items(state: State<'_, AppState>) -> Result<Vec<MarketItem>, String> {
    let cfg = state.config.lock().await.clone();
    plugins::market(&cfg).await
}

#[tauri::command]
pub fn open_profile_dir(app: AppHandle, profile: String) -> Result<(), String> {
    let dir = crate::envres::profiles_root().join(&profile);
    if !dir.exists() {
        return Err("目录不存在".into());
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = store::data_dir();
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_in_browser(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_webui_window(
    app: AppHandle,
    profile: String,
    url: String,
) -> Result<(), String> {
    let label = format!("webui-{}", profile);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.set_focus();
        return Ok(());
    }
    let parsed: tauri::Url = url
        .parse()
        .map_err(|e| format!("URL 无效: {e}"))?;
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
        .title(format!("{} — DeepSeek Harness WebUI", profile))
        .inner_size(1280.0, 800.0)
        .build()
        .map_err(|e| format!("打开窗口失败: {e}"))?;
    Ok(())
}
