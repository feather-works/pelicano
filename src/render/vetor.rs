//! Desenho vetorial no [`Renderer`]: caminhos com curvas, preenchidos (cor ou degradê) ou
//! contornados, debaixo de uma transformação (mover, girar, escalar) — o `Frame` do canvas.
use super::{clip_mask, paint, skia_color, Chave, Renderer};
use std::hash::Hasher;
use std::sync::Arc;
use tiny_skia::Pixmap;
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
    /// O deslocamento da transformação atual, se ela só desloca. Aí dá para usar
    /// `fill_quad`/`draw_text` (que vão à GPU e usam o cache de glifos) somando esse ponto.
    pub fn translation(&self) -> Option<Point> {
        let t = self.transformacao().0;
        (t.sx == 1.0 && t.sy == 1.0 && t.kx == 0.0 && t.ky == 0.0).then(|| Point::new(t.tx, t.ty))
    }

    /// A caixa do caminho (com `margem` lógica) passa do que uma textura em cache aguenta.
    fn grande_para_gpu(&self, p: &Path, margem: f32) -> bool {
        let b = p.transform(self.transformacao()).map_or(Rect::default(), |p| p.bounds());
        let s = self.scale;
        // Só a parte na janela vai à textura (`caminho_gpu`): com zoom, o contorno do slide passa dela.
        let j = self.window_size();
        let (x0, y0) = ((b.x - margem).max(-1.0), (b.y - margem).max(-1.0));
        let (x1, y1) = ((b.x + b.width + margem).min(j.width + 1.0), (b.y + b.height + margem).min(j.height + 1.0));
        let (w, h) = ((x1 - x0).max(0.0) * s + 2.0, (y1 - y0).max(0.0) * s + 2.0);
        let max = if self.gpu.max_textura > 0 { self.gpu.max_textura as f32 } else { 2048.0 };
        w * h > (1u64 << 22) as f32 || w > max || h > max
    }

    /// Na GPU: `dev` (em pixels) é rasterizado pela CPU só na caixa dele e vai como textura em
    /// cache. A chave é a forma relativa ao canto da caixa: mover por pixels inteiros reaproveita.
    fn caminho_gpu(&mut self, dev: &tiny_skia::Path, fundo: &Background, logico: Rect, regra: tiny_skia::FillRule) {
        // A posição vai para o quarto de pixel mais perto: arrastando (com zoom, quase sempre em
        // fração de pixel) cada forma reaproveita uma de 16 rasterizações em vez de refazer.
        let b = dev.bounds();
        let quarto = |v: f32| (v * 4.0).round() / 4.0 - v;
        let (ax, ay) = (quarto(b.left() - b.left().floor()), quarto(b.top() - b.top().floor()));
        let Some(dev) = dev.clone().transform(tiny_skia::Transform::from_translate(ax, ay)) else { return };
        let dev = &dev;
        let b = dev.bounds();
        // Rasteriza só o que cai na janela (os pontos entram na chave relativos a este canto).
        let (jw, jh) = (self.tela.0 as f32 + 2.0, self.tela.1 as f32 + 2.0);
        let (x0, y0) = (b.left().floor().max(-2.0), b.top().floor().max(-2.0));
        let (x1, y1) = (b.right().ceil().min(jw), b.bottom().ceil().min(jh));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (w, h) = ((x1 - x0) as u32 + 1, (y1 - y0) as u32 + 1);
        if w as u64 * h as u64 > 1 << 22 {
            return;
        }
        let mut k = Chave::new(11);
        k.h(regra == tiny_skia::FillRule::EvenOdd).h(w).h(h);
        match fundo {
            Background::Color(c) => k.c(*c),
            // O degradê vem da caixa lógica e da transformação: só reaproveita parado.
            Background::Linear { angle, stops } => {
                k.f(*angle).r(logico).f(x0).f(y0);
                let t = self.para_pixels();
                k.f(t.sx).f(t.ky).f(t.kx).f(t.sy).f(t.tx).f(t.ty);
                for (t, c) in stops {
                    k.f(*t).c(*c);
                }
                &mut k
            }
        };
        for q in dev.points() {
            k.f(((q.x - x0) * 16.0).round()).f(((q.y - y0) * 16.0).round());
        }
        let chave = k.0.finish();
        // Bits de cima 01: não colide com as imagens nem com os SVGs (bit 63).
        let id = chave >> 2 | 1 << 62;
        // A opacidade vai na instância (`imagem_gpu`), não nos pixels. O deslocamento do
        // `fill_path` abaixo também move o degradê.
        let opacidade = std::mem::replace(&mut self.opacidade, 1.0);
        let pincel = self.pincel(fundo, logico);
        self.opacidade = opacidade;
        self.caminhos.aparar(self.caminhos.len() > 1024 || self.caminhos.bytes() > 64 << 20);
        let pm = self
            .caminhos
            .pegar(chave, || {
                let mut pm = Pixmap::new(w, h).expect("tamanho > 0");
                pm.fill_path(dev, &pincel, regra, tiny_skia::Transform::from_translate(-x0, -y0), None);
                Arc::new(pm)
            })
            .clone();
        self.imagem_gpu(id, &pm, [x0, y0, w as f32, h as f32], [0.0; 4]);
    }
    /// Do lógico para os pixels da janela inteira (o `para_pixels` de um quadro cheio).
    fn para_janela(&self) -> tiny_skia::Transform {
        self.transformacao().0.post_scale(self.scale, self.scale)
    }

    /// Preenche `janela` (em pixels da janela) num quadro parcial. Direto no pixmap da região,
    /// o tiny-skia corta as arestas na borda dela e refaz as inclinações: a borda da forma sai
    /// diferente da do quadro cheio (rastro). A cobertura vai numa máscara da caixa da forma na
    /// janela, cortada só onde o quadro cheio corta, e se pinta a parte que cai na região.
    pub(super) fn preencher_parcial(&mut self, janela: &tiny_skia::Path, pincel: &Paint, regra: tiny_skia::FillRule) {
        let Some(par) = &self.parcial else { return };
        let (jw, jh) = (par.cheio.width() as i32, par.cheio.height() as i32);
        let (ex, ey) = self.desloc;
        let (pw, ph) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        let b = janela.bounds();
        let (bx0, by0) = ((b.left().floor() as i32).max(0), (b.top().floor() as i32).max(0));
        let (bx1, by1) = ((b.right().ceil() as i32).min(jw), (b.bottom().ceil() as i32).min(jh));
        // A parte da forma dentro da região, em pixels da janela.
        let (x0, y0, x1, y1) = (bx0.max(ex), by0.max(ey), bx1.min(ex + pw), by1.min(ey + ph));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let exata = tiny_skia::Rect::from_ltrb(b.left() - ex as f32, b.top() - ey as f32, b.right() - ex as f32, b.bottom() - ey as f32);
        if (x0, y0, x1, y1) == (bx0, by0, bx1, by1) {
            // Inteira na região: nada a cortar, direto (deslocar por pixels inteiros é exato).
            let Some(dev) = janela.clone().transform(tiny_skia::Transform::from_translate(-ex as f32, -ey as f32)) else { return };
            let mask = clip_mask(&mut self.clips, pw as u32, ph as u32, self.scale, exata);
            return self.pixmap.fill_path(&dev, pincel, regra, tiny_skia::Transform::identity(), mask);
        }
        let Some(mut cob) = tiny_skia::Mask::new((bx1 - bx0) as u32, (by1 - by0) as u32) else { return };
        // Deslocar por pixels inteiros não muda a cobertura (o tiny-skia rasteriza em ponto fixo).
        cob.fill_path(janela, regra, true, tiny_skia::Transform::from_translate(-bx0 as f32, -by0 as f32));
        let Some(mut m) = tiny_skia::Mask::new(pw as u32, ph as u32) else { return };
        let (cw, n) = ((bx1 - bx0) as usize, (x1 - x0) as usize);
        for y in y0..y1 {
            let o = (y - by0) as usize * cw + (x0 - bx0) as usize;
            let d = (y - ey) as usize * pw as usize + (x0 - ex) as usize;
            m.data_mut()[d..d + n].copy_from_slice(&cob.data()[o..o + n]);
        }
        // O recorte decide pela caixa exata da forma, como no quadro cheio (a arredondada para
        // pixels passaria da borda de um recorte fracionário e cortaria a coluna do antisserrilhado).
        if let Some(c) = clip_mask(&mut self.clips, pw as u32, ph as u32, self.scale, exata) {
            for (a, c) in m.data_mut().iter_mut().zip(c.data()) {
                *a = ((*a as u32 * *c as u32 + 127) / 255) as u8;
            }
        }
        if let Some(r) = tiny_skia::Rect::from_ltrb((x0 - ex) as f32, (y0 - ey) as f32, (x1 - ex) as f32, (y1 - ey) as f32) {
            self.pixmap.fill_rect(r, pincel, tiny_skia::Transform::identity(), Some(&m));
        }
    }

    /// Do lógico (com a transformação do usuário) para os pixels do pixmap atual.
    fn para_pixels(&self) -> tiny_skia::Transform {
        let s = self.scale;
        self.transformacao().0.post_translate(-self.origem.x, -self.origem.y).post_scale(s, s)
    }

    /// A área (lógica, na tela) que o caminho cobre, e a impressão digital dele.
    fn anotar(&mut self, p: &Path, margem: f32, k: &mut Chave) -> bool {
        let t = self.transformacao();
        k.h(p.0.points().len());
        for q in p.0.points() {
            k.f(q.x).f(q.y);
        }
        let tt = t.0;
        k.f(tt.sx).f(tt.ky).f(tt.kx).f(tt.sy).f(tt.tx).f(tt.ty);
        let k = std::mem::replace(k, Chave::new(0));
        if !self.gravando {
            return self.gravou(Rect::default(), k);
        }
        let area = p.transform(t).map_or(Rect::default(), |p| p.bounds());
        self.gravou(super::crescer(area, margem + 1.0), k)
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
        if self.gravando && self.grande_para_gpu(p, 0.0) {
            self.falta("caminho grande demais");
        }
        if self.anotar(p, 0.0, &mut k) {
            return;
        }
        let Some(dev) = p.0.clone().transform(self.para_pixels()) else { return };
        if self.pintando_gpu {
            let regra = if regra == FillRule::EvenOdd { tiny_skia::FillRule::EvenOdd } else { tiny_skia::FillRule::Winding };
            return self.caminho_gpu(&dev, &fundo, p.bounds(), regra);
        }
        let pincel = self.pincel(&fundo, p.bounds());
        let regra = if regra == FillRule::EvenOdd { tiny_skia::FillRule::EvenOdd } else { tiny_skia::FillRule::Winding };
        if self.parcial.is_some() && self.camadas.is_empty() {
            if let Some(janela) = p.0.clone().transform(self.para_janela()) {
                self.preencher_parcial(&janela, &pincel, regra);
            }
            return;
        }
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let mask = clip_mask(&mut self.clips, pw, ph, self.scale, Some(dev.bounds()));
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
        // O resto vai à GPU como o caminho preenchido: o contorno rasterizado numa textura.
        let capsulas = retas && opaca && traco.cap == LineCap::Round && traco.join == LineJoin::Round && traco.dash.is_empty() && cor_lisa.is_some();
        if self.gravando && !capsulas && self.grande_para_gpu(p, sobra) {
            self.falta("contorno grande demais");
        }
        if self.anotar(p, sobra, &mut k) {
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
        if self.pintando_gpu && !capsulas {
            let t = self.para_pixels();
            let res = tiny_skia::PathStroker::compute_resolution_scale(&t);
            // `Path::stroke` não traceja: o `stroke_path` da CPU traceja antes, igual aqui.
            let tracejado = ts.dash.as_ref().and_then(|d| p.0.dash(d, res));
            let contorno = tracejado.as_ref().unwrap_or(&p.0).stroke(&ts, res);
            if let Some(dev) = contorno.and_then(|c| c.transform(t)) {
                self.caminho_gpu(&dev, &cor, p.bounds(), tiny_skia::FillRule::Winding);
            }
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
        let pincel = self.pincel(&cor, p.bounds());
        // Num quadro parcial, o contorno vira caminho como o `stroke_path` do tiny-skia faz (com a
        // transformação da janela) e é preenchido como o cheio; o traço fino (até 1 pixel, que o
        // tiny-skia faz como linha fina, com a mesma conta do `treat_as_hairline` dele) não.
        let t = self.para_janela();
        let curto = |x: f32, y: f32| x.abs().max(y.abs()) + x.abs().min(y.abs()) / 2.0;
        let fino = traco.width == 0.0 || (curto(t.sx * traco.width, t.ky * traco.width) <= 1.0 && curto(t.kx * traco.width, t.sy * traco.width) <= 1.0);
        if self.parcial.is_some() && self.camadas.is_empty() && !fino {
            let res = tiny_skia::PathStroker::compute_resolution_scale(&t);
            let tracejado = ts.dash.as_ref().and_then(|d| p.0.dash(d, res));
            if let Some(janela) = tracejado.as_ref().unwrap_or(&p.0).stroke(&ts, res).and_then(|c| c.transform(t)) {
                self.preencher_parcial(&janela, &pincel, tiny_skia::FillRule::Winding);
            }
            return;
        }
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
