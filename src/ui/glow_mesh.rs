//! 径向球圆光晕：iced 0.14 没有径向渐变（gradient.rs 标 TBD），用
//! `Mesh::Solid` 扇形网格模拟——中心一个亮顶点，外圈 N 个全透明顶点，
//! GPU 对顶点色做线性插值，出来的就是从圆心向外线性衰减的真圆光晕。
//! 公开 API（`graphics::mesh::Renderer::draw_mesh`，wgpu / tiny-skia /
//! fallback renderer 都实现了），不碰自定义 shader 管线。
//!
//! 每帧重建的是 ~130 顶点的小扇形（CPU 侧一个 Vec），GPU 缓存按内容走，
//! 15fps 慢漂移下开销可忽略。

use iced::advanced::graphics::mesh::{Indexed, Mesh, SolidVertex2D};
use iced::advanced::{layout, mouse, renderer, widget::Tree, Layout, Widget};
use iced::{Color, Element, Length, Point, Rectangle, Size};

/// 扇形细分数：圆周顶点数。64 段足够圆，再多肉眼分不出。
const SEGMENTS: usize = 64;

/// 一团光晕的参数（由环境光漂移时钟驱动）。
#[derive(Clone, Copy)]
pub struct GlowSpec {
    /// 圆心在 widget bounds 内的像素坐标（可在窗外，窗外顶点自然被裁）。
    pub center: Point,
    /// 光晕半径（逻辑像素）。
    pub radius: f32,
    /// 圆心色（alpha 即峰值强度）。
    pub color: Color,
}

/// 全窗画布上画径向光晕。透明、不吃任何事件、布局上只想占 Fill。
pub fn glow_layer<Message: 'static>(specs: Vec<GlowSpec>) -> Element<'static, Message> {
    Glow { specs }.into()
}

struct Glow {
    specs: Vec<GlowSpec>,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Glow
where
    Message: 'static,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // 占满可用空间（Fill 的语义自己实现）：limits 的最大尺寸即父级
        // 分给 Fill 子项的空间。
        let size = limits.max();
        layout::Node::new(size)
    }

    fn draw(
        &self,
        _state: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        for spec in &self.specs {
            renderer.draw_mesh(build_orb(spec.center, spec.radius, spec.color, bounds));
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

/// SolidVertex2D 要的是 linear RGBA（不开 web-colors 时 iced 按物理混色）。
fn mesh_pack(c: Color) -> iced::advanced::graphics::color::Packed {
    iced::advanced::graphics::color::pack(c)
}

/// 建一颗光球的扇形网格。`center` 是**局部**坐标（相对调用方 bounds 左上角），
/// `clip` 传调用方的 bounds。圆心在窗外时外圈顶点跟着出去，裁剪自然发生。
pub fn build_orb(center: Point, radius: f32, color: Color, clip: Rectangle) -> Mesh {
    let (cx, cy) = (center.x, center.y);
    let edge = Color { a: 0.0, ..color };

    // 圆心顶点（峰值色）+ SEGMENTS+1 个外圈顶点（全透明，首尾闭合）
    let mut vertices: Vec<SolidVertex2D> = Vec::with_capacity(SEGMENTS + 2);
    let mut indices: Vec<u32> = Vec::with_capacity(SEGMENTS * 3);

    vertices.push(SolidVertex2D {
        position: [cx, cy],
        color: mesh_pack(color),
    });

    for i in 0..=SEGMENTS {
        let t = i as f32 / SEGMENTS as f32;
        let (sin, cos) = (t * std::f32::consts::TAU).sin_cos();
        vertices.push(SolidVertex2D {
            position: [cx + cos * radius, cy + sin * radius],
            color: mesh_pack(edge),
        });
        if i > 0 {
            // 扇形：圆心(0) + 上一个外圈(i) + 当前外圈(i+1)
            indices.extend_from_slice(&[0, i as u32, i as u32 + 1]);
        }
    }

    Mesh::Solid {
        buffers: Indexed { vertices, indices },
        // 顶点已是局部坐标；当前 layer 的 transformation 由 draw_mesh 自乘
        // （layer.rs draw_mesh 负责叠加），恒等即可。
        transformation: iced::Transformation::IDENTITY,
        clip_bounds: clip,
    }
}

/// 环境光双球的冻结参数（**窗口逻辑坐标** + 已含冻结相位脉动的颜色）：
/// 背景 ambient() 与 Frosted 的伪造 backdrop 共用这一份——卡内画的是
/// 同一对光球的柔化版，颜色随位置对应窗外背景，磨砂的「透」就是这么来的。
pub fn ambient_specs(pal: &crate::theme::Palette) -> [(Point, f32, Color); 2] {
    let clock = FROZEN_CLOCK;
    let s = std::f32::consts::TAU;
    let top_pulse = 1.0 + 0.15 * (clock * s / 16.0).sin();
    let bot_pulse = 1.0 + 0.15 * (clock * s / 21.0 + 2.0).sin();
    let top = crate::theme::with_alpha(pal.ambient_top, (pal.ambient_top.a * top_pulse).min(1.0));
    let bot = crate::theme::with_alpha(pal.ambient_bot, (pal.ambient_bot.a * bot_pulse).min(1.0));
    [
        (
            Point::new(
                40.0 + 26.0 * (clock * s / 31.0).sin(),
                30.0 + 18.0 * (clock * s / 23.0 + 0.8).sin(),
            ),
            520.0 * (1.0 + 0.05 * (clock * s / 19.0).sin()),
            top,
        ),
        (
            Point::new(
                1240.0 + 30.0 * (clock * s / 37.0 + 2.1).sin(),
                830.0 + 20.0 * (clock * s / 29.0 + 1.4).sin(),
            ),
            480.0 * (1.0 + 0.05 * (clock * s / 23.0 + 1.0).sin()),
            bot,
        ),
    ]
}

/// 冻结相位：漂移版跑到约 40s 时的观感。pages::ambient 与本文件共用。
pub const FROZEN_CLOCK: f32 = 40.0;

impl<Message> From<Glow> for Element<'static, Message>
where
    Message: 'static,
{
    fn from(glow: Glow) -> Self {
        Element::new(glow)
    }
}

// mesh::Renderer 只在 advanced::graphics 里可达，这个 use 是给 draw 里
// `renderer.draw_mesh` 的方法解析用的（trait 在作用域内才找得到方法）。
use iced::advanced::graphics::mesh::Renderer as _MeshRenderer;

// 未用但保持导出的引用（模块自洽）
#[allow(dead_code)]
fn _bounds_helper(p: Point, b: &Rectangle) -> Point {
    Point::new(p.x + b.x, p.y + b.y)
}

// Size/unused import 约束占位（编译器要求的显式 use）
const _: fn(Size) -> Size = |s| s;
