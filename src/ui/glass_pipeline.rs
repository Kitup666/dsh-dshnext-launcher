//! 玻璃场自定义 wgpu 管线：iced_wgpu 0.14 官方 primitive 通路
//! （`primitive::Primitive` + `Renderer::draw_primitive`）。
//!
//! 架构是「一张场、多裁剪窗」的解析实现：光球场在 fragment 里逐像素
//! 求值（数学上比离屏贴图更准，零拷贝零中间纹理），背景与所有卡片
//! 走同一份求值代码——背景与卡内从构造上就是同一片场。卡片模式在
//! 场之上做苹果/Fluent 的完整层栈：面层渐变 → 场 → overlay 颜色修正
//! → 颗粒 → 圆角描边（层栈详见 glass.wgsl 头注释）。
//!
//! 层序保证：iced_wgpu 同层 flush 顺序是 quads→triangles→primitives→
//! images→text（layer.rs `start()/end()`），所以阴影 quad（基层）→
//! 玻璃 primitive（with_layer 层）→ 内容（再一层）天然保序。
//! 卡片矩形不经手滚动数学：prepare 收到的 bounds 已带上层变换
//! （scrollable 的 with_translation、reveal 位移），滚动对齐免费正确。
//!
//! tiny-skia 回退：fallback 渲染器枚举变体公开，调用方按变体分路
//! （`Primary` = wgpu，`Secondary` = tiny-skia 走旧 mesh 路径）。

use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};

use bytemuck::{Pod, Zeroable};
use iced::advanced::layout;
use iced::advanced::widget::Tree;
use iced::advanced::{mouse, renderer, Layout, Widget};
use iced::{Color, Element, Length, Point, Radians, Rectangle, Size};
use iced_renderer::fallback::Renderer as Fallback;
use iced_wgpu::primitive::{self, Primitive};
use iced_wgpu::wgpu::util::DeviceExt as _;
use iced_wgpu::{graphics::Viewport, wgpu};

use crate::ui::glow_mesh;

/// overlay 修正参数（zhihu p/657181578：苹果暗色 = overlay 叠灰）。
/// 文章给 B=20 是校准明亮壁纸的；我们底子近乎纯黑，照抄会把卡面压成
/// (10,10,13) 死黑、场全灭——实测后取 0.42（轻度加深 + 保饱和）。
/// 灰 0.5 = 恒等；要回「无修正」把灰设 0.5 或强度设 0。
pub const OVERLAY_GRAY: f32 = 0.42;
pub const OVERLAY_STRENGTH: f32 = 1.0;

/// 卡片模式的全部面层数据（从 container::Style 的线性渐变提取）。
#[derive(Debug, Clone, Copy)]
pub struct CardGlass {
    pub sheen: Color,
    pub base: Color,
    pub stop: f32,
    pub angle: Radians,
    pub border: Color,
    pub border_width: f32,
    pub radius: f32,
    pub grain_opacity: f32,
    pub grain_tile: (f32, f32),
}

/// 一个玻璃 quad 的绘制模式。shader 里 mode：0=背景、1=卡片、2=帏幕。
/// 生命周期：每帧 draw 时构造，prepare 后即弃；slot 存管线里的
/// uniform 缓冲槽位号（prepare 无 mut self，用原子回写；Primitive
/// 要求 Send+Sync，不能用 Cell）。
#[derive(Debug)]
pub struct GlassQuad {
    pub kind: Kind,
    pub orbs: [(Point, f32, Color); 2],
    slot: AtomicU32,
}

#[derive(Debug)]
pub enum Kind {
    /// 只画光球场（透明底，premultiplied 输出）。
    Background,
    /// 卡片完整层栈（面层渐变→场→overlay→颗粒→圆角描边）。
    Card(CardGlass),
    /// 滚动虚化帏幕：不透明背景场（bg_app + 光球，与窗外续上），
    /// 沿带高做 alpha 渐隐——滚动内容经过顶/底边时融进背景。
    /// band 逻辑 px；top=true 时贴上边不透明、向下渐隐。
    Veil { bg: Color, band: f32, top: bool },
}

impl GlassQuad {
    pub fn background(orbs: [(Point, f32, Color); 2]) -> Self {
        Self { kind: Kind::Background, orbs, slot: AtomicU32::new(0) }
    }

    pub fn card(glass: CardGlass, orbs: [(Point, f32, Color); 2]) -> Self {
        Self { kind: Kind::Card(glass), orbs, slot: AtomicU32::new(0) }
    }

    pub fn veil(
        bg: Color,
        band: f32,
        top: bool,
        orbs: [(Point, f32, Color); 2],
    ) -> Self {
        Self { kind: Kind::Veil { bg, band, top }, orbs, slot: AtomicU32::new(0) }
    }
}

// ── GPU 侧数据 ──────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    rect: [f32; 4],       // 物理 px 窗口坐标 x,y,w,h
    params_a: [f32; 4],   // scale, radius物理, mode(1=卡), 等效overlay灰
    params_b: [f32; 4],   // 渐变stop, 颗粒不透明度, 0, 0
    grad: [f32; 4],       // 渐变起止（物理 px）
    sheen: [f32; 4],
    base: [f32; 4],
    border: [f32; 4],     // linear rgb + 物理宽
    grain_tile: [f32; 4], // 逻辑尺寸 xy
    orb0: [f32; 4],       // 圆心物理 xy、半径物理、峰值 alpha
    orbcol0: [f32; 4],    // linear rgb
    orb1: [f32; 4],
    orbcol1: [f32; 4],
}

impl Primitive for GlassQuad {
    type Pipeline = GlassPipeline;

    fn prepare(
        &self,
        pipeline: &mut GlassPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let s = viewport.scale_factor();
        let rp = *bounds * s; // 物理 px；层变换（滚动/reveal）已由 iced 叠好
        // 对齐物理像素网格：1.25 缩放下逻辑间距 14/7 之类会产生 17.5 这种
        // 半像素坐标，1px 描边被两行像素平摊、深底上隐形（用户报的「底边
        // 白边有时缺失」）。整卡取整，位移 ≤0.5px 不可感知。
        let rp = Rectangle {
            x: rp.x.round(),
            y: rp.y.round(),
            width: rp.width.round(),
            height: rp.height.round(),
        };

        let mut u = Uniforms {
            rect: [rp.x, rp.y, rp.width, rp.height],
            params_a: [s, 0.0, 0.0, 0.0],
            params_b: [0.0; 4],
            grad: [0.0; 4],
            sheen: [0.0; 4],
            base: [0.0; 4],
            border: [0.0; 4],
            grain_tile: [0.0; 4],
            orb0: [0.0; 4],
            orbcol0: [0.0; 4],
            orb1: [0.0; 4],
            orbcol1: [0.0; 4],
        };
        for (i, (center, radius, color)) in self.orbs.iter().enumerate() {
            let c = Point::new(center.x * s, center.y * s);
            let col = linear(*color);
            let (spec, col_slot) = if i == 0 {
                (&mut u.orb0, &mut u.orbcol0)
            } else {
                (&mut u.orb1, &mut u.orbcol1)
            };
            *spec = [c.x, c.y, radius * s, col[3]];
            *col_slot = [col[0], col[1], col[2], 0.0];
        }
        // 帏幕：sheen 扛背景底色，grain_tile.x 扛带高（物理 px）、
        // .y 扛方向旗（0=顶帏、1=底帏）——shader 模式 2 的约定。
        if let Kind::Veil { bg, band, top } = &self.kind {
            let c = linear(*bg);
            u.params_a[2] = 2.0;
            u.sheen = [c[0], c[1], c[2], 0.0];
            u.grain_tile = [band * s, if *top { 0.0 } else { 1.0 }, 0.0, 0.0];
        }
        if let Kind::Card(g) = &self.kind {
            u.params_a[1] = g.radius * s;
            u.params_a[2] = 1.0;
            // 等效灰：gray_eff = 0.5 + (gray − 0.5)·strength（0.5 = 恒等）
            u.params_a[3] = 0.5 + (OVERLAY_GRAY - 0.5) * OVERLAY_STRENGTH;
            // shader 约定：params_b = (渐变stop, 0, 颗粒不透明度, 0)
            u.params_b = [g.stop, 0.0, g.grain_opacity, 0.0];
            // 渐变起止：iced_core Angle::to_distance 原式（逻辑）× scale
            let angle = g.angle.0 - std::f32::consts::FRAC_PI_2;
            let (rx, ry) = (angle.cos(), angle.sin());
            let d = ((rx * rp.width / 2.0).abs()).max((ry * rp.height / 2.0).abs());
            let cx = rp.x + rp.width / 2.0;
            let cy = rp.y + rp.height / 2.0;
            u.grad = [cx - rx * d, cy - ry * d, cx + rx * d, cy + ry * d];
            let sheen = linear(g.sheen);
            let base = linear(g.base);
            let border = linear(g.border);
            u.sheen = [sheen[0], sheen[1], sheen[2], 0.0];
            u.base = [base[0], base[1], base[2], 0.0];
            u.border = [
                // iced quad shader 对描边色 premultiply（shader/color.wgsl）；
                // 不预乘的话 5% 白会按纯白混入，描边亮一圈
                border[0] * border[3],
                border[1] * border[3],
                border[2] * border[3],
                g.border_width * s,
            ];
            u.grain_tile = [g.grain_tile.0, g.grain_tile.1, 0.0, 0.0];
        }

        let slot = pipeline.slot(device);
        queue.write_buffer(&pipeline.slots[slot].buffer, 0, bytemuck::bytes_of(&u));
        self.slot.store(slot as u32, Ordering::Relaxed);
    }

    fn draw(&self, pipeline: &GlassPipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let slot = &pipeline.slots[self.slot.load(Ordering::Relaxed) as usize];
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &slot.bind_group, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

/// web-colors 关闭：iced 全管线按物理混色，顶点色走 into_linear。
fn linear(c: Color) -> [f32; 4] {
    c.into_linear()
}

pub struct GlassPipeline {
    pipeline: wgpu::RenderPipeline,
    grain_view: wgpu::TextureView,
    grain_sampler: wgpu::Sampler,
    slots: Vec<Slot>,
    next: usize,
    bgl: wgpu::BindGroupLayout,
}

struct Slot {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl primitive::Pipeline for GlassPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dshnext.glass.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glass.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dshnext.glass.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dshnext.glass.layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("dshnext.glass.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // 颗粒纹理：sRGB 采样视图（与 iced image 管线的线性化一致）
        let decoder = png::Decoder::new(Cursor::new(GRAIN_PNG));
        let mut reader = decoder.read_info().expect("grain.png 可读");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("grain.png 解码");
        assert_eq!(info.color_type, png::ColorType::Rgba, "grain.png 必须是 RGBA8");
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("dshnext.glass.grain"),
                size: wgpu::Extent3d {
                    width: info.width,
                    height: info.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &buf[..info.buffer_size()],
        );
        let grain_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let grain_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("dshnext.glass.grain.sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { pipeline, grain_view, grain_sampler, slots: Vec::new(), next: 0, bgl }
    }

    fn trim(&mut self) {
        self.next = 0;
    }
}

impl GlassPipeline {
    /// 取第 i 个槽位，不够就现建（每帧实例数稳定后零分配）。
    fn slot(&mut self, device: &wgpu::Device) -> usize {
        let i = self.next;
        self.next += 1;
        if i >= self.slots.len() {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dshnext.glass.uniforms"),
                size: std::mem::size_of::<Uniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dshnext.glass.bind_group"),
                layout: &self.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.grain_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.grain_sampler),
                    },
                ],
            });
            self.slots.push(Slot { buffer, bind_group });
        }
        i
    }
}

const GRAIN_PNG: &[u8] = include_bytes!("../../assets/textures/grain.png");

// ── 背景模式 widget ─────────────────────────────────────────────────────

/// 全窗光球场背景：wgpu 走玻璃 shader 的背景模式（与卡片同一份求值），
/// tiny-skia 回退到 mesh 扇形（旧路径）。
pub fn background_field<Message: 'static>(
    specs: [(Point, f32, Color); 2],
) -> Element<'static, Message> {
    BackgroundField { specs }.into()
}

struct BackgroundField {
    specs: [(Point, f32, Color); 2],
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for BackgroundField
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
        layout::Node::new(limits.max())
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
        match renderer {
            Fallback::Primary(r) => {
                use iced_wgpu::primitive::Renderer as _;
                r.draw_primitive(bounds, GlassQuad::background(self.specs));
            }
            Fallback::Secondary(r) => {
                use iced::advanced::graphics::mesh::Renderer as _;
                for (center, radius, color) in &self.specs {
                    r.draw_mesh(glow_mesh::build_orb(*center, *radius, *color, bounds));
                }
            }
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

impl<Message> From<BackgroundField> for Element<'static, Message>
where
    Message: 'static,
{
    fn from(w: BackgroundField) -> Self {
        Element::new(w)
    }
}

// ── 滚动虚化帏幕 widget ────────────────────────────────────────────────

/// 滚动内容在顶/底边的渐隐帏幕：wgpu 走 shader 模式 2（不透明背景场
/// bg_app + 光球 + 带高 alpha 渐隐，与窗外背景续上）；tiny-skia 回退
/// fill_quad 两停线性渐变。Fill×Fill 布局，带只占靠边 band 逻辑 px，
/// 其余区域 alpha=0——别按带高收缩，stack 里要盖住整个滚动区。
///
/// 事件透传：不实现 update，stack 逆序派发时返回 Ignored，滚动区
/// （前一个孩子）照常收滚轮/拖拽。
pub fn fade_veil<Message: 'static>(
    pal: &'static crate::theme::Palette,
    band: f32,
    top: bool,
) -> Element<'static, Message> {
    FadeVeil { pal, band, top }.into()
}

struct FadeVeil {
    pal: &'static crate::theme::Palette,
    band: f32,
    top: bool,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for FadeVeil
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
        layout::Node::new(limits.max())
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
        match renderer {
            Fallback::Primary(r) => {
                use iced_wgpu::primitive::Renderer as _;
                let quad = GlassQuad::veil(
                    self.pal.bg_app,
                    self.band,
                    self.top,
                    glow_mesh::ambient_specs(self.pal),
                );
                r.draw_primitive(bounds, quad);
            }
            Fallback::Secondary(r) => {
                // 回退不用渐变 quad API：两三角形 mesh，顶点 alpha 直落
                // （与光球回退同一套 Mesh::Solid 机制）。
                use iced::advanced::graphics::mesh::{Indexed, Mesh, SolidVertex2D};
                use iced::advanced::graphics::mesh::Renderer as _;
                let bg = self.pal.bg_app;
                let clear = iced::Color { a: 0.0, ..bg };
                let pack = iced::advanced::graphics::color::pack;
                let h = self.band.min(bounds.height);
                let (y0, y1) = if self.top {
                    (0.0, h)
                } else {
                    (bounds.height - h, bounds.height)
                };
                let (c_edge, c_inner) = if self.top { (bg, clear) } else { (clear, bg) };
                let w = bounds.width;
                let vertices = vec![
                    SolidVertex2D { position: [0.0, y0], color: pack(c_edge) },
                    SolidVertex2D { position: [w, y0], color: pack(c_edge) },
                    SolidVertex2D { position: [0.0, y1], color: pack(c_inner) },
                    SolidVertex2D { position: [w, y1], color: pack(c_inner) },
                ];
                r.draw_mesh(Mesh::Solid {
                    buffers: Indexed { vertices, indices: vec![0, 1, 2, 1, 3, 2] },
                    transformation: iced::Transformation::IDENTITY,
                    clip_bounds: bounds,
                });
            }
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

impl<Message> From<FadeVeil> for Element<'static, Message>
where
    Message: 'static,
{
    fn from(w: FadeVeil) -> Self {
        Element::new(w)
    }
}
