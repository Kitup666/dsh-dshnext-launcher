//! 切页入场包装件（DESIGN.md §7.7）。
//!
//! 只做两件事，都在 `draw` 里：把内容**整体下移**一点，再盖一层**背景色面纱**。
//! 两者由同一个补间驱动（`anim::PAGE`，1 = 刚切过来，0 = 已落定）。
//!
//! 为什么不做别的：
//! - **不动 padding/height**，否则每帧都要重新布局，长页面（设置页六张卡）
//!   要重排整棵树；平移只影响 `draw`，布局结果逐帧完全复用。
//! - **不用 `Transformation::scale`**：缩放会连文字一起缩，cosmic-text 在非整数
//!   缩放下要么重新栅格化（每帧一次，白扔性能）要么拉伸图集（更糊）。字已经够糊了。
//! - **淡入只能靠面纱**：iced 0.14 的 `renderer::Style` 只有 `text_color`，没有
//!   全局 opacity，wgpu 后端也没有「把一层画进纹理再整体调 alpha」的入口。
//!   模态遮罩用的就是同一招（`bg_app` 半透明叠加），已过视觉验收。
//!
//! 这个 widget 在树里是**透明的**：tag/state/children/layout 全部直通内容
//! （抄 `iced_widget::themer` 的做法），所以补间值变化不会引起任何树 diff。

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{Background, Color, Element, Event, Length, Rectangle, Size, Vector};

/// 包一层入场动画。`t` 是剩余进度（1 = 刚切过来、0 = 落定），`shift` 是起始下移
/// 距离，`veil` 是面纱色（用 `bg_app`），`veil_max` 是面纱最大不透明度。
pub fn reveal<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    t: f32,
    shift: f32,
    veil: Color,
    veil_max: f32,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Reveal {
        content: content.into(),
        t: t.clamp(0.0, 1.0),
        shift,
        veil,
        veil_max,
    })
}

struct Reveal<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    t: f32,
    shift: f32,
    veil: Color,
    veil_max: f32,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Reveal<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        // 事件按**未平移**的布局派发：动画只有 190ms，期间光标命中差十几像素
        // 无人可辨；换成跟着平移会让「按下时元素还在动」的判定与 layout 不一致。
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        // 落定态走原路：不推变换、不画面纱，和没包这层完全一样。
        if self.t <= f32::EPSILON {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
            return;
        }

        renderer.with_translation(Vector::new(0.0, self.shift * self.t), |renderer| {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        });

        // 面纱**必须自己开一层**。两个后端在同一层里都是「先画所有 quad，再画所有
        // 文字」（wgpu 见 lib.rs 的 render 循环，tiny-skia 同序），面纱如果落在内容
        // 那一层，文字会盖在它上面——底色淡入而文字全程清晰，比不做动画更怪。
        // `with_layer` 走 `push_clip`，新层的序号一定更大，于是稳定地画在内容之后。
        //
        // 盖的是**未平移**的整块区域：内容下移后会探出布局边界，按未平移的 bounds
        // 盖才能同时罩住两个位置。
        let bounds = layout.bounds();
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..Default::default()
                },
                Background::Color(Color {
                    a: self.veil.a * self.veil_max * self.t,
                    ..self.veil
                }),
            );
        });
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}
