//! 后端桥：把 `core/` 的异步函数接到 iced 的 `Task` / `Subscription` 上。
//!
//! 两个麻烦事，都在这里解决：
//!
//! 1. **`Subscription::run` 的 builder 是 `fn()` 裸函数指针**，捕获不了状态，
//!    所以 channel 不能在 `main` 里建好再当参数传进去。做法是把两端都放进
//!    全局：`main` 调 `init()` 建 channel、发送端进 `OnceLock`、接收端等订阅
//!    第一次启动时取走。后端要 `EventSink` 时从 `sink()` 拿。
//!
//! 2. **`ProcMap` 要跨 Task 共享**（启动时插入、停止时移除、轮询时读取），
//!    而 `Task::perform` 的 future 必须 `'static`。同样用全局解决，且它内部
//!    本来就是 `tokio::sync::Mutex`。

use crate::core::event::{CoreEvent, EventSink, EventStream};
use crate::core::procman::ProcMap;
use std::sync::{Arc, Mutex, OnceLock};

static SINK: OnceLock<EventSink> = OnceLock::new();
/// 接收端只被订阅取用一次，取走后置 None。std::Mutex 足够：只在建流时碰一下。
static STREAM: Mutex<Option<EventStream>> = Mutex::new(None);
static PROCS: OnceLock<Arc<ProcMap>> = OnceLock::new();

/// 在 `main` 里调用一次：建好 channel，发送端进全局，接收端等订阅取走。
pub fn init() {
    let (tx, rx) = crate::core::event::channel();
    let _ = SINK.set(tx);
    *STREAM.lock().expect("STREAM 锁") = Some(rx);
    let _ = procs();
}

/// 发送端。后端函数要 `EventSink` 时从这里拿。
/// 阶段 2 尚无调用方（安装/启动/插件在阶段 3 接），先留着。
#[allow(dead_code)]
pub fn sink() -> EventSink {
    SINK.get().expect("core bridge 未初始化").clone()
}

pub fn procs() -> Arc<ProcMap> {
    PROCS.get_or_init(|| Arc::new(ProcMap::default())).clone()
}

/// 把接收端变成 iced 的消息流。返回 `Subscription<CoreEvent>`，
/// 调用方自己 `.map(Message::Core)`——这样 builder 就是无捕获的裸函数指针，
/// 不用为了塞进 `fn()` 去 transmute。
pub fn events() -> iced::Subscription<CoreEvent> {
    iced::Subscription::run(|| {
        // 只在订阅第一次建立时取走接收端；iced 用 recipe hash 去重，
        // 同一个订阅不会被重复建立。
        let rx = STREAM.lock().expect("STREAM 锁").take();
        iced::stream::channel(256, async move |mut out| {
            let Some(mut rx) = rx else {
                // 理论到不了：接收端已被取走说明订阅被重建。挂着不产消息，
                // 好过 panic 把整个 UI 拖死。
                log::warn!("core 事件流被重复建立，本次不产消息");
                std::future::pending::<()>().await;
                return;
            };
            use futures_util::SinkExt;
            while let Some(event) = rx.recv().await {
                if out.send(event).await.is_err() {
                    break;
                }
            }
        })
    })
}
