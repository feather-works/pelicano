//! Texto rico: trechos com fonte, tamanho, cor, itálico, sublinhado, tachado, fundo e link
//! próprios, quebrando linha juntos como um parágrafo só.
use crate::widget::{CursorIcon, DrawCx, EventCx, Node, Widget};
use crate::{Border, Captured, Color, Event, Font, LayoutCx, Length, Limits, MouseButton, Point, Rect, Size};
use cosmic_text::{Attrs, Buffer, Metrics, Shaping, Style as Estilo};
use std::borrow::Cow;
use std::hash::{Hash, Hasher};

pub struct Span<'a, M> {
    text: Cow<'a, str>,
    size: Option<f32>,
    font: Option<Font>,
    color: Option<Color>,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    background: Option<Color>,
    link: Option<M>,
}

/// Um trecho de um [`rich_text`]; o que não for dito vem do texto todo.
pub fn span<'a, M>(text: impl Into<Cow<'a, str>>) -> Span<'a, M> {
    Span { text: text.into(), size: None, font: None, color: None, italic: false, underline: false, strikethrough: false, background: None, link: None }
}

impl<M> Span<'_, M> {
    pub fn size(mut self, s: f32) -> Self {
        self.size = Some(s);
        self
    }
    pub fn font(mut self, f: Font) -> Self {
        self.font = Some(f);
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    /// Fundo atrás do trecho (código em linha, marca-texto).
    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
    /// Clicar no trecho manda `m`; ganha a cor de destaque e sublinhado se não tiver cor própria.
    pub fn link(mut self, m: M) -> Self {
        self.link = Some(m);
        self
    }
}

pub struct RichText<'a, M> {
    spans: Vec<Span<'a, M>>,
    size: f32,
    font: Font,
    color: Option<Color>,
    width: Length,
    line_height: f32,
    wrap: bool,
}

/// Um parágrafo feito de [`span`]s.
pub fn rich_text<'a, M>(spans: impl IntoIterator<Item = Span<'a, M>>) -> RichText<'a, M> {
    RichText { spans: spans.into_iter().collect(), size: 14.0, font: Font::DEFAULT, color: None, width: Length::Shrink, line_height: 1.3, wrap: true }
}

impl<M> RichText<'_, M> {
    pub fn size(mut self, s: impl Into<crate::Num>) -> Self {
        self.size = s.into().0;
        self
    }
    pub fn font(mut self, f: Font) -> Self {
        self.font = f;
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    /// Altura da linha, em vezes o tamanho do maior trecho dela. Padrão: 1,3.
    pub fn line_height(mut self, f: f32) -> Self {
        self.line_height = f;
        self
    }
    pub fn no_wrap(mut self) -> Self {
        self.wrap = false;
        self
    }

    fn chave(&self, largura: Option<f32>, link: Color) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (self.size.to_bits(), self.font, self.line_height.to_bits(), largura.map(f32::to_bits), link.r.to_bits(), link.g.to_bits(), link.b.to_bits()).hash(&mut h);
        for s in &self.spans {
            (&s.text, s.size.map(f32::to_bits), s.font, s.italic, s.link.is_some()).hash(&mut h);
            s.color.map(|c| [c.r, c.g, c.b, c.a].map(f32::to_bits)).hash(&mut h);
        }
        h.finish()
    }

    /// O buffer montado (em cache no renderer) e a chave dele.
    fn buffer<'r>(&self, r: &'r mut crate::Renderer, largura: Option<f32>, link: Color) -> (u64, &'r Buffer) {
        let largura = largura.filter(|w| self.wrap && w.is_finite());
        let chave = self.chave(largura, link);
        let b = r.proprio(chave, |fonts, s| {
            let maior = self.spans.iter().filter_map(|t| t.size).fold(self.size, f32::max);
            let mut b = Buffer::new(fonts, Metrics::new(self.size * s, (maior * self.line_height * s).ceil()));
            b.set_size(fonts, largura.map(|w| w * s), None);
            let trechos = self.spans.iter().enumerate().map(|(i, t)| {
                let f = t.font.unwrap_or(self.font);
                let mut a = crate::render::attrs(f).metadata(i);
                if let Some(tam) = t.size {
                    a = a.metrics(Metrics::new(tam * s, (tam * self.line_height * s).ceil()));
                }
                if t.italic {
                    a = a.style(Estilo::Italic);
                }
                if let Some(c) = t.color.or(t.link.as_ref().map(|_| link)) {
                    a = a.color(cosmic_text::Color::rgba((c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8, (c.a * 255.0) as u8));
                }
                (t.text.as_ref(), a)
            });
            b.set_rich_text(fonts, trechos, Attrs::new(), Shaping::Advanced);
            b
        });
        (chave, b)
    }

    /// Caixas (lógicas, relativas ao canto) de cada pedaço de trecho que `quer` aceitar.
    fn caixas(b: &Buffer, s: f32, quer: impl Fn(usize) -> bool) -> Vec<(Rect, usize, f32)> {
        let mut v: Vec<(Rect, usize, f32)> = Vec::new();
        for run in b.layout_runs() {
            for g in run.glyphs.iter().filter(|g| quer(g.metadata)) {
                let r = Rect { x: g.x / s, y: run.line_top / s, width: g.w / s, height: run.line_height / s };
                match v.last_mut() {
                    // Glifos seguidos do mesmo trecho na mesma linha viram uma caixa só.
                    Some((u, i, _)) if *i == g.metadata && u.y == r.y && (u.x + u.width - r.x).abs() < 0.5 => u.width = r.x + r.width - u.x,
                    _ => v.push((r, g.metadata, (run.line_y - run.line_top) / s)),
                }
            }
        }
        v
    }

    fn trecho_em(b: &Buffer, s: f32, p: Point) -> Option<usize> {
        Self::caixas(b, s, |_| true).into_iter().find(|(r, ..)| r.contains(p)).map(|(_, i, _)| i)
    }
}

impl<M: Clone> Widget<M> for RichText<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let largura = match self.width {
            Length::Fixed(v) => v.min(l.max.width),
            _ => l.max.width,
        };
        let s = cx.renderer.scale();
        let link = cx.theme.primary;
        let (_, b) = self.buffer(cx.renderer, Some(largura), link);
        let (mut w, mut h) = (0.0f32, 0.0f32);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            h = run.line_top + run.line_height;
        }
        let m = Size::new((w / s).ceil(), (h / s).ceil());
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, m.width), m.height.min(l.max.height)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let r = n.rect;
        let s = cx.renderer.scale();
        let link = cx.theme.primary;
        let (chave, b) = self.buffer(cx.renderer, Some(r.width), link);
        let decorados = |i: usize| self.spans.get(i).is_some_and(|t| t.background.is_some() || t.underline || t.strikethrough || t.link.is_some());
        let caixas = Self::caixas(b, s, decorados);
        let sob = cx.cursor.and_then(|p| Self::trecho_em(b, s, Point::new(p.x - r.x, p.y - r.y)));
        if sob.is_some_and(|i| self.spans[i].link.is_some()) {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        let cor = self.color.unwrap_or(cx.text_color);
        for &(c, i, _) in &caixas {
            if let Some(f) = self.spans[i].background {
                cx.renderer.fill_quad(Rect { x: r.x + c.x - 2.0, y: r.y + c.y, width: c.width + 4.0, height: c.height }, Some(&f.into()), Border::rounded(3.0), None);
            }
        }
        let maior = self.spans.iter().filter_map(|t| t.size).fold(self.size, f32::max);
        cx.renderer.draw_proprio(chave, Point::new(r.x, r.y), cor, crate::render::crescer(r, crate::render::sobra_glifos(maior, self.line_height)));
        for &(c, i, base) in &caixas {
            let t = &self.spans[i];
            let tom = t.color.unwrap_or(if t.link.is_some() { link } else { cor });
            let tam = t.size.unwrap_or(self.size);
            if t.underline || t.link.is_some() {
                cx.renderer.fill_quad(Rect { x: r.x + c.x, y: r.y + c.y + base + 2.0, width: c.width, height: 1.0 }, Some(&tom.into()), Border::default(), None);
            }
            if t.strikethrough {
                cx.renderer.fill_quad(Rect { x: r.x + c.x, y: r.y + c.y + base - tam * 0.3, width: c.width, height: 1.0 }, Some(&tom.into()), Border::default(), None);
            }
        }
        let tudo: String = self.spans.iter().map(|t| t.text.as_ref()).collect();
        cx.access_text(n.key, r, &tudo);
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let (Event::MousePressed(MouseButton::Left), Some(p)) = (ev, cx.cursor) else { return Captured::No };
        if !n.rect.contains(p) || self.spans.iter().all(|t| t.link.is_none()) {
            return Captured::No;
        }
        let s = cx.renderer.scale();
        let (_, b) = self.buffer(cx.renderer, Some(n.rect.width), Color::TRANSPARENT);
        match Self::trecho_em(b, s, Point::new(p.x - n.rect.x, p.y - n.rect.y)).and_then(|i| self.spans[i].link.clone()) {
            Some(m) => {
                cx.publish(m);
                Captured::Yes
            }
            None => Captured::No,
        }
    }
}

impl<'a, M: Clone + 'a> From<RichText<'a, M>> for crate::Element<'a, M> {
    fn from(w: RichText<'a, M>) -> Self {
        crate::Element::new(w)
    }
}
