//! 从上一代 `../src-tauri/src/` 直接复制过来的后端。
//!
//! **业务逻辑一行不改**，只把 Tauri 的 `AppHandle`/`Emitter` 换成 `event::EventSink`
//! （DESIGN.md §3 的机械替换表）。`store`/`profiles`/`envres` 本来就零耦合。
//!
//! 模块间的路径引用从上一代的 `crate::xxx` 改成 `crate::core::xxx`——这是复制
//! 过来后唯一的批量改动。
//!
//! 阶段 2 只接了环境探测、配置读写、profile 列表、进程轮询四条链路，
//! 剩下的（安装、启动/停止、插件增删、市场检索）在阶段 3 随页面一起接。
//! 未被调用的函数先留着，别删——它们是上一代验证过的逻辑。
#![allow(dead_code)]

pub mod appwin;
pub mod diag;
pub mod envres;
pub mod event;
pub mod installs;
pub mod migrate;
pub mod platform;
pub mod plugins;
pub mod procman;
pub mod profiles;
pub mod selfupdate;
pub mod store;
