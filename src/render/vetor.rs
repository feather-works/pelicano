//! Desenho vetorial no [`Renderer`]: caminhos com curvas, preenchidos (cor ou degradê) ou
//! contornados, debaixo de uma transformação (mover, girar, escalar) — o `Frame` do canvas.
use super::{clip_mask, paint, skia_color, Chave, Renderer};
use crate::{Background, Color, Font, Point, Rect};
use cosmic_text::Command;
use tiny_skia::{GradientStop, LinearGradient, Paint, SpreadMode};

/// Uma forma pronta, em pixels lógicos.
#[derive(Debug, Clone, PartialEq)]
pub struct Path(pub(crate) tiny_skia::Path);

#[derive(Default)]
pub struct PathBuilder(tiny_skia::PathBuilder);

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.0.move_to(p.x, p.y);
        self
    }
    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.0.line_to(p.x, p.y);
        self
    }
    pub fn quadratic_to(&mut self, controle: Point, p: Point) -> &mut Self {
        self.0.quad_to(controle.x, controle.y, p.x, p.y);
        self
    }
    pub fn bezier_to(&mut self, c1: Point, c2: Point, p: Point) -> &mut Self {
        self.0.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y);
        self
    }
    pub fn close(&mut self) -> &mut Self {
        self.0.close();
        self
    }
    pub fn rect(&mut self, r: Rect) -> &mut Self {
        if let Some(r) = tiny_skia::Rect::from_xywh(r.x, r.y, r.width, r.height) {
            self.0.push_rect(r);
        }
        self
    }
    pub fn circle(&mut self, centro: Point, raio: f32) -> &mut Self {
        self.0.push_circle(centro.x, centro.y, raio);
        self
    }
    pub fn ellipse(&mut self, centro: Point, raios: Point) -> &mut Self {
        if let Some(r) = tiny_skia::Rect::from_xywh(centro.x - raios.x, centro.y - raios.y, raios.x * 2.0, raios.y * 2.0) {
            self.0.push_oval(r);
        }
        self
    }
    /// Arco de elipse de `inicio` a `fim` (radianos, sentido horário na tela). Liga com uma reta
    /// ao ponto atual, se houver um; senão começa nele.
    pub fn arc(&mut self, centro: Point, raios: Point, inicio: f32, fim: f32) -> &mut Self {
        let ponto = |a: f32| Point::new(centro.x + raios.x * a.cos(), centro.y + raios.y * a.sin());
        let p0 = ponto(inicio);
        if self.0.is_empty() {
            self.move_to(p0);
        } else {
            self.line_to(p0);
        }
        // Até 90° por pedaço: cada um é uma cúbica com as alças em 4/3·tan(θ/4).
        let n = ((fim - inicio).abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
        let passo = (fim - inicio) / n as f32;
        let k = 4.0 / 3.0 * (passo / 4.0).tan();
        for i in 0..n {
            let (a, b) = (inicio + passo * i as f32, inicio + passo * (i + 1) as f32);
            let (pa, pb) = (ponto(a), ponto(b));
            let c1 = Point::new(pa.x - k * raios.x * a.sin(), pa.y + k * raios.y * a.cos());
            let c2 = Point::new(pb.x + k * raios.x * b.sin(), pb.y - k * raios.y * b.cos());
            self.bezier_to(c1, c2, pb);
        }
        self
    }
    /// `None` sem nenhum segmento.
    pub fn build(self) -> Option<Path> {
        self.0.finish().map(Path)
    }
}

impl Path {
    pub fn new(f: impl FnOnce(&mut PathBuilder)) -> Option<Path> {
        let mut b = PathBuilder::new();
        f(&mut b);
        b.build()
    }
    pub fn line(a: Point, b: Point) -> Path {
        Path::new(|p| {
            p.move_to(a).line_to(b);
        })
        .expect("dois pontos")
    }
    pub fn rect(r: Rect) -> Option<Path> {
        Path::new(|p| {
            p.rect(r);
        })
    }
    pub fn circle(centro: Point, raio: f32) -> Option<Path> {
        Path::new(|p| {
            p.circle(centro, raio);
        })
    }
    /// Pontos ligados por retas; `fechado` liga o último ao primeiro.
    pub fn polyline(pontos: &[Point], fechado: bool) -> Option<Path> {
        Path::new(|p| {
            for (i, q) in pontos.iter().enumerate() {
                if i == 0 {
                    p.move_to(*q);
                } else {
                    p.line_to(*q);
                }
            }
            if fechado {
                p.close();
            }
        })
    }
    pub fn bounds(&self) -> Rect {
        let b = self.0.bounds();
        Rect { x: b.left(), y: b.top(), width: b.width(), height: b.height() }
    }
    pub fn transform(&self, t: Transform) -> Option<Path> {
        self.0.clone().transform(t.0).map(Path)
    }
    /// Os comandos do caminho, na ordem (para quem quer os pontos: deformar, achatar, exportar).
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        use tiny_skia::PathSegment as S;
        let p = |q: tiny_skia::Point| Point::new(q.x, q.y);
        self.0.segments().map(move |s| match s {
            S::MoveTo(a) => Segment::MoveTo(p(a)),
            S::LineTo(a) => Segment::LineTo(p(a)),
            S::QuadTo(c, a) => Segment::QuadTo(p(c), p(a)),
            S::CubicTo(c1, c2, a) => Segment::CubicTo(p(c1), p(c2), p(a)),
            S::Close => Segment::Close,
        })
    }
}

/// Um comando de [`Path::segments`]; os pontos de controle vêm antes do ponto final.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

/// Mover, girar e escalar, compostos. `a.then(b)`: primeiro `a`, depois `b`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform(pub(crate) tiny_skia::Transform);

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Transform = Transform(tiny_skia::Transform { sx: 1.0, kx: 0.0, ky: 0.0, sy: 1.0, tx: 0.0, ty: 0.0 });
    pub fn translate(x: f32, y: f32) -> Self {
        Transform(tiny_skia::Transform::from_translate(x, y))
    }
    pub fn scale(x: f32, y: f32) -> Self {
        Transform(tiny_skia::Transform::from_scale(x, y))
    }
    /// Gira `graus` (horário na tela) em volta da origem.
    pub fn rotate(graus: f32) -> Self {
        Transform(tiny_skia::Transform::from_rotate(graus))
    }
    /// Gira `graus` em volta de `centro`.
    pub fn rotate_around(graus: f32, centro: Point) -> Self {
        Transform(tiny_skia::Transform::from_rotate_at(graus, centro.x, centro.y))
    }
    /// `(sx, ky, kx, sy, tx, ty)`, como no SVG/PDF: x' = sx·x + kx·y + tx, y' = ky·x + sy·y + ty.
    pub fn from_row(sx: f32, ky: f32, kx: f32, sy: f32, tx: f32, ty: f32) -> Self {
        Transform(tiny_skia::Transform::from_row(sx, ky, kx, sy, tx, ty))
    }
    pub fn then(self, depois: Transform) -> Self {
        Transform(self.0.post_concat(depois.0))
    }
    pub fn apply(self, p: Point) -> Point {
        let mut q = [tiny_skia::Point::from_xy(p.x, p.y)];
        self.0.map_points(&mut q);
        Point::new(q[0].x, q[0].y)
    }
    /// A volta (de um ponto da tela para o espaço da forma: acertar o clique numa forma girada).
    pub fn invert(self) -> Option<Self> {
        self.0.invert().map(Transform)
    }
    pub fn is_identity(self) -> bool {
        self.0.is_identity()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Como contornar um [`Path`].
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Tracejado: traço, espaço, traço… (lógicos). Vazio = contínuo.
    pub dash: Vec<f32>,
}

impl Stroke {
    pub fn new(width: f32) -> Self {
        Stroke { width, cap: LineCap::Butt, join: LineJoin::Miter, dash: Vec::new() }
    }
    pub fn cap(mut self, c: LineCap) -> Self {
        self.cap = c;
        self
    }
    pub fn join(mut self, j: LineJoin) -> Self {
        self.join = j;
        self
    }
    pub fn dash(mut self, d: impl Into<Vec<f32>>) -> Self {
        self.dash = d.into();
        self
    }
}

impl Renderer {
    /// O que vem até o `pop_transform` é desenhado transformado por `t` (depois das de fora).
    /// Vale para caminhos, imagens e [`Renderer::fill_text`]; os widgets comuns não.
    pub fn push_transform(&mut self, t: Transform) {
        let atual = self.transformacoes.last().copied().unwrap_or_default();
        self.transformacoes.push(t.then(atual));
    }
    pub fn pop_transform(&mut self) {
        self.transformacoes.pop();
    }
    fn transformacao(&self) -> Transform {
        self.transformacoes.last().copied().unwrap_or_default()
    }
    /// Do lógico (com a transformação do usuário) para os pixels do pixmap atual.
    fn para_pixels(&self) -> tiny_skia::Transform {
        let s = self.scale;
        self.transformacao().0.post_translate(-self.origem.x, -self.origem.y).post_scale(s, s)
    }

    /// A área (lógica, na tela) que o caminho cobre, e a impressão digital dele.
    fn anotar(&mut self, p: &Path, margem: f32, k: &mut Chave) -> bool {
        if !self.gravando {
            return false;
        }
        let t = self.transformacao();
        k.h(p.0.points().len());
        for q in p.0.points() {
            k.f(q.x).f(q.y);
        }
        let tt = t.0;
        k.f(tt.sx).f(tt.ky).f(tt.kx).f(tt.sy).f(tt.tx).f(tt.ty);
        let area = p.transform(t).map_or(Rect::default(), |p| p.bounds());
        self.gravou(super::crescer(area, margem + 1.0), std::mem::replace(k, Chave::new(0)));
        true
    }

    fn pincel(&self, fundo: &Background, caixa: Rect) -> Paint<'static> {
        match fundo {
            Background::Color(c) => paint(self.esmaecer(*c)),
            Background::Linear { angle, stops } => {
                let (cx, cy, dx, dy) = (caixa.x + caixa.width / 2.0, caixa.y + caixa.height / 2.0, angle.cos(), angle.sin());
                let meio = (caixa.width * dx.abs() + caixa.height * dy.abs()) / 2.0;
                let paradas = stops.iter().map(|(t, c)| GradientStop::new(*t, skia_color(self.esmaecer(*c)))).collect();
                match LinearGradient::new((cx - dx * meio, cy - dy * meio).into(), (cx + dx * meio, cy + dy * meio).into(), paradas, SpreadMode::Pad, self.para_pixels()) {
                    Some(shader) => Paint { shader, anti_alias: true, ..Default::default() },
                    None => paint(stops.first().map(|s| self.esmaecer(s.1)).unwrap_or_default()),
                }
            }
        }
    }

    /// Preenche `p` com cor ou degradê (o degradê vai de ponta a ponta da caixa do caminho).
    pub fn fill_path(&mut self, p: &Path, fundo: impl Into<Background>, regra: FillRule) {
        let fundo = fundo.into();
        let mut k = Chave::new(9);
        k.h(regra == FillRule::EvenOdd);
        match &fundo {
            Background::Color(c) => k.c(*c),
            Background::Linear { angle, stops } => {
                k.f(*angle);
                for (t, c) in stops {
                    k.f(*t).c(*c);
                }
                &mut k
            }
        };
        self.falta("caminho preenchido");
        if self.anotar(p, 0.0, &mut k) {
            return;
        }
        let Some(dev) = p.0.clone().transform(self.para_pixels()) else { return };
        let pincel = self.pincel(&fundo, p.bounds());
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let mask = clip_mask(&mut self.clips, pw, ph, self.scale, Some(dev.bounds()));
        let regra = if regra == FillRule::EvenOdd { tiny_skia::FillRule::EvenOdd } else { tiny_skia::FillRule::Winding };
        self.pixmap.fill_path(&dev, &pincel, regra, tiny_skia::Transform::identity(), mask);
    }

    /// Contorna `p`. A largura escala junto com a transformação.
    pub fn stroke_path(&mut self, p: &Path, traco: &Stroke, cor: impl Into<Background>) {
        let cor = cor.into();
        let mut k = Chave::new(10);
        k.f(traco.width).h(traco.cap as u8).h(traco.join as u8).h(traco.dash.iter().map(|d| d.to_bits()).collect::<Vec<_>>());
        if let Background::Color(c) = &cor {
            k.c(*c);
        } else {
            k.h(format!("{cor:?}"));
        }
        let escala = {
            let t = self.transformacao().0;
            ((t.sx * t.sy - t.kx * t.ky).abs()).sqrt()
        };
        // Quanto o traço passa do caminho: a ponta da junção em quina vai até 2× a largura (limite 4).
        let sobra = traco.width * escala * if traco.join == LineJoin::Miter { 2.0 } else if traco.cap == LineCap::Square { 0.71 } else { 0.5 };
        // Na GPU só a linha quebrada (retas) com pontas e junções redondas, de cor lisa: cada reta é uma cápsula.
        let retas = p.0.segments().all(|s| !matches!(s, tiny_skia::PathSegment::QuadTo(..) | tiny_skia::PathSegment::CubicTo(..)));
        let cor_lisa = match &cor {
            Background::Color(c) => Some(self.esmaecer(*c)),
            Background::Linear { .. } => None,
        };
        let opaca = cor_lisa.is_some_and(|c| c.a >= 1.0) || p.0.len() <= 2;
        if !(retas && opaca && traco.cap == LineCap::Round && traco.join == LineJoin::Round && traco.dash.is_empty() && cor_lisa.is_some()) {
            self.falta("contorno com curva, ponta reta, tracejado ou translúcido");
        }
        if self.anotar(p, sobra, &mut k) {
            return;
        }
        if self.pintando_gpu {
            let (t, mut ini, mut ant) = (self.transformacao(), None, None);
            let mut segs = Vec::new();
            for s in p.0.segments() {
                match s {
                    tiny_skia::PathSegment::MoveTo(a) => (ini, ant) = (Some(t.apply(Point::new(a.x, a.y))), Some(t.apply(Point::new(a.x, a.y)))),
                    tiny_skia::PathSegment::LineTo(a) => {
                        let q = t.apply(Point::new(a.x, a.y));
                        segs.extend(ant.map(|o| (o, q)));
                        ant = Some(q);
                    }
                    tiny_skia::PathSegment::Close => {
                        segs.extend(ant.zip(ini));
                        ant = ini;
                    }
                    _ => {}
                }
            }
            if let Some(c) = cor_lisa {
                self.traco_gpu(segs.into_iter(), traco.width * escala, c);
            }
            return;
        }
        let ts = tiny_skia::Stroke {
            width: traco.width,
            line_cap: match traco.cap {
                LineCap::Butt => tiny_skia::LineCap::Butt,
                LineCap::Round => tiny_skia::LineCap::Round,
                LineCap::Square => tiny_skia::LineCap::Square,
            },
            line_join: match traco.join {
                LineJoin::Miter => tiny_skia::LineJoin::Miter,
                LineJoin::Round => tiny_skia::LineJoin::Round,
                LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
            },
            dash: (traco.dash.len() >= 2).then(|| tiny_skia::StrokeDash::new(traco.dash.clone(), 0.0)).flatten(),
            ..Default::default()
        };
        let pincel = self.pincel(&cor, p.bounds());
        let t = self.para_pixels();
        let Some(dev) = p.0.clone().transform(t) else { return };
        let meia = sobra * self.scale;
        let b = dev.bounds();
        let area = tiny_skia::Rect::from_ltrb(b.left() - meia, b.top() - meia, b.right() + meia, b.bottom() + meia);
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let mask = clip_mask(&mut self.clips, pw, ph, self.scale, area);
        self.pixmap.stroke_path(&p.0, &pincel, &ts, t, mask);
    }

    /// Os contornos das letras de `text` como caminhos (lógicos, o topo da primeira linha em
    /// `at`): texto girado, WordArt, texto vazado.
    pub fn text_path(&mut self, text: &str, size: f32, font: Font, at: Point, max_width: Option<f32>) -> Option<Path> {
        let s = self.scale;
        self.buffer(text, size, font, max_width, super::ALTURA_LINHA);
        let key = super::TextKey { text: text.to_owned(), size: (size * s * 64.0) as u32, font, width: max_width.map(|w| (w * s) as u32), line_height: super::ALTURA_LINHA.to_bits() };
        let b = self.texts.get(&key)?;
        let mut pb = PathBuilder::new();
        for run in b.layout_runs() {
            for g in run.glyphs {
                let fis = g.physical((0., 0.), 1.0);
                let Some(cmds) = self.swash.get_outline_commands(&mut self.fonts, fis.cache_key) else { continue };
                // O contorno vem em pixels com y para cima, a partir da linha de base do glifo.
                let (ox, oy) = (fis.x as f32, run.line_y + fis.y as f32);
                let q = |x: f32, y: f32| Point::new(at.x + (ox + x) / s, at.y + (oy - y) / s);
                for c in cmds {
                    match c {
                        Command::MoveTo(p) => pb.move_to(q(p.x, p.y)),
                        Command::LineTo(p) => pb.line_to(q(p.x, p.y)),
                        Command::QuadTo(c, p) => pb.quadratic_to(q(c.x, c.y), q(p.x, p.y)),
                        Command::CurveTo(c1, c2, p) => pb.bezier_to(q(c1.x, c1.y), q(c2.x, c2.y), q(p.x, p.y)),
                        Command::Close => pb.close(),
                    };
                }
            }
        }
        pb.build()
    }

    /// Texto que segue a transformação (girado, inclinado): os contornos das letras preenchidos.
    pub fn fill_text(&mut self, text: &str, size: f32, font: Font, color: Color, at: Point) {
        if let Some(p) = self.text_path(text, size, font, at, None) {
            self.fill_path(&p, color, FillRule::NonZero);
        }
    }
}
