//! 设计令牌，色值直接抄 `../../src/styles.css` 的 `:root[data-theme]` 两组。
//! 阶段 0 只用暗色，浅色一并写好是为了确认 `Color::from_rgb8` 这条路走得通。

use iced::{Color, Shadow, Vector};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_app: Color,
    pub surface_1: Color,
    pub surface_2: Color,
    pub border: Color,
    pub border_mid: Color,
    pub text: Color,
    pub text_2: Color,
    pub text_3: Color,
    pub accent: Color,
    pub accent_soft: Color,
    pub input_bg: Color,
    pub shadow_card: Shadow,
}

impl Palette {
    /// 对应 `:root[data-theme="dark"]`：#060607 底、#0e0e10 卡、#5b76ff 强调。
    pub const DARK: Self = Self {
        bg_app: Color::from_rgb(0.0235, 0.0235, 0.0275),
        surface_1: Color::from_rgb(0.0549, 0.0549, 0.0627),
        surface_2: Color::from_rgb(0.0863, 0.0863, 0.098),
        border: Color::from_rgba(1.0, 1.0, 1.0, 0.06),
        border_mid: Color::from_rgba(1.0, 1.0, 1.0, 0.10),
        text: Color::from_rgb(0.949, 0.949, 0.957),
        text_2: Color::from_rgb(0.604, 0.612, 0.651),
        text_3: Color::from_rgb(0.361, 0.369, 0.408),
        accent: Color::from_rgb(0.357, 0.463, 1.0),
        accent_soft: Color::from_rgba(0.357, 0.463, 1.0, 0.14),
        input_bg: Color::from_rgb(0.039, 0.039, 0.047),
        // CSS: 0 20px 40px -24px rgba(0,0,0,.8)
        // iced 的 Shadow 没有 spread，用「模糊半径 + 偏移」近似；负 spread 的收缩效果
        // 只能靠调小 blur 找回来，这里 40 → 30 是等价观感的经验值。
        shadow_card: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.8),
            offset: Vector::new(0.0, 14.0),
            blur_radius: 30.0,
        },
    };

    /// 对应 `:root[data-theme="light"]`，阶段 0 未上屏，仅确认类型可写成常量。
    #[allow(dead_code)]
    pub const LIGHT: Self = Self {
        bg_app: Color::from_rgb(0.953, 0.957, 0.98),
        surface_1: Color::WHITE,
        surface_2: Color::from_rgb(0.969, 0.973, 0.992),
        border: Color::from_rgba(0.118, 0.137, 0.314, 0.07),
        border_mid: Color::from_rgba(0.118, 0.137, 0.314, 0.11),
        text: Color::from_rgb(0.098, 0.106, 0.18),
        text_2: Color::from_rgb(0.337, 0.357, 0.471),
        text_3: Color::from_rgb(0.545, 0.565, 0.671),
        accent: Color::from_rgb(0.318, 0.376, 0.918),
        accent_soft: Color::from_rgba(0.318, 0.376, 0.918, 0.10),
        input_bg: Color::WHITE,
        shadow_card: Shadow {
            color: Color::from_rgba(0.094, 0.118, 0.314, 0.14),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        },
    };
}
