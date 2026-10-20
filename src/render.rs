//! Desenho do quadro na CPU. Tudo entra em pixels lógicos e sai multiplicado pela escala.
use crate::{Background, Border, Color, Point, Rect, Shadow, Size};
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight};
use std::collections::HashMap;
use std::sync::Arc;
use tiny_skia::{FillRule, FilterQuality, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pattern, Pixmap, PixmapPaint, SpreadMode, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Font {
    /// `None` = sem serifa do sistema.
    pub family: Option<&'static str>,
    pub weight: u16,
}

impl Font {
    pub const DEFAULT: Font = Font { family: None, weight: 400 };
    pub const MEDIUM: Font = Font { family: None, weight: 500 };
    pub const SEMIBOLD: Font = Font { family: None, weight: 600 };
    pub const BOLD: Font = Font { family: None, weight: 700 };
    pub const MONO: Font = Font { family: Some("monospace"), weight: 400 };
}

impl Default for Font {
    fn default() -> Self {
        Font::DEFAULT
    }
}

/// Imagem já decodificada, compartilhada barato entre quadros.
#[derive(Clone)]
pub struct Image(Arc<Pixmap>);

impl Image {
    /// Pixels RGBA8 sem pré-multiplicar, linha a linha. `None` se o tamanho não bate.
    pub fn from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Option<Self> {
        if rgba.len() != (width * height * 4) as usize {
            return None;
        }
        for p in rgba.chunks_exact_mut(4) {
            let a = p[3] as u16;
            for c in &mut p[..3] {
                *c = ((*c as u16 * a + 127) / 255) as u8;
            }
        }
        Some(Image(Arc::new(Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(width, height)?)?)))
    }
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        Pixmap::decode_png(bytes).ok().map(|p| Image(Arc::new(p)))
    }
    pub fn size(&self) -> Size {
        Size::new(self.0.width() as f32, self.0.height() as f32)
    }
}

/// SVG analisado uma vez; a rasterização fica em cache por tamanho e cor.
#[derive(Clone)]
pub struct Svg(Arc<resvg::usvg::Tree>);

impl Svg {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok().map(|t| Svg(Arc::new(t)))
    }
    pub fn size(&self) -> Size {
        let s = self.0.size();
        Size::new(s.width(), s.height())
    }
}

#[derive(PartialEq, Eq, Hash)]
struct TextKey {
    text: String,
    size: u32,
    font: Font,
    width: Option<u32>,
}

#[derive(PartialEq, Eq, Hash)]
struct SvgKey {
    tree: usize,
    w: u32,
    h: u32,
    color: Option<[u8; 4]>,
}

#[derive(PartialEq, Eq, Hash)]
struct ShadowKey {
    w: u32,
    h: u32,
    radius: [u32; 4],
    blur: u32,
    color: [u8; 4],
}

struct Clip {
    rect: Rect,
    mask: Option<Mask>,
}

pub struct Renderer {
    pub pixmap: Pixmap,
    scale: f32,
    fonts: FontSystem,
    swash: SwashCache,
    texts: HashMap<TextKey, Buffer>,
    svgs: HashMap<SvgKey, Pixmap>,
    shadows: HashMap<ShadowKey, Pixmap>,
    clips: Vec<Clip>,
}

impl Renderer {
    pub fn new() -> Self {
        Renderer {
            pixmap: Pixmap::new(1, 1).expect("1x1"),
            scale: 1.0,
            fonts: FontSystem::new(),
            swash: SwashCache::new(),
            texts: HashMap::new(),
            svgs: HashMap::new(),
            shadows: HashMap::new(),
            clips: Vec::new(),
        }
    }

    /// Prepara um quadro novo de `size` lógico na escala `scale`, limpo com `fundo`.
    pub fn begin(&mut self, size: Size, scale: f32, fundo: Color) {
        let (w, h) = (((size.width * scale).round() as u32).max(1), ((size.height * scale).round() as u32).max(1));
        if self.pixmap.width() != w || self.pixmap.height() != h {
            self.pixmap = Pixmap::new(w, h).expect("tamanho válido");
        }
        if self.scale != scale {
            self.texts.clear();
            self.svgs.clear();
        }
        self.scale = scale;
        self.clips.clear();
        self.pixmap.fill(skia_color(fundo));
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    fn buffer(&mut self, text: &str, size: f32, font: Font, max_width: Option<f32>) -> &Buffer {
        let s = self.scale;
        let key = TextKey { text: text.to_owned(), size: (size * s * 64.0) as u32, font, width: max_width.map(|w| (w * s) as u32) };
        if self.texts.len() > 4000 {
            self.texts.clear();
        }
        let fonts = &mut self.fonts;
        self.texts.entry(key).or_insert_with(|| {
            let mut b = Buffer::new(fonts, Metrics::new(size * s, (size * 1.3 * s).ceil()));
            b.set_size(fonts, max_width.map(|w| w * s), None);
            let attrs = Attrs::new().weight(Weight(font.weight)).family(match font.family {
                None => Family::SansSerif,
                Some("monospace") => Family::Monospace,
                Some(nome) => Family::Name(nome),
            });
            b.set_text(fonts, text, attrs, Shaping::Advanced);
            b
        })
    }

    /// Tamanho lógico do texto; com `max_width` quebra em linhas.
    pub fn measure(&mut self, text: &str, size: f32, font: Font, max_width: Option<f32>) -> Size {
        let s = self.scale;
        let b = self.buffer(text, size, font, max_width);
        let (mut w, mut h) = (0.0f32, 0.0f32);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            h = run.line_top + run.line_height;
        }
        Size::new((w / s).ceil(), (h / s).ceil())
    }

    /// Início e fim (x lógico) de cada caractere da primeira linha, com o índice em bytes: o cursor do campo de texto.
    pub fn carets(&mut self, text: &str, size: f32, font: Font) -> Vec<(usize, f32, f32)> {
        let s = self.scale;
        let b = self.buffer(text, size, font, None);
        b.layout_runs().take(1).flat_map(|r| r.glyphs.iter().map(|g| (g.start, g.x / s, (g.x + g.w) / s)).collect::<Vec<_>>()).collect()
    }

    pub fn draw_text(&mut self, text: &str, size: f32, font: Font, color: Color, at: Point, max_width: Option<f32>) {
        let s = self.scale;
        let (ox, oy) = ((at.x * s).round() as i32, (at.y * s).round() as i32);
        let clip = self.clips.last().map(|c| c.rect);
        let cor = cosmic_text::Color::rgba((color.r * 255.0) as u8, (color.g * 255.0) as u8, (color.b * 255.0) as u8, (color.a * 255.0) as u8);
        self.buffer(text, size, font, max_width);
        let key = TextKey { text: text.to_owned(), size: (size * s * 64.0) as u32, font, width: max_width.map(|w| (w * s) as u32) };
        let b = &self.texts[&key];
        let (w, h) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let (cx0, cy0, cx1, cy1) = match clip {
            Some(r) => ((r.x * s).floor() as i32, (r.y * s).floor() as i32, ((r.x + r.width) * s).ceil() as i32, ((r.y + r.height) * s).ceil() as i32),
            None => (0, 0, w, h),
        };
        let mask = self.clips.last().and_then(|c| c.mask.as_ref()).map(|m| m.data());
        let px = self.pixmap.pixels_mut();
        b.draw(&mut self.fonts, &mut self.swash, cor, |x, y, gw, gh, c| {
            let a = c.a() as u32;
            if a == 0 {
                return;
            }
            for yy in (oy + y).max(cy0.max(0))..(oy + y + gh as i32).min(cy1.min(h)) {
                for xx in (ox + x).max(cx0.max(0))..(ox + x + gw as i32).min(cx1.min(w)) {
                    let i = (yy * w + xx) as usize;
                    let a = match mask {
                        Some(m) => a * m[i] as u32 / 255,
                        None => a,
                    };
                    blend(&mut px[i], c.r(), c.g(), c.b(), a);
                }
            }
        });
    }

    /// Retângulo com fundo, borda e sombra; o raio vale para os três.
    pub fn fill_quad(&mut self, r: Rect, bg: Option<&Background>, border: Border, shadow: Option<Shadow>) {
        let s = self.scale;
        if let Some(sh) = shadow.filter(|sh| sh.color.a > 0.0) {
            self.shadow(r, border.radius, sh);
        }
        let Some(path) = rounded(r, border.radius, s) else { return };
        if let Some(bg) = bg {
            let p = match bg {
                Background::Color(c) => paint(*c),
                Background::Linear { angle, stops } => {
                    let (cx, cy, dx, dy) = ((r.x + r.width / 2.0) * s, (r.y + r.height / 2.0) * s, angle.cos(), angle.sin());
                    let meio = (r.width * dx.abs() + r.height * dy.abs()) / 2.0 * s;
                    let paradas = stops.iter().map(|(t, c)| GradientStop::new(*t, skia_color(*c))).collect();
                    let shader = LinearGradient::new((cx - dx * meio, cy - dy * meio).into(), (cx + dx * meio, cy + dy * meio).into(), paradas, SpreadMode::Pad, Transform::identity());
                    match shader {
                        Some(shader) => Paint { shader, anti_alias: true, ..Default::default() },
                        None => paint(stops.first().map(|s| s.1).unwrap_or_default()),
                    }
                }
            };
            self.fill_path(&path, &p);
        }
        if border.width > 0.0 && border.color.a > 0.0 {
            // A borda fica do lado de dentro: o anel entre o contorno e o contorno encolhido.
            let w = border.width;
            let dentro = Rect { x: r.x + w, y: r.y + w, width: r.width - 2.0 * w, height: r.height - 2.0 * w };
            let mut pb = PathBuilder::new();
            push_rounded(&mut pb, r, border.radius, s);
            push_rounded(&mut pb, dentro, border.radius.map(|v| (v - w).max(0.0)), s);
            if let Some(anel) = pb.finish() {
                let mask = self.clips.last().and_then(|c| c.mask.as_ref());
                self.pixmap.fill_path(&anel, &paint(border.color), FillRule::EvenOdd, Transform::identity(), mask);
            }
        }
    }

    fn fill_path(&mut self, p: &tiny_skia::Path, paint: &Paint) {
        let mask = self.clips.last().and_then(|c| c.mask.as_ref());
        self.pixmap.fill_path(p, paint, FillRule::Winding, Transform::identity(), mask);
    }

    /// Sombra desfocada de verdade, guardada em cache pelo tamanho, raio, desfoque e cor.
    fn shadow(&mut self, r: Rect, radius: [f32; 4], sh: Shadow) {
        let s = self.scale;
        let m = (sh.blur * s).ceil();
        let (w, h) = ((r.width * s).ceil() + 2.0 * m, (r.height * s).ceil() + 2.0 * m);
        if w < 1.0 || h < 1.0 {
            return;
        }
        let cor = skia_color(sh.color).to_color_u8();
        let key = ShadowKey { w: w as u32, h: h as u32, radius: radius.map(|v| (v * s) as u32), blur: m as u32, color: [cor.red(), cor.green(), cor.blue(), cor.alpha()] };
        if self.shadows.len() > 256 {
            self.shadows.clear();
        }
        let pm = self.shadows.entry(key).or_insert_with(|| {
            let mut pm = Pixmap::new(w as u32, h as u32).expect("w, h >= 1");
            let caixa = Rect { x: m / s, y: m / s, width: r.width, height: r.height };
            if let Some(p) = rounded(caixa, radius, s) {
                pm.fill_path(&p, &paint(sh.color), FillRule::Winding, Transform::identity(), None);
            }
            if m >= 1.0 {
                let mut px: Vec<[u32; 4]> = pm.pixels().iter().map(|p| [p.red() as u32, p.green() as u32, p.blue() as u32, p.alpha() as u32]).collect();
                let k = (m / 3.0).round().max(1.0) as usize;
                for _ in 0..3 {
                    box_blur(&mut px, w as usize, h as usize, k, true);
                    box_blur(&mut px, w as usize, h as usize, k, false);
                }
                let bytes: Vec<u8> = px.iter().flat_map(|p| p.map(|c| c.min(255) as u8)).collect();
                pm = Pixmap::from_vec(bytes, tiny_skia::IntSize::from_wh(w as u32, h as u32).expect("w, h >= 1")).expect("mesmo tamanho");
            }
            pm
        });
        let mask = self.clips.last().and_then(|c| c.mask.as_ref());
        let (x, y) = (((r.x + sh.offset.x) * s - m).round() as i32, ((r.y + sh.offset.y) * s - m).round() as i32);
        self.pixmap.draw_pixmap(x, y, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
    }

    /// Imagem esticada em `r`, já recortada pelos cantos `radius`.
    pub fn draw_image(&mut self, img: &Image, r: Rect, radius: [f32; 4]) {
        let s = self.scale;
        let (iw, ih) = (img.0.width() as f32, img.0.height() as f32);
        let Some(path) = rounded(r, radius, s) else { return };
        let t = Transform::from_row(r.width * s / iw, 0.0, 0.0, r.height * s / ih, r.x * s, r.y * s);
        let shader = Pattern::new(img.0.as_ref().as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, t);
        self.fill_path(&path, &Paint { shader, anti_alias: true, ..Default::default() });
    }

    /// SVG em `r`; com `color`, troca a cor de tudo mantendo o alfa (ícones de um traço só).
    pub fn draw_svg(&mut self, svg: &Svg, r: Rect, color: Option<Color>) {
        let s = self.scale;
        let (w, h) = ((r.width * s).round() as u32, (r.height * s).round() as u32);
        if w == 0 || h == 0 {
            return;
        }
        let cor = color.map(|c| skia_color(c).to_color_u8()).map(|c| [c.red(), c.green(), c.blue(), c.alpha()]);
        let key = SvgKey { tree: Arc::as_ptr(&svg.0) as usize, w, h, color: cor };
        if self.svgs.len() > 512 {
            self.svgs.clear();
        }
        let pm = self.svgs.entry(key).or_insert_with(|| {
            let mut pm = Pixmap::new(w, h).expect("tamanho > 0");
            let t = svg.0.size();
            resvg::render(&svg.0, Transform::from_scale(w as f32 / t.width(), h as f32 / t.height()), &mut pm.as_mut());
            if let Some([cr, cg, cb, ca]) = cor {
                for p in pm.pixels_mut() {
                    let a = p.alpha() as u32 * ca as u32 / 255;
                    let pre = |c: u8| (c as u32 * a / 255) as u8;
                    *p = tiny_skia::PremultipliedColorU8::from_rgba(pre(cr), pre(cg), pre(cb), a as u8).expect("pré-multiplicado");
                }
            }
            pm
        });
        let mask = self.clips.last().and_then(|c| c.mask.as_ref());
        self.pixmap.draw_pixmap((r.x * s).round() as i32, (r.y * s).round() as i32, pm.as_ref(), &PixmapPaint::default(), Transform::identity(), mask);
    }

    /// Linha passando pelos `pontos`, com pontas e junções redondas.
    pub fn stroke(&mut self, pontos: &[Point], largura: f32, cor: Color) {
        let s = self.scale;
        let mut pb = PathBuilder::new();
        for (i, p) in pontos.iter().enumerate() {
            if i == 0 {
                pb.move_to(p.x * s, p.y * s);
            } else {
                pb.line_to(p.x * s, p.y * s);
            }
        }
        let Some(path) = pb.finish() else { return };
        let traco = tiny_skia::Stroke { width: largura * s, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
        let mask = self.clips.last().and_then(|c| c.mask.as_ref());
        self.pixmap.stroke_path(&path, &paint(cor), &traco, Transform::identity(), mask);
    }

    /// Desfoca o que já está desenhado em `r` (vidro), respeitando os cantos.
    pub fn blur(&mut self, r: Rect, raio: f32, cantos: [f32; 4]) {
        let s = self.scale;
        let (pw, ph) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let x0 = ((r.x * s).floor() as i32).clamp(0, pw);
        let y0 = ((r.y * s).floor() as i32).clamp(0, ph);
        let x1 = (((r.x + r.width) * s).ceil() as i32).clamp(0, pw);
        let y1 = (((r.y + r.height) * s).ceil() as i32).clamp(0, ph);
        let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
        if w == 0 || h == 0 {
            return;
        }
        let mut px: Vec<[u32; 4]> = Vec::with_capacity(w * h);
        let src = self.pixmap.pixels();
        for y in y0..y1 {
            for x in x0..x1 {
                let p = src[(y * pw + x) as usize];
                px.push([p.red() as u32, p.green() as u32, p.blue() as u32, p.alpha() as u32]);
            }
        }
        // Três caixas seguidas aproximam uma gaussiana.
        let k = ((raio * s) / 3.0).round().max(1.0) as usize;
        for _ in 0..3 {
            box_blur(&mut px, w, h, k, true);
            box_blur(&mut px, w, h, k, false);
        }
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.map(|c| c.min(255) as u8)).collect();
        let Some(pm) = Pixmap::from_vec(bytes, tiny_skia::IntSize::from_wh(w as u32, h as u32).expect("w, h > 0")) else { return };
        let Some(path) = rounded(r, cantos, s) else { return };
        let shader = Pattern::new(pm.as_ref(), SpreadMode::Pad, FilterQuality::Nearest, 1.0, Transform::from_translate(x0 as f32, y0 as f32));
        let mut paint = Paint { shader, anti_alias: true, ..Default::default() };
        paint.blend_mode = tiny_skia::BlendMode::Source;
        self.fill_path(&path, &paint);
    }

    /// Daqui até o `pop_clip`, nada sai de `r` (nem dos cantos arredondados).
    pub fn push_clip(&mut self, r: Rect, radius: [f32; 4]) {
        let s = self.scale;
        let rect = match self.clips.last() {
            Some(c) => c.rect.intersect(&r),
            None => r,
        };
        let mut mask = match self.clips.last().and_then(|c| c.mask.clone()) {
            Some(m) => m,
            None => {
                let mut m = Mask::new(self.pixmap.width(), self.pixmap.height()).expect("pixmap > 0");
                m.data_mut().fill(255);
                m
            }
        };
        let mask = match rounded(r, radius, s) {
            Some(p) => {
                mask.intersect_path(&p, FillRule::Winding, true, Transform::identity());
                Some(mask)
            }
            None => {
                mask.data_mut().fill(0);
                Some(mask)
            }
        };
        self.clips.push(Clip { rect, mask });
    }

    pub fn pop_clip(&mut self) {
        self.clips.pop();
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::new()
    }
}

/// Média móvel de largura `2k+1` numa direção, com a borda repetida.
fn box_blur(px: &mut [[u32; 4]], w: usize, h: usize, k: usize, horizontal: bool) {
    let (linhas, n) = if horizontal { (h, w) } else { (w, h) };
    let idx = |l: usize, i: usize| if horizontal { l * w + i } else { i * w + l };
    let mut linha = vec![[0u32; 4]; n];
    let div = (2 * k + 1) as u32;
    for l in 0..linhas {
        for (i, v) in linha.iter_mut().enumerate() {
            *v = px[idx(l, i)];
        }
        let mut soma = [0u32; 4];
        for j in 0..=2 * k {
            let v = linha[j.saturating_sub(k).min(n - 1)];
            (0..4).for_each(|c| soma[c] += v[c]);
        }
        for i in 0..n {
            px[idx(l, i)] = soma.map(|v| v / div);
            let sai = linha[i.saturating_sub(k)];
            let entra = linha[(i + k + 1).min(n - 1)];
            (0..4).for_each(|c| soma[c] = soma[c] + entra[c] - sai[c]);
        }
    }
}

fn blend(p:&mut tiny_skia::PremultipliedColorU8, r: u8, g: u8, b: u8, a: u32) {
    let inv = 255 - a;
    let m = |src: u8, dst: u8| ((src as u32 * a + dst as u32 * inv) / 255) as u8;
    let alpha = (a + p.alpha() as u32 * inv / 255) as u8;
    *p = tiny_skia::PremultipliedColorU8::from_rgba(m(r, p.red()).min(alpha), m(g, p.green()).min(alpha), m(b, p.blue()).min(alpha), alpha).unwrap_or(*p);
}

fn skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(c.r.clamp(0.0, 1.0), c.g.clamp(0.0, 1.0), c.b.clamp(0.0, 1.0), c.a.clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::TRANSPARENT)
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint { anti_alias: true, ..Default::default() };
    p.set_color(skia_color(c));
    p
}

fn rounded(r: Rect, radius: [f32; 4], s: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    push_rounded(&mut pb, r, radius, s);
    pb.finish()
}

fn push_rounded(pb: &mut PathBuilder, r: Rect, radius: [f32; 4], s: f32) {
    if r.width <= 0.0 || r.height <= 0.0 {
        return;
    }
    let (x, y, w, h) = (r.x * s, r.y * s, r.width * s, r.height * s);
    let max = w.min(h) / 2.0;
    let [tl, tr, br, bl] = radius.map(|v| (v * s).clamp(0.0, max));
    // Constante da aproximação de um quarto de círculo por cúbica.
    const K: f32 = 0.447_715;
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.cubic_to(x + w - tr * K, y, x + w, y + tr * K, x + w, y + tr);
    pb.line_to(x + w, y + h - br);
    pb.cubic_to(x + w, y + h - br * K, x + w - br * K, y + h, x + w - br, y + h);
    pb.line_to(x + bl, y + h);
    pb.cubic_to(x + bl * K, y + h, x, y + h - bl * K, x, y + h - bl);
    pb.line_to(x, y + tl);
    pb.cubic_to(x, y + tl * K, x + tl * K, y, x + tl, y);
    pb.close();
}
