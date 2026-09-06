//! 切页入场包装件（DESIGN.md §7.7）。
//!
//! 只做一件事，在 `draw` 里：把内容**整体上移落位**（起始时下移 `shift` 像素）。
//! 由 `anim::PAGE` 补间驱动（1 = 刚切过来，0 = 已落定）。
//!
//! 为什么不做别的：
//! - **不动 padding/height**，否则每帧都要重新布局，长页面（设置页六张卡）
//!   要重排整棵树；平移只影响 `draw`，布局结果逐帧完全复用。
//! - **不用 `Transformation::scale`**：缩放会连文字一起缩，cosmic-text 在非整数
//!   缩放下要么重新栅格化（每帧一次，白扔性能）要么拉伸图集（更糊）。字已经够糊了。
//! - **不做面纱淡入**（曾实现过，用户否了）：面纱盖的是整块区域，主区实际背景比
//!   `bg_app` 亮，盖上去背景肉眼可见地变暗，观感像闪屏。「淡入只能靠面纱」这个
//!   技术结论仍然成立（iced 0.14 没有全局 opacity），但代价是背景被染色——
//!   位移单独用就够了。
//!
//! 这个 widget 在树里是**透明的**：tag/state/children/layout 全部直通内容
//! （抄 `iced_widget::themer` 的做法），所以补间值变化不会引起任何树 diff。

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// 包一层入场动画。`t` 是剩余进度（1 = 刚切过来、0 = 落定），`shift` 是起始下移距离。
#[allow(dead_code)]
pub fn reveal<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    t: f32,
    shift: f32,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    reveal_at(content, t, shift, 0.0)
}

/// 错峰版：`delay` ∈ [0,1) 是整段入场时长里这张卡**迟到**的比例。
/// 有效进度 `(t-delay)/(1-delay)` —— delay=0 与 `reveal` 完全一致，
/// delay=0.3 的卡在总时长的后 70% 里走完自己的位移（motion-designer 的
/// stagger 纪律：总错峰封顶 ~400ms，后面的卡走得快一点才不会拖沓）。
pub fn reveal_at<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    t: f32,
    shift: f32,
    delay: f32,
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
        delay: delay.clamp(0.0, 0.99),
    })
}

struct Reveal<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    t: f32,
    shift: f32,
    delay: f32,
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
        // 落定态走原路：不推变换，和没包这层完全一样。
        let te = ((self.t - self.delay) / (1.0 - self.delay)).clamp(0.0, 1.0);
        if te <= f32::EPSILON {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
            return;
        }

        renderer.with_translation(Vector::new(0.0, self.shift * te), |renderer| {
            self.content
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
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
