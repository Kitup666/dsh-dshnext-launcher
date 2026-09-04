//! 过渡动画（DESIGN.md §7.1）。
//!
//! CSS 的 `transition: background .14s` 在 iced 里没有对应物，只能自己补间。
//! 关键纪律：**动画结束必须把 tween 移除**，否则 `window::frames()` 一直订阅着，
//! 空闲 CPU 回不到 0，整个重构的意义就没了。这条在阶段 1 用出帧计数验证。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 补间目标的标识。用 `&'static str`：Copy、可 Hash、日志里直接可读。
pub type Key = &'static str;

/// hover 过渡时长，对应 CSS `.14s ease`。
pub const HOVER_DUR: Duration = Duration::from_millis(140);

/// 一个值的补间。`update` 里推进（`AnimState::tick`），`view` 里读当前值。
#[derive(Debug, Clone, Copy)]
pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    dur: Duration,
}

impl Tween {
    pub fn between(from: f32, to: f32, dur: Duration, now: Instant) -> Self {
        Self {
            from,
            to,
            start: now,
            dur,
        }
    }

    /// 缓动插值。CSS `ease` ≈ cubic-bezier(.25,.1,.25,1)，这里用 ease-out cubic
    /// `1-(1-x)³` 近似——观感差异在 140ms 的短过渡里不可辨。
    pub fn value_at(&self, now: Instant) -> f32 {
        let x = (now - self.start).as_secs_f32() / self.dur.as_secs_f32();
        let x = x.clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - x) * (1.0 - x) * (1.0 - x);
        self.from + (self.to - self.from) * e
    }

    pub fn done(&self, now: Instant) -> bool {
        now - self.start >= self.dur
    }
}

/// 全局动画状态：每个 key 一个当前值 + 至多一个活跃补间。
#[derive(Default)]
pub struct AnimState {
    values: HashMap<Key, f32>,
    tweens: HashMap<Key, Tween>,
}

impl AnimState {
    /// 当前值。没有记录 = 0.0（未 hover）。
    pub fn value(&self, key: Key) -> f32 {
        self.values.get(key).copied().unwrap_or(0.0)
    }

    /// 是否还有补间在跑。**决定 subscription 要不要订阅 frames。**
    pub fn is_animating(&self) -> bool {
        !self.tweens.is_empty()
    }

    pub fn tween_count(&self) -> usize {
        self.tweens.len()
    }

    /// 把 key 补间到 target。若当前值已等于 target 则直接返回（不产生帧）。
    /// 中途改向：从当前值重新出发，时长不变——CSS transition 的行为。
    pub fn animate_to(&mut self, key: Key, target: f32, dur: Duration, now: Instant) {
        let from = self.value(key);
        if (from - target).abs() < f32::EPSILON {
            self.tweens.remove(key);
            return;
        }
        self.tweens.insert(key, Tween::between(from, target, dur, now));
    }

    /// 推进所有补间；返回是否仍在动画中。完成的 tween 落定到目标值并移除。
    pub fn tick(&mut self, now: Instant) -> bool {
        // 解构借用：retain 可变借用 tweens，闭包里再碰 self.values 会冲突。
        let AnimState { values, tweens } = self;
        tweens.retain(|key, tw| {
            let v = tw.value_at(now);
            values.insert(*key, v);
            !tw.done(now)
        });
        !tweens.is_empty()
    }
}
