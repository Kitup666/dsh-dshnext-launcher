//! DeepseekHarness 桌面窗口宿主入口。两种打开方式，同一份实现：
//!
//! - **启动器拉起**：stdin 是管道，每行一个带 token 的 URL，父进程退出
//!   （EOF）则自毁；关窗时 stdout 回 `closed` 行供 close_stops。
//! - **双击独立打开**：stdin 不是管道 → 从数据目录 `webui-url.txt` 读最近
//!   一次启动的 URL；已有窗口就前置复用；没有地址弹提示退出。
//!
//! 模式判定与全部窗口逻辑在 `dshnext::core::webview::host_main`。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> ! {
    env_logger::init();
    dshnext::core::webview::host_main()
}
