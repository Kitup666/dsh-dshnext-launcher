//! 假高斯模糊卡片容器：布局完全跟随内容（glass 底 + 颗粒贴卡面 + 内容在上）。
//!
//! 为什么不用 `container(stack![grain, content])` 拼：stack 里 Fill×Fill 的
//! 颗粒图会把栈撑到**剩余空间**（卡片只占顶部，颗粒矩形漏到卡片下面——
//! 用户截图抓的正是这个），iced 没有「随兄弟尺寸」的图元长度，只能自己写
//! 布局跟随内容的 widget。
//!
//! 绘制顺序在同一个 layer 里靠 with_layer 保序：glass 底 quad → 光球+颗粒
//! 共享场（窗口锚定，卡片只是裁剪窗）→ 内容（按钮/下拉/文字全部在
//! 颗粒之上，不受污染）。pick_list 弹层走 overlay 委托，不受影响。

use iced::advanced::image::Renderer as _;
use iced::advanced::renderer::Renderer as _LayerRenderer;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{
    image, layout, mouse, overlay, renderer, Clipboard, Layout, Shell, Widget,
};
use iced_renderer::fallback::Renderer as Fallback;
use iced::widget::container;
use iced::{Background, Border, Color, Element, Gradient, Length, Padding, Point, Rectangle, Size, Vector};

// ── 滚动对齐 ─────────────────────────────────────────────────────────────
// 卡片的 bounds.pos 是「滚动内容空间」坐标：scrollable 用
// with_translation(-offset) 画内容，卡片窗口位置 = 原点 + pos - 平移，
// 而 backdrop 光球/颗粒场必须钉在窗口上（窗外球不动）。
//
// 平移量不另存全局：scrollable 传给内容子树的 viewport.pos = 主区原点 +
// 平移（容器链默认不裁剪 viewport，reveal 也原样透传），draw 时直接反推。
// 切页、滚动钳制、改布局都天然自洽，不会出现全局状态与 widget 状态脱节。
//
// 主滚动区的窗口原点（逻辑 px）：侧边栏 SIDEBAR_W + 1px 分隔线，标题栏 TITLEBAR_H。
// 细标题栏（TITLEBAR_H=40）只盖主区上方，侧边栏（含独立品牌头部单元）在它左边。
// 两维都引用 titlebar 常量，改布局尺寸这里自动跟随，不双处硬编码。
const MAIN_ORIGIN: (f32, f32) = (
    crate::ui::titlebar::SIDEBAR_W + 1.0,
    crate::ui::titlebar::TITLEBAR_H,
);

// 颗粒场：GRAIN_ROWS×GRAIN_COLS 张瓦片铺窗口逻辑坐标，瓦片 1:1 贴逻辑像素。
// 2048×1376 覆盖本机最大窗口（2560×1600 物理 @125% = 2048×1280 逻辑）。
const GRAIN_TILE: (f32, f32) = (1024.0, 688.0);
const GRAIN_ROWS: usize = 2;
const GRAIN_COLS: usize = 2;

/// 磨砂卡片。`grain <= 0.0`（亮色主题）时只画 glass 底，零图片采样。
pub struct Frosted<'a, Message> {
    content: Element<'a, Message>,
    width: Length,
    height: Length,
    padding: Padding,
    /// 构造时就定死的容器样式（pal 是 'static，不必留闭包）。
    style: container::Style,
    /// 颗粒不透明度（theme 令牌 `grain`，0 = 不画）。
    grain: f32,
    /// 伪造 backdrop-blur 的光球（窗口坐标 + 已含脉冲的颜色，来自
    /// `glow_mesh::ambient_specs`）。None = 亮色主题，跳过整层。
    backdrop: Option<[(Point, f32, Color); 2]>,
}

impl<'a, Message> Frosted<'a, Message> {
    /// 内容画在最上层：with_layer 开新层，层序号更大、稳定后画
    /// （AGENTS.md 坑 23 的结论），按钮/输入框/文字都不吃颗粒。
    fn draw_content(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        clipped: Rectangle,
    ) {
        let child = layout.children().next().unwrap();
        // 层边界必须用**裁剪后**的矩形：iced 0.14 的 push_clip 不与父裁剪
        // 求交（AGENTS.md 坑 31），传 bounds 会逃出 scrollable 的裁剪层——
        // 滚动时卡面/内容盖过标题栏带（用户抓的 bug）。
        renderer.with_layer(clipped, |renderer| {
            self.content.as_widget().draw(
                tree,
                renderer,
                theme,
                style,
                child,
                cursor,
                &clipped,
            );
        });
    }

    pub fn new(
        content: impl Into<Element<'a, Message>>,
        style: container::Style,
        padding: impl Into<Padding>,
        grain: f32,
        backdrop: Option<[(Point, f32, Color); 2]>,
    ) -> Self {
        Self {
            content: content.into(),
            width: Length::Fill,
            height: Length::Shrink,
            padding: padding.into(),
            style,
            grain,
            backdrop,
        }
    }

    /// 覆盖高度（默认 Shrink 跟随内容；控制台等满高卡用 Fill）。
    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Frosted<'_, Message>
where
    Message: 'static,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // 直接复用 container 的布局逻辑（Fill 宽 + Shrink 高 + padding + 内容定位）
        iced::widget::container::layout(
            limits,
            self.width,
            self.height,
            f32::INFINITY,
            f32::INFINITY,
            self.padding,
            iced::alignment::Horizontal::Left,
            iced::alignment::Vertical::Top,
            |limits| self.content.as_widget_mut().layout(tree, renderer, limits),
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(clipped) = bounds.intersection(viewport) else {
            return;
        };

        // 暗色 + wgpu 后端：卡面/光球/颗粒/描边/圆角全部收进玻璃 shader
        // （glass_pipeline），基层只留阴影。场是解析求值的，卡片矩形由
        // iced 层变换（滚动/reveal）自动带到位，不需要本文件里的
        // viewport 反推。tiny-skia 与亮色主题走下面旧路径。
        if self.backdrop.is_some() {
            if let Fallback::Primary(r) = renderer {
                // 只有阴影的 quad：background 置 None、描边归零（描边由
                // shader 画，quad 的边会被不透明卡面盖住）
                let shadow_style = container::Style {
                    text_color: self.style.text_color,
                    background: None,
                    border: Border {
                        color: self.style.border.color,
                        width: 0.0,
                        radius: self.style.border.radius,
                    },
                    shadow: self.style.shadow,
                    snap: self.style.snap,
                };
                iced::widget::container::draw_background(r, &shadow_style, bounds);

                r.with_layer(clipped, |r| {
                    use iced_wgpu::primitive::Renderer as _;
                    if let Some(glass) = extract_glass(&self.style, self.grain) {
                        r.draw_primitive(
                            clipped,
                            crate::ui::glass_pipeline::GlassQuad::card(
                                glass,
                                self.backdrop.unwrap(),
                            ),
                        );
                    }
                });

                self.draw_content(tree, renderer, theme, style, layout, cursor, clipped);
                return;
            }
        }

        // 1. 不透明底 + 描边 + 阴影。磨砂的「透」不靠真半透明（底下没东西
        //    可透，只有噪声感），靠第 2 步把背景光球柔化后画进卡内——
        //    参考用户给的 glassmorphism shader：col = mix(背景, 磨砂色, mask)。
        iced::widget::container::draw_background(renderer, &self.style, bounds);

        // 2. 伪造 backdrop-blur：**与背景完全同参**的光球（高斯模糊不位移，
        //    半径/浓度都必须和窗外一致——用户指出「晕染和背景对不上」的根因
        //    就是之前的 ×1.5/×0.8 柔化偏移），圆心换算到卡内局部坐标，整层
        //    with_layer 裁剪在卡内。卡内颜色 = 窗外该位置的本底 + 光球，玻璃
        //    的磨砂感由底色 sheen 渐变 + 颗粒承担。
        //    光球 mesh 和颗粒 image 放同一层：同层 pass 序 mesh→image 正好
        //    是我们想要的叠放；内容再开一层，绝不与它们同层。
        if self.grain > 0.0 || self.backdrop.is_some() {
            // 层边界用裁剪后矩形——嵌套层不继承 scrollable 的裁剪（坑 31）。
            renderer.with_layer(clipped, |renderer| {
                // 平移量由 viewport 反推（见模块顶注释）：viewport.pos =
                // 主区原点 + 滚动平移 → 卡片窗口位置 = 原点 + bounds.pos - 平移。
                let (ox, oy) = MAIN_ORIGIN;
                let tx = viewport.x - ox;
                let ty = viewport.y - oy;
                let wx = ox + bounds.x - tx;
                let wy = oy + bounds.y - ty;
                if let Some(specs) = self.backdrop {
                    use iced::advanced::graphics::mesh::Renderer as _;
                    for (window_center, radius, color) in specs {
                        // 切页 reveal 的 ~300ms 位移动画不参与换算，过渡瞬间
                        // 卡内球会短暂错位，落定即恢复——可接受。
                        let local = Point::new(window_center.x - wx, window_center.y - wy);
                        renderer.draw_mesh(crate::ui::glow_mesh::build_orb(
                            local,
                            radius,
                            color,
                            clipped,
                        ));
                    }
                }
                if self.grain > 0.0 {
                    static HANDLE: std::sync::OnceLock<iced::advanced::image::Handle> =
                        std::sync::OnceLock::new();
                    let handle = HANDLE.get_or_init(|| {
                        iced::advanced::image::Handle::from_bytes(
                            include_bytes!("../../assets/textures/grain.png").to_vec(),
                        )
                    });
                    let img = image::Image::new(handle.clone())
                        .opacity(self.grain)
                        .filter_method(image::FilterMethod::Linear);
                    // 颗粒场是**窗口锚定的共享贴图**：2×2 张瓦片在窗口逻辑坐标
                    // (0,0)-(2048,1376) 上 1:1 铺一片场，所有卡采样同一片——
                    // 卡片只是裁剪窗（用户定的架构），不再每卡各自拉伸瓦片。
                    // 滚动时场钉在窗口上，与光球一致。换算：内容 = 窗口
                    // - 主区原点 + 平移（与上面光球同一套）。
                    let (tw, th) = GRAIN_TILE;
                    for row in 0..GRAIN_ROWS {
                        for col in 0..GRAIN_COLS {
                            let tile = Rectangle::new(
                                Point::new(
                                    col as f32 * tw - ox + tx,
                                    row as f32 * th - oy + ty,
                                ),
                                Size::new(tw, th),
                            );
                            if tile.intersection(&clipped).is_some() {
                                renderer.draw_image(img.clone(), tile, clipped);
                            }
                        }
                    }
                }
            });
        }

        // 3. 内容在最上层：with_layer 开新层，层序号更大、稳定后画
        //    （AGENTS.md 坑 23 的结论），按钮/输入框/文字都不吃颗粒。
        self.draw_content(tree, renderer, theme, style, layout, cursor, clipped);
    }

    // 无状态透传（与 iced container 同款）：tree 本身就是 content 的树，
    // 本 widget 不占状态槽。children()/diff() 全部直接委托。
    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            tree,
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                tree,
                layout.children().next().unwrap(),
                renderer,
                operation,
            );
        });
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            tree,
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            tree,
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message> From<Frosted<'a, Message>> for Element<'a, Message>
where
    Message: 'static,
{
    fn from(frosted: Frosted<'a, Message>) -> Self {
        Element::new(frosted)
    }
}

/// 从容器样式的线性渐变提取玻璃 shader 的面层数据。没有渐变背景
/// （不该发生）时返回 None，调用方退回旧路径。
fn extract_glass(
    style: &container::Style,
    grain: f32,
) -> Option<crate::ui::glass_pipeline::CardGlass> {
    let Background::Gradient(Gradient::Linear(l)) = style.background.as_ref()? else {
        return None;
    };
    let sheen = l.stops[0]?.color;
    let base_stop = l.stops[1]?.color;
    Some(crate::ui::glass_pipeline::CardGlass {
        sheen,
        base: base_stop,
        stop: l.stops[1]?.offset,
        angle: l.angle,
        border: style.border.color,
        border_width: style.border.width,
        radius: style.border.radius.top_left,
        grain_opacity: grain,
        grain_tile: GRAIN_TILE,
    })
}
