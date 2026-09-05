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
/// `R_HERO` 是本代新增的一档：英雄区比普通卡片高一级，圆角也得跟着走
/// （一套圆角用到底会让所有盒子看着同等重要）。
pub const R_CARD: f32 = 16.0;
pub const R_HERO: f32 = 20.0;
pub const R_CTL: f32 = 10.0;
pub const R_PILL: f32 = 999.0;

/// 字号阶梯。上一代 CSS 以 14px 为根、用 rem 写，直译过来散出 9/9.5/10/11/11.6/
/// 12/12.5/13/13.5/13.7 十档——相邻两档差 0.1~0.5px，肉眼分不出层级，只显得乱。
/// 这里收拢成八档并**只许用这些常量**：改一处大小就是改一层语义，不是调一个数。
pub const FS_MICRO: f32 = 10.0; // 徽标、meta 小标签、日志时间戳
pub const FS_TINY: f32 = 11.5; // 辅助说明、路径、小按钮
pub const FS_SMALL: f32 = 12.5; // 表单标签、卡片副标题、输入框
pub const FS_BODY: f32 = 13.0; // 正文、列表主行
pub const FS_TITLE: f32 = 13.5; // 卡片标题、hero 按钮
pub const FS_LEAD: f32 = 15.0; // 模态标题
pub const FS_HEAD: f32 = 19.0; // 页标题
/// hero 大标题。上一代 CSS 是 `48px * .62`（`.hero h1` 的 clamp 折中值），
/// 直接写成 30 免得读代码的人去推那个乘法。**注意 CSS 还有 `letter-spacing: -1.2px`
/// 而 iced 0.14 的 Text 没有 letter_spacing API**（同 tnum 一类的限制，
/// 见 DESIGN.md §12），负字距只能放弃——这也是它看着比上一代略宽的原因。
pub const FS_HERO: f32 = 30.0;

/// 纵向节奏（px，都在 8 的倍数上）。上一代六个页面一律用 18 把页头和各卡片
/// 等距排开，于是「页头 → 内容」和「卡片 → 卡片」看着是同一层关系，读不出层级。
/// 现在分两档：页头之后留 `GAP_SECTION`，同级卡片之间留 `GAP_CARD`。
pub const GAP_CARD: f32 = 16.0;
pub const GAP_SECTION: f32 = 24.0;

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
    /// 卡片 1px 描边。暗色靠它勾轮廓（黑底上黑阴影不可见，借鉴 orevx glass-dark）；
    /// 亮色为 transparent，维持上一代「白卡 + 阴影浮起」的设计。
    pub card_border: Color,

    pub text: Color,
    pub text_2: Color,
    pub text_3: Color,

    pub accent: Color,
    pub accent_hi: Color,
    pub accent_soft: Color,
    /// 列表选中行的底色。**必须是不透明色**：iced 关掉 `web-colors` 后按物理
    /// （线性空间）混色，深底上叠一点饱和蓝会被放大得离谱——实测 5% accent
    /// 叠在 #18181a 上得到 (31,36,69)，蓝通道从 26 冲到 69，整行盖过行内按钮。
    /// 半透明叠色只适合中性灰（hover），带色相的一律写死。
    pub row_selected: Color,
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

    /// 环境光对角渐变（左上辉 → 右下回暖，借鉴 orevx 但改斜角）。
    /// 靛紫系而不是纯蓝——蓝紫过渡比蓝青更「贵」。
    /// 亮色为 transparent（无环境光）。
    pub ambient_top: Color,
    pub ambient_mid: Color,
    /// 渐变中段收净后，右下角再回暖一档深海军蓝（双辉）。
    pub ambient_bot: Color,

    /// 卡片浮起：CSS `0 20px 40px -24px`。iced 的 Shadow 没有 spread，
    /// 负 spread 的收缩效果靠调小 blur 找回（40 → 30，阶段 0 验证观感等价）。
    pub shadow_card: Shadow,
    /// 英雄区：比 `shadow_card` 深一档、扩散更远。**不复用 `shadow_pop`**——
    /// 那是浮层（模态/下拉）的量，用在常驻内容上会显得整块要脱离页面。
    pub shadow_hero: Shadow,
    /// 弹层：CSS `0 24px 60px -12px` → blur 48 / dy 18。
    pub shadow_pop: Shadow,
    /// 主按钮：CSS `0 6px 18px -8px accent/.5` → blur 14 / dy 5。
    pub shadow_btn: Shadow,
    /// 次级按钮的极浅投影：CSS `0 1px 2px`。
    pub shadow_ctl: Shadow,

    pub input_bg: Color,

    /// 控制台日志视口的底色。刻意比 `input_bg` 亮一档：纯黑在深底卡片里像
    /// 「渲染破洞」而非终端面板（judge P1）。
    pub bg_log: Color,
}

impl Palette {
    /// 暗色：借鉴 orevx glass-dark 的色阶关系（oklch 值已转 sRGB 硬编码）。
    /// 与上一代 `#060607` 底相比：背景几乎不变，**surface 提亮一档**
    /// （#0e0e10→#18181a），卡片靠色阶差 + 1px 白描边浮起而非黑阴影
    /// ——纯黑底上黑阴影本来就不可见，这是 orevx 好看的第一原因。
    /// 顶部再加一道蓝→青→透明的环境光渐变（ambient_top/mid）。
    pub const DARK: Self = Self {
        bg_app: rgb!(0x050606),
        bg_side: rgb!(0x050606),
        surface_1: rgb!(0x18181a),
        surface_2: rgb!(0x232325),
        surface_3: rgb!(0x262728),
        hover: rgba!(0xffffff, 0.04),

        border: rgba!(0xffffff, 0.06),
        border_mid: rgba!(0xffffff, 0.09),
        border_hi: rgba!(0xffffff, 0.20),
        card_border: rgba!(0xffffff, 0.05),

        text: rgb!(0xf2f2f4),
        text_2: rgb!(0x9a9ca6),
        text_3: rgb!(0x82848f),

        accent: rgb!(0x4a63e0),
        accent_hi: rgb!(0x4f68e8),
        accent_soft: rgba!(0x4a63e0, 0.14),
        row_selected: rgb!(0x20222e),
        accent_line: rgba!(0x4a63e0, 0.40),
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

        // 环境光双球：峰值 alpha 在径向 Mesh 上做线性衰减，用户定过 -30%
        // （0.17→0.12 / 0.13→0.09），嫌亮再动这里。
        ambient_top: rgba!(0x4f46e5, 0.12),
        ambient_mid: rgba!(0x7c5cd6, 0.05),
        ambient_bot: rgba!(0x1e3a8a, 0.09),

        // 暗色阴影弱化：轮廓交给 card_border，阴影只留一点深度感。
        shadow_card: Shadow {
            color: rgba!(0x000000, 0.45),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 24.0,
        },
        shadow_hero: Shadow {
            color: rgba!(0x000000, 0.55),
            offset: Vector::new(0.0, 14.0),
            blur_radius: 34.0,
        },
        shadow_pop: Shadow {
            color: rgba!(0x000000, 0.70),
            offset: Vector::new(0.0, 18.0),
            blur_radius: 48.0,
        },
        shadow_btn: Shadow {
            color: rgba!(0x4a63e0, 0.50),
            offset: Vector::new(0.0, 5.0),
            blur_radius: 14.0,
        },
        shadow_ctl: Shadow {
            color: rgba!(0x000000, 0.35),
            offset: Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },

        input_bg: rgb!(0x0a0a0c),
        bg_log: rgb!(0x101014),
    };

    /// 对应 `:root[data-theme="light"]`：淡紫灰底 #f3f4fa + 白卡浮起。
    /// 亮色保持上一代设计（阴影浮起、无描边、无环境光），不跟 orevx 改。
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
        card_border: Color::TRANSPARENT,

        text: rgb!(0x191b2e),
        text_2: rgb!(0x565b78),
        text_3: rgb!(0x6b7091),

        accent: rgb!(0x5160ea),
        accent_hi: rgb!(0x5262dd),
        accent_soft: rgba!(0x5160ea, 0.10),
        row_selected: rgb!(0xeef0fb),
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

        ambient_top: Color::TRANSPARENT,
        ambient_mid: Color::TRANSPARENT,
        ambient_bot: Color::TRANSPARENT,

        shadow_card: Shadow {
            color: rgba!(0x181e50, 0.14),
            offset: Vector::new(0.0, 10.0),
            blur_radius: 28.0,
        },
        shadow_hero: Shadow {
            color: rgba!(0x181e50, 0.18),
            offset: Vector::new(0.0, 14.0),
            blur_radius: 38.0,
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
        bg_log: rgb!(0xf2f3fa),
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
