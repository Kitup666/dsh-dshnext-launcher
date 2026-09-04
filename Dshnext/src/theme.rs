//! 设计令牌。色值逐条抄自 `../../src/styles.css` 的 `:root[data-theme]` 两组，
//! 保证与上一代观感一致（DESIGN.md §6）。暗色那套的阴影近似值在阶段 0 验证过。
//!
//! 纪律：
//! - `Palette` 必须 `Copy` 按值传（`&Palette` 会撞 `view()` 返回 `Element<'_>` 的生命周期）。
//! - 常量用 `rgb!`/`rgba!` 宏从十六进制展开，避免手算浮点出错。

use iced::{Color, Shadow, Vector};

/// `#rrggbb` → Color。const 上下文可用（浮点四则运算是稳定的）。
macro_rules! rgb {
    ($hex:expr) => {{
        let h: u32 = $hex;
        Color::from_rgb(
            ((h >> 16) & 0xff) as f32 / 255.0,
            ((h >> 8) & 0xff) as f32 / 255.0,
            (h & 0xff) as f32 / 255.0,
        )
    }};
}

/// `#rrggbb` + alpha → Color。
macro_rules! rgba {
    ($hex:expr, $a:expr) => {{
        let h: u32 = $hex;
        Color::from_rgba(
            ((h >> 16) & 0xff) as f32 / 255.0,
            ((h >> 8) & 0xff) as f32 / 255.0,
            (h & 0xff) as f32 / 255.0,
            $a,
        )
    }};
}

/// 圆角令牌（px），对应 CSS `--r-card` / `--r-ctl` / `--r-pill`。
pub const R_CARD: f32 = 16.0;
pub const R_CTL: f32 = 10.0;
pub const R_PILL: f32 = 999.0;

/// 完整令牌集。阶段 1 只用到一部分（input_bg/warn/surface_3 等是阶段 3 表单页的），
/// 整套先照抄齐，避免后面逐条回去翻 CSS。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_app: Color,
    pub bg_side: Color,
    pub surface_1: Color,
    pub surface_2: Color,
    pub surface_3: Color,
    /// hover 叠加色（半透明），用 `lerp(TRANSPARENT, hover, t)` 做过渡。
    pub hover: Color,

    pub border: Color,
    pub border_mid: Color,
    pub border_hi: Color,

    pub text: Color,
    pub text_2: Color,
    pub text_3: Color,

    pub accent: Color,
    pub accent_hi: Color,
    pub accent_soft: Color,
    pub accent_line: Color,
    pub on_accent: Color,

    pub teal: Color,
    pub teal_soft: Color,
    /// 对应 CSS `.btn-teal` 的 `color: #053028`。
    pub on_teal: Color,
    pub ok: Color,
    pub ok_soft: Color,
    pub warn: Color,
    pub warn_soft: Color,
    pub bad: Color,
    pub bad_soft: Color,

    /// 卡片浮起：CSS `0 20px 40px -24px`。iced 的 Shadow 没有 spread，
    /// 负 spread 的收缩效果靠调小 blur 找回（40 → 30，阶段 0 验证观感等价）。
    pub shadow_card: Shadow,
    /// 弹层：CSS `0 24px 60px -12px` → blur 48 / dy 18。
    pub shadow_pop: Shadow,
    /// 主按钮：CSS `0 6px 18px -8px accent/.5` → blur 14 / dy 5。
    pub shadow_btn: Shadow,
    /// 次级按钮的极浅投影：CSS `0 1px 2px`。
    pub shadow_ctl: Shadow,

    pub input_bg: Color,
}

impl Palette {
    /// 对应 `:root[data-theme="dark"]`：纯黑底 #060607、卡 #0e0e10、强调 #5b76ff。
    pub const DARK: Self = Self {
        bg_app: rgb!(0x060607),
        bg_side: rgb!(0x060607),
        surface_1: rgb!(0x0e0e10),
        surface_2: rgb!(0x161619),
        surface_3: rgb!(0x1d1d21),
        hover: rgba!(0xffffff, 0.04),

        border: rgba!(0xffffff, 0.06),
        border_mid: rgba!(0xffffff, 0.10),
        border_hi: rgba!(0xffffff, 0.18),

        text: rgb!(0xf2f2f4),
        text_2: rgb!(0x9a9ca6),
        text_3: rgb!(0x5c5e68),

        accent: rgb!(0x5b76ff),
        accent_hi: rgb!(0x7d92ff),
        accent_soft: rgba!(0x5b76ff, 0.14),
        accent_line: rgba!(0x5b76ff, 0.40),
        on_accent: Color::WHITE,

        teal: rgb!(0x2fd6b3),
        teal_soft: rgba!(0x2fd6b3, 0.13),
        on_teal: rgb!(0x053028),
        ok: rgb!(0x3ecf8e),
        ok_soft: rgba!(0x3ecf8e, 0.13),
        warn: rgb!(0xf5a524),
        warn_soft: rgba!(0xf5a524, 0.13),
        bad: rgb!(0xf0616d),
        bad_soft: rgba!(0xf0616d, 0.13),

        shadow_card: Shadow {
            color: rgba!(0x000000, 0.80),
            offset: Vector::new(0.0, 14.0),
            blur_radius: 30.0,
        },
        shadow_pop: Shadow {
            color: rgba!(0x000000, 0.70),
            offset: Vector::new(0.0, 18.0),
            blur_radius: 48.0,
        },
        shadow_btn: Shadow {
            color: rgba!(0x5b76ff, 0.50),
            offset: Vector::new(0.0, 5.0),
            blur_radius: 14.0,
        },
        shadow_ctl: Shadow {
            color: rgba!(0x000000, 0.35),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },

        input_bg: rgb!(0x0a0a0c),
    };

    /// 对应 `:root[data-theme="light"]`：淡紫灰底 #f3f4fa + 白卡浮起。
    pub const LIGHT: Self = Self {
        bg_app: rgb!(0xf3f4fa),
        bg_side: rgb!(0xf3f4fa),
        surface_1: Color::WHITE,
        surface_2: rgb!(0xf7f8fd),
        surface_3: rgb!(0xeef0f9),
        hover: rgba!(0x28306e, 0.04),

        border: rgba!(0x1e2350, 0.07),
        border_mid: rgba!(0x1e2350, 0.11),
        border_hi: rgba!(0x1e2350, 0.20),

        text: rgb!(0x191b2e),
        text_2: rgb!(0x565b78),
        text_3: rgb!(0x8b90ab),

        accent: rgb!(0x5160ea),
        accent_hi: rgb!(0x6a77f2),
        accent_soft: rgba!(0x5160ea, 0.10),
        accent_line: rgba!(0x5160ea, 0.35),
        on_accent: Color::WHITE,

        teal: rgb!(0x0fb898),
        teal_soft: rgba!(0x0fb898, 0.12),
        on_teal: rgb!(0x053028),
        ok: rgb!(0x12a06a),
        ok_soft: rgba!(0x12a06a, 0.10),
        warn: rgb!(0xcf8607),
        warn_soft: rgba!(0xcf8607, 0.10),
        bad: rgb!(0xdd4257),
        bad_soft: rgba!(0xdd4257, 0.09),

        shadow_card: Shadow {
            color: rgba!(0x181e50, 0.14),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        },
        shadow_pop: Shadow {
            color: rgba!(0x181e50, 0.22),
            offset: Vector::new(0.0, 16.0),
            blur_radius: 48.0,
        },
        shadow_btn: Shadow {
            color: rgba!(0x5160ea, 0.45),
            offset: Vector::new(0.0, 6.0),
            blur_radius: 16.0,
        },
        shadow_ctl: Shadow {
            color: rgba!(0x14183c, 0.12),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },

        input_bg: Color::WHITE,
    };
}

// ---- 颜色工具：CSS filter: brightness() / color-mix() 的等价物 ----

/// 线性插值（含 alpha）。hover 过渡全靠它。
pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// 等价 CSS `filter: brightness(f)`：RGB 乘系数，alpha 不动。
pub fn brighten(c: Color, f: f32) -> Color {
    Color {
        r: (c.r * f).min(1.0),
        g: (c.g * f).min(1.0),
        b: (c.b * f).min(1.0),
        a: c.a,
    }
}

/// 覆盖 alpha。
pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// 等价 CSS `color-mix(in srgb, c w%, white)`。
pub fn mix_white(c: Color, w: f32) -> Color {
    lerp(c, Color::WHITE, w)
}
