// 玻璃场 shader：背景与卡片共用一个 fragment 求值——「一张贴图、多裁剪窗」
// 的解析版（场是程序化的，解析求值比离屏贴图更准且零拷贝）。
//
// 卡片模式 = 苹果/Fluent 毛玻璃的完整层栈（zhihu p/657181578）：
//   面层（sheen→base 渐变，iced quad smoothstep 渐变的逐像素复刻）
//   → 模糊层（与背景同参光球，径向线性衰减 = mesh 扇形插值的解析式）
//   → 颜色修正层（overlay 混合 sRGB 空间灰值，恢复饱和度防死灰）
//   → 噪点层（窗口锚定颗粒场）
//   → 圆角裁剪 + 描边。
// iced 没有混合模式，修正层在这里第一次成为「真的」逐像素混合。
//
// 数学对齐 iced_wgpu 0.14（保证改版前后像素级一致）：
// - 渐变插值 = smoothstep（quad/gradient.wgsl），不是线性
// - 颜色 = linear 空间（GAMMA_CORRECTION），输出 premultiplied
//   （quad/triangle 管线都是 PREMULTIPLIED_ALPHA_BLENDING）
// - 圆角/描边 = quad.wgsl 的 rounded_box_sdf 原式

struct Uniforms {
    // 物理像素的吸附矩形（x, y, w, h）
    rect: vec4<f32>,
    // scale, radius(物理), mode(0=背景 1=卡), overlay 强度
    params_a: vec4<f32>,
    // overlay 灰值(sRGB 空间), 渐变 stop(0.4), 颗粒不透明度, pad
    params_b: vec4<f32>,
    // 渐变起止（物理 px，窗口坐标）
    grad: vec4<f32>,
    // sheen 色（linear rgb）+ pad
    sheen: vec4<f32>,
    // base 色（linear rgb）+ pad
    base: vec4<f32>,
    // 描边色（linear rgb）+ 物理宽
    border: vec4<f32>,
    // 颗粒瓦片逻辑尺寸 xy + pad
    grain_tile: vec4<f32>,
    // 球 0：圆心物理 xy、半径物理、峰值 alpha
    orb0: vec4<f32>,
    // 球 0 色（linear rgb）+ pad
    orbcol0: vec4<f32>,
    // 球 1
    orb1: vec4<f32>,
    orbcol1: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var grain_tex: texture_2d<f32>;
@group(0) @binding(2) var grain_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0)
    );
    let ndc = p[vi];
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    // viewport 恰为矩形本身：uv 0..1 覆盖矩形，v=0 在顶
    out.uv = vec2<f32>((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5);
    return out;
}

// 光球：mesh 扇形（中心色→r 处全透明，GPU 线性插值）的解析式，
// 距离线性衰减，premultiply 前的 straight rgb + alpha。
fn orb(p: vec2<f32>, spec: vec4<f32>, col: vec4<f32>) -> vec4<f32> {
    let t = col.a * clamp(1.0 - distance(p, spec.xy) / spec.z, 0.0, 1.0);
    return vec4<f32>(col.rgb, t);
}

// iced 渐变求值：smoothstep 插值（quad/gradient.wgsl 的两 stop 特例）
fn face_gradient(p: vec2<f32>) -> vec3<f32> {
    let v = u.grad.zw - u.grad.xy;
    let t = dot(normalize(v), p - u.grad.xy) / length(v);
    let f = smoothstep(0.0, u.params_b.y, t);
    return mix(u.sheen.rgb, u.base.rgb, f);
}

// overlay 混合（Photoshop 语义，sRGB 空间逐通道）
fn overlay_channel(base: f32, top: f32) -> f32 {
    return select(1.0 - 2.0 * (1.0 - base) * (1.0 - top), 2.0 * base * top, base < 0.5);
}

fn linear_to_srgb(c: f32) -> f32 {
    return select(c * 12.92, 1.055 * pow(c, 1.0 / 2.4) - 0.055, c > 0.0031308);
}

fn srgb_to_linear(c: f32) -> f32 {
    return select(c / 12.92, pow((c + 0.055) / 1.055, 2.4), c > 0.04045);
}

// quad.wgsl 原式
fn rounded_box_sdf(p: vec2<f32>, size: vec2<f32>, corners: vec4<f32>) -> f32 {
    let box_half = select(corners.yz, corners.xw, p.x > 0.0);
    let corner = select(box_half.y, box_half.x, p.y > 0.0);
    let q = abs(p) - size + vec2<f32>(corner);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - corner;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let phys = u.rect.xy + in.uv * u.rect.zw;
    let logical = phys / u.params_a.x;
    let is_card = u.params_a.z > 0.5;

    // ── 模糊层：光球场（背景/卡片同一份求值）──
    let f0 = orb(phys, u.orb0, u.orbcol0);
    let f1 = orb(phys, u.orb1, u.orbcol1);

    if (!is_card) {
        // 背景模式：透明底上按绘制顺序叠两球，输出 premultiplied
        let a = f1.a + f0.a * (1.0 - f1.a);
        let rgb = (f0.rgb * f0.a * (1.0 - f1.a) + f1.rgb * f1.a) / max(a, 1e-6);
        return vec4<f32>(rgb * a, a);
    }

    // ── 面层：sheen→base 渐变（不透明）──
    var rgb = face_gradient(phys);

    // 光球按序叠上（straight-alpha over，与 mesh 逐球混合一致）
    rgb = rgb * (1.0 - f0.a) + f0.rgb * f0.a;
    rgb = rgb * (1.0 - f1.a) + f1.rgb * f1.a;

    // ── 颜色修正层：sRGB 空间 overlay 灰（苹果暗色做法）──
    let gray = u.params_a.w;
    if (gray > 0.0) {
        var s = vec3<f32>(
            linear_to_srgb(rgb.r), linear_to_srgb(rgb.g), linear_to_srgb(rgb.b)
        );
        s = vec3<f32>(
            overlay_channel(s.r, gray),
            overlay_channel(s.g, gray),
            overlay_channel(s.b, gray),
        );
        rgb = vec3<f32>(
            srgb_to_linear(s.r), srgb_to_linear(s.g), srgb_to_linear(s.b)
        );
    }

    // ── 噪点层：窗口锚定颗粒场（窗口逻辑像素直接当 uv）──
    if (u.params_b.z > 0.0) {
        let uv = logical / u.grain_tile.xy;
        let g = textureSample(grain_tex, grain_samp, uv);
        let ga = g.a * u.params_b.z;
        rgb = rgb * (1.0 - ga) + g.rgb * ga;
    }

    // ── 圆角 + 描边（quad.wgsl 同款 SDF，物理 px）──
    let radius = vec4<f32>(u.params_a.y);
    let dist = rounded_box_sdf(
        -(phys - u.rect.xy - u.rect.zw / 2.0) * 2.0,
        u.rect.zw,
        radius * 2.0,
    ) / 2.0;

    let width = u.border.w;
    if (width > 0.0) {
        let k = clamp(0.5 + dist + width, 0.0, 1.0);
        rgb = mix(rgb, u.border.rgb, k);
    }

    let alpha = clamp(0.5 - dist, 0.0, 1.0);
    return vec4<f32>(rgb * alpha, alpha);
}
