//! 后端与 UI 之间唯一的新抽象（DESIGN.md §3）。
//!
//! 上一代用 Tauri 的 `AppHandle::emit` 往前端推流（进度、日志、退出码）。
//! Dshnext 换成 tokio 的 unbounded channel：
//! - 后端只认 `&EventSink`，不知道 UI 是什么
//! - UI 侧用 `Subscription::run` 把接收端变成消息流，直接进 `update()`
//!
//! 比 JSON 事件强的地方是**类型安全**：字段拼错、类型不对，编译期就报。

/// 后端发给 UI 的事件。字段与上一代的 JSON 载荷一一对应。
#[derive(Debug, Clone)]
pub enum CoreEvent {
    /// 环境安装进度（对应上一代 `env-progress`）。
    EnvProgress { task: String, line: String },
    /// dsh / 插件命令的输出行（对应 `dsh-log`）。
    Log {
        profile: String,
        stream: LogStream,
        line: String,
        ts: i64,
    },
    /// 从 dsh 输出里解析到的 WebUI 地址（对应 `dsh-url`）。
    Url { profile: String, url: String },
    /// dsh 进程退出（对应 `dsh-exit`）。
    Exit { profile: String, code: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
    /// 启动器自己打的提示（如「已发送停止指令」）。
    System,
    /// `dsh plugin` 子命令的输出。
    Plugin,
}

impl LogStream {
    pub fn label(self) -> &'static str {
        match self {
            LogStream::Stdout => "stdout",
            LogStream::Stderr => "stderr",
            LogStream::System => "system",
            LogStream::Plugin => "plugin",
        }
    }
}

/// 后端只认这个：一个能发事件的东西。
///
/// unbounded 是有意的：日志洪峰时**绝不能反压住子进程的 stdout 读取循环**，
/// 那会让 dsh 自己卡在写管道上。上限交给 UI 侧的环形缓冲（`VecDeque` 上限 2000）。
pub type EventSink = tokio::sync::mpsc::UnboundedSender<CoreEvent>;
pub type EventStream = tokio::sync::mpsc::UnboundedReceiver<CoreEvent>;

pub fn channel() -> (EventSink, EventStream) {
    tokio::sync::mpsc::unbounded_channel()
}

/// 毫秒时间戳。日志行要带，放这里省得三个模块各写一遍。
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 便捷发送：channel 关闭（UI 已退出）时静默忽略，不让后端因此报错。
pub fn log(tx: &EventSink, profile: &str, stream: LogStream, line: impl Into<String>) {
    let _ = tx.send(CoreEvent::Log {
        profile: profile.to_string(),
        stream,
        line: line.into(),
        ts: now_millis(),
    });
}

pub fn progress(tx: &EventSink, task: &str, line: impl Into<String>) {
    let _ = tx.send(CoreEvent::EnvProgress {
        task: task.to_string(),
        line: line.into(),
    });
}
