//! Controles prontos: checkbox, toggler, dropdown, slider e régua.
use crate::widget::{CursorIcon, DrawCx, Element, EventCx, LayoutCx, Limits, Node, Num, Widget};
use crate::{Background, Border, Captured, Color, Event, Font, Key, Length, MouseButton, NamedKey, Point, Rect, Shadow, Size, Status};
use std::borrow::Cow;

fn status(desabilitado: bool, hover: bool) -> Status {
    match (desabilitado, hover) {
        (true, _) => Status::Disabled,
        (_, true) => Status::Hovered,
        _ => Status::Idle,
    }
}

// ---------- Checkbox e toggler ----------

#[derive(Clone, Copy, PartialEq)]
enum Forma {
    Caixa,
    Chave,
    Radio,
}

/// Checkbox e toggler: o mesmo comportamento, só o desenho muda.
pub struct Toggle<'a, M> {
    forma: Forma,
    label: Cow<'a, str>,
    checked: bool,
    on_toggle: Option<Box<dyn Fn(bool) -> M + 'a>>,
    size: f32,
    color: Option<Color>,
}

pub fn checkbox<'a, M>(label: impl Into<Cow<'a, str>>, checked: bool) -> Toggle<'a, M> {
    Toggle { forma: Forma::Caixa, label: label.into(), checked, on_toggle: None, size: 14.0, color: None }
}

/// Escolha única: marcado quando `selected == Some(value)`; o clique manda `value`.
pub fn radio<'a, V: Copy + PartialEq + 'a, M>(label: impl Into<Cow<'a, str>>, value: V, selected: Option<V>, on_select: impl Fn(V) -> M + 'a) -> Toggle<'a, M> {
    Toggle { forma: Forma::Radio, ..checkbox(label, selected == Some(value)) }.on_toggle(move |_| on_select(value))
}

/// Chave liga/desliga.
pub fn toggler<'a, M>(label: impl Into<Cow<'a, str>>, on: bool) -> Toggle<'a, M> {
    Toggle { forma: Forma::Chave, ..checkbox(label, on) }
}

impl<'a, M> Toggle<'a, M> {
    pub fn on_toggle(mut self, f: impl Fn(bool) -> M + 'a) -> Self {
        self.on_toggle = Some(Box::new(f));
        self
    }
    pub fn size(mut self, s: impl Into<Num>) -> Self {
        self.size = s.into().0;
        self
    }
    /// A cor de quando está marcado.
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    fn marca(&self) -> Size {
        match self.forma {
            Forma::Caixa | Forma::Radio => Size::new(16.0, 16.0),
            Forma::Chave => Size::new(34.0, 18.0),
        }
    }
}

impl<M> Widget<M> for Toggle<'_, M> {
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let m = self.marca();
        let t = if self.label.is_empty() { Size::ZERO } else { cx.renderer.measure(&self.label, self.size, Font::DEFAULT, None) };
        let gap = if self.label.is_empty() { 0.0 } else { 8.0 };
        Node::leaf(cx.key(), Size::new((m.width + gap + t.width).min(l.max.width), m.height.max(t.height)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let hover = cx.hovered(&n.rect);
        let st = status(self.on_toggle.is_none(), hover);
        if hover && self.on_toggle.is_some() {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        let t = *cx.theme;
        let ativa = self.color.unwrap_or(t.primary);
        let m = self.marca();
        let r = Rect { x: n.rect.x, y: n.rect.y + (n.rect.height - m.height) / 2.0, width: m.width, height: m.height };
        let apagado = if st == Status::Disabled { 0.45 } else { 1.0 };
        match self.forma {
            Forma::Caixa => {
                let (bg, borda) = if self.checked { (ativa, ativa) } else { (t.surface, if hover { t.muted } else { t.border }) };
                cx.renderer.fill_quad(r, Some(&bg.alpha(apagado).into()), Border::rounded(4.0).color(borda.alpha(apagado), 1.0), None);
                if self.checked {
                    let p = |x: f32, y: f32| Point::new(r.x + x, r.y + y);
                    cx.renderer.stroke(&[p(4.0, 8.5), p(7.0, 11.5), p(12.0, 5.0)], 2.0, t.on_primary);
                }
            }
            Forma::Radio => {
                let borda = if self.checked { ativa } else if hover { t.muted } else { t.border };
                cx.renderer.fill_quad(r, Some(&t.surface.alpha(apagado).into()), Border::rounded(8.0).color(borda.alpha(apagado), if self.checked { 5.0 } else { 1.0 }), None);
            }
            Forma::Chave => {
                let trilho = if self.checked { ativa } else { t.text.alpha(0.2) };
                cx.renderer.fill_quad(r, Some(&trilho.alpha(trilho.a * apagado).into()), Border::rounded(m.height / 2.0), None);
                let d = m.height - 4.0;
                let x = if self.checked { r.x + m.width - d - 2.0 } else { r.x + 2.0 };
                let bola = Rect { x, y: r.y + 2.0, width: d, height: d };
                cx.renderer.fill_quad(bola, Some(&Color::WHITE.into()), Border::rounded(d / 2.0), Some(Shadow { color: Color::BLACK.alpha(0.25), offset: Point::new(0.0, 1.0), blur: 2.0 }));
            }
        }
        if !self.label.is_empty() {
            let th = cx.renderer.measure(&self.label, self.size, Font::DEFAULT, None).height;
            let cor = if st == Status::Disabled { t.muted } else { cx.text_color };
            cx.renderer.draw_text(&self.label, self.size, Font::DEFAULT, cor, Point::new(r.x + m.width + 8.0, n.rect.y + (n.rect.height - th) / 2.0), None);
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        match (ev, &self.on_toggle) {
            (Event::MousePressed(MouseButton::Left), Some(f)) if cx.cursor.is_some_and(|p| n.rect.contains(p)) => {
                cx.publish(f(!self.checked));
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
}

// ---------- Dropdown ----------

#[derive(Default)]
struct Aberto(bool);

/// Lista suspensa: mostra a opção escolhida e abre as outras embaixo.
pub struct PickList<'a, T, M> {
    options: &'a [T],
    selected: Option<T>,
    on_select: Box<dyn Fn(T) -> M + 'a>,
    placeholder: Cow<'a, str>,
    width: Length,
    size: f32,
}

pub fn pick_list<'a, T: Clone + PartialEq + ToString, M>(options: &'a [T], selected: Option<T>, on_select: impl Fn(T) -> M + 'a) -> PickList<'a, T, M> {
    PickList { options, selected, on_select: Box::new(on_select), placeholder: "".into(), width: Length::Shrink, size: 14.0 }
}

const ITEM: f32 = 30.0;

impl<'a, T: Clone + PartialEq + ToString, M> PickList<'a, T, M> {
    pub fn placeholder(mut self, p: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = p.into();
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn size(mut self, s: impl Into<Num>) -> Self {
        self.size = s.into().0;
        self
    }
    fn menu(&self, campo: Rect, janela: f32) -> Rect {
        let altura = self.options.len() as f32 * ITEM + 8.0;
        Rect { x: campo.x, y: crate::pickers::painel_y(campo, altura, janela), width: campo.width, height: altura }
    }
}

impl<'a, T: Clone + PartialEq + ToString + 'a, M> Widget<M> for PickList<'a, T, M>
{
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let mut w = cx.renderer.measure(&self.placeholder, self.size, Font::DEFAULT, None).width;
        for o in self.options.iter() {
            w = w.max(cx.renderer.measure(&o.to_string(), self.size, Font::DEFAULT, None).width);
        }
        let h = cx.renderer.measure("Ag", self.size, Font::DEFAULT, None).height + 14.0;
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, w + 20.0 + 24.0), h))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let t = *cx.theme;
        let aberto = cx.states.get::<Aberto>(n.key).is_some_and(|a| a.0);
        let hover = cx.hovered(&n.rect);
        if hover {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        let borda = if aberto { t.primary } else if hover { t.muted } else { t.border };
        cx.renderer.fill_quad(n.rect, Some(&t.surface.into()), Border::rounded(6.0).color(borda, 1.0), None);
        let (rotulo, cor) = match &self.selected {
            Some(s) => (s.to_string(), cx.text_color),
            None => (self.placeholder.to_string(), t.muted),
        };
        let th = cx.renderer.measure(&rotulo, self.size, Font::DEFAULT, None).height;
        let y = n.rect.y + (n.rect.height - th) / 2.0;
        cx.renderer.push_clip(Rect { width: n.rect.width - 28.0, ..n.rect }, [0.0; 4]);
        cx.renderer.draw_text(&rotulo, self.size, Font::DEFAULT, cor, Point::new(n.rect.x + 10.0, y), None);
        cx.renderer.pop_clip();
        let (ax, ay) = (n.rect.x + n.rect.width - 18.0, n.rect.y + n.rect.height / 2.0);
        let seta = if aberto { [Point::new(ax - 4.0, ay + 2.0), Point::new(ax, ay - 2.0), Point::new(ax + 4.0, ay + 2.0)] } else { [Point::new(ax - 4.0, ay - 2.0), Point::new(ax, ay + 2.0), Point::new(ax + 4.0, ay - 2.0)] };
        cx.renderer.stroke(&seta, 1.5, t.muted);
        if !aberto {
            return;
        }
        let menu = self.menu(n.rect, cx.renderer.window_size().height);
        let itens: Vec<(String, bool)> = self.options.iter().map(|o| (o.to_string(), self.selected.as_ref() == Some(o))).collect();
        let (cursor, size) = (cx.cursor, self.size);
        cx.overlay(move |rd| {
            rd.fill_quad(menu, Some(&t.surface.into()), Border::rounded(8.0).color(t.border, 1.0), Some(Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 6.0), blur: 14.0 }));
            for (i, (s, escolhido)) in itens.iter().enumerate() {
                let r = Rect { x: menu.x + 4.0, y: menu.y + 4.0 + i as f32 * ITEM, width: menu.width - 8.0, height: ITEM };
                if *escolhido || cursor.is_some_and(|p| r.contains(p)) {
                    let bg = if *escolhido { t.primary.alpha(0.22) } else { t.text.alpha(0.08) };
                    rd.fill_quad(r, Some(&bg.into()), Border::rounded(5.0), None);
                }
                let th = rd.measure(s, size, Font::DEFAULT, None).height;
                rd.draw_text(s, size, Font::DEFAULT, t.text, Point::new(r.x + 8.0, r.y + (ITEM - th) / 2.0), None);
            }
        });
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let aberto = cx.states.get::<Aberto>(n.key).is_some_and(|a| a.0);
        let p = cx.cursor;
        match ev {
            // shortcut: o menu recebe o clique antes dos irmãos seguintes, mas um irmão anterior que fique embaixo dele ganha; um canal de "popup" na raiz resolve se aparecer.
            Event::MousePressed(MouseButton::Left) if aberto => {
                cx.states.get_mut::<Aberto>(n.key).0 = false;
                cx.redraw = true;
                let menu = self.menu(n.rect, cx.renderer.window_size().height);
                if let Some(p) = p.filter(|p| menu.contains(*p)) {
                    let i = ((p.y - menu.y - 4.0) / ITEM).floor().max(0.0) as usize;
                    if let Some(o) = self.options.get(i) {
                        cx.publish((self.on_select)(o.clone()));
                    }
                    return Captured::Yes;
                }
                if p.is_some_and(|p| n.rect.contains(p)) {
                    Captured::Yes
                } else {
                    Captured::No
                }
            }
            Event::MousePressed(MouseButton::Left) if p.is_some_and(|p| n.rect.contains(p)) => {
                cx.states.get_mut::<Aberto>(n.key).0 = true;
                cx.redraw = true;
                Captured::Yes
            }
            Event::Key { key: Key::Named(NamedKey::Escape), pressed: true, .. } if aberto => {
                cx.states.get_mut::<Aberto>(n.key).0 = false;
                cx.redraw = true;
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
    fn opaque_at(&self, _: &Node, _: Point) -> bool {
        false
    }
}

// ---------- Slider ----------

#[derive(Default)]
struct Arrastando(bool);

pub struct Slider<'a, M> {
    range: std::ops::RangeInclusive<f32>,
    value: f32,
    step: f32,
    on_change: Box<dyn Fn(f32) -> M + 'a>,
    width: Length,
    color: Option<Color>,
}

pub fn slider<'a, M>(range: std::ops::RangeInclusive<f32>, value: f32, on_change: impl Fn(f32) -> M + 'a) -> Slider<'a, M> {
    Slider { range, value, step: 0.0, on_change: Box::new(on_change), width: Length::FILL, color: None }
}

impl<M> Slider<'_, M> {
    /// Arredonda o valor para múltiplos de `step` (0 = contínuo).
    pub fn step(mut self, s: f32) -> Self {
        self.step = s;
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    /// A cor da parte preenchida e da borda da bolinha.
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
    fn valor_em(&self, n: &Node, x: f32) -> f32 {
        let (a, b) = (*self.range.start(), *self.range.end());
        let t = ((x - n.rect.x - 8.0) / (n.rect.width - 16.0).max(1.0)).clamp(0.0, 1.0);
        let v = a + (b - a) * t;
        if self.step > 0.0 {
            (a + ((v - a) / self.step).round() * self.step).clamp(a, b)
        } else {
            v
        }
    }
}

impl<M> Widget<M> for Slider<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Fixed(20.0))
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 160.0), 20.0))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let t = *cx.theme;
        let (a, b) = (*self.range.start(), *self.range.end());
        let frac = if b > a { ((self.value - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
        let trilho = Rect { x: n.rect.x + 8.0, y: n.rect.y + 8.0, width: n.rect.width - 16.0, height: 4.0 };
        cx.renderer.fill_quad(trilho, Some(&t.text.alpha(0.15).into()), Border::rounded(2.0), None);
        let cor = self.color.unwrap_or(t.primary);
        cx.renderer.fill_quad(Rect { width: trilho.width * frac, ..trilho }, Some(&cor.into()), Border::rounded(2.0), None);
        let ativo = cx.hovered(&n.rect) || cx.states.get::<Arrastando>(n.key).is_some_and(|d| d.0);
        if ativo {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        let d = if ativo { 16.0 } else { 14.0 };
        let bola = Rect { x: trilho.x + trilho.width * frac - d / 2.0, y: n.rect.y + 10.0 - d / 2.0, width: d, height: d };
        cx.renderer.fill_quad(bola, Some(&Color::WHITE.into()), Border::rounded(d / 2.0).color(cor, 2.0), Some(Shadow { color: Color::BLACK.alpha(0.2), offset: Point::new(0.0, 1.0), blur: 2.0 }));
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let arrastando = cx.states.get::<Arrastando>(n.key).is_some_and(|d| d.0);
        match ev {
            Event::MousePressed(MouseButton::Left) if cx.cursor.is_some_and(|p| n.rect.contains(p)) => {
                cx.states.get_mut::<Arrastando>(n.key).0 = true;
                let v = self.valor_em(n, cx.cursor.map_or(0.0, |p| p.x));
                cx.publish((self.on_change)(v));
                Captured::Yes
            }
            Event::CursorMoved(p) if arrastando => {
                let v = self.valor_em(n, p.x);
                if v != self.value {
                    cx.publish((self.on_change)(v));
                }
                Captured::Yes
            }
            Event::MouseReleased(MouseButton::Left) if arrastando => {
                cx.states.get_mut::<Arrastando>(n.key).0 = false;
                cx.redraw = true;
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
}

// ---------- Régua ----------

/// Linha divisória fina na cor da borda do tema.
pub struct Rule {
    horizontal: bool,
    espessura: f32,
    color: Option<Color>,
}

pub fn horizontal_rule(espessura: f32) -> Rule {
    Rule { horizontal: true, espessura, color: None }
}

pub fn vertical_rule(espessura: f32) -> Rule {
    Rule { horizontal: false, espessura, color: None }
}

impl Rule {
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
}

impl<M> Widget<M> for Rule {
    fn size(&self) -> (Length, Length) {
        if self.horizontal {
            (Length::FILL, Length::Fixed(self.espessura))
        } else {
            (Length::Fixed(self.espessura), Length::FILL)
        }
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let (w, h) = Widget::<M>::size(self);
        Node::leaf(cx.key(), Size::new(Limits::resolve(w, l.max.width, self.espessura), Limits::resolve(h, l.max.height, self.espessura)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let c = self.color.unwrap_or(cx.theme.border);
        cx.renderer.fill_quad(n.rect, Some(&c.into()), Border::default(), None);
    }
}

impl<'a, M: 'a> From<Toggle<'a, M>> for Element<'a, M> {
    fn from(w: Toggle<'a, M>) -> Self {
        Element::new(w)
    }
}
impl<'a, T: Clone + PartialEq + ToString + 'a, M: 'a> From<PickList<'a, T, M>> for Element<'a, M>
{
    fn from(w: PickList<'a, T, M>) -> Self {
        Element::new(w)
    }
}
impl<'a, M: 'a> From<Slider<'a, M>> for Element<'a, M> {
    fn from(w: Slider<'a, M>) -> Self {
        Element::new(w)
    }
}
impl<'a, M: 'a> From<Rule> for Element<'a, M> {
    fn from(w: Rule) -> Self {
        Element::new(w)
    }
}

// ---------- Abas e número (montados com os widgets prontos) ----------

/// Abas com sublinhado na ativa.
pub fn tabs<'a, M: Clone + 'a>(labels: &[&'a str], active: usize, on_select: impl Fn(usize) -> M + 'a) -> Element<'a, M> {
    let abas = labels.iter().enumerate().map(|(i, l)| {
        let ativa = i == active;
        let rotulo = crate::text(*l).size(14).font(if ativa { Font::SEMIBOLD } else { Font::DEFAULT });
        crate::button(rotulo).on_press(on_select(i)).padding([8u16, 14]).style(move |t, s| {
            let cor = if ativa { t.text } else if s == Status::Hovered { t.text.alpha(0.85) } else { t.muted };
            let fundo = if s == Status::Hovered && !ativa { t.text.alpha(0.05) } else { Color::TRANSPARENT };
            // O sublinhado é o fim do próprio fundo: um filho "encher" ao lado esticaria a aba.
            let barra = if ativa { t.primary } else { fundo };
            let bg = Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, fundo), (0.94, fundo), (0.95, barra), (1.0, barra)] };
            crate::Style::default().text_color(cor).background(bg).border(Border { radius: [6.0, 6.0, 0.0, 0.0], ..Border::default() })
        })
        .into()
    });
    let linha = crate::row(abas);
    crate::column([linha.into(), horizontal_rule(1.0).into()]).into()
}

/// Número com botões − e +, preso em `range`, andando de `step`.
pub fn number_input<'a, M: Clone + 'a>(value: f64, range: std::ops::RangeInclusive<f64>, step: f64, on_change: impl Fn(f64) -> M + 'a) -> Element<'a, M> {
    let (min, max) = (*range.start(), *range.end());
    let menos = (value > min).then(|| on_change((value - step).max(min)));
    let mais = (value < max).then(|| on_change((value + step).min(max)));
    let casas = if step.fract() == 0.0 { 0 } else { 2 };
    let botao = |s: &'static str, m: Option<M>| crate::button(crate::text(s).size(15)).on_press_maybe(m).width(32.0).height(32.0).padding(0.0).style(crate::button_text);
    let valor = crate::container(crate::text(format!("{value:.casas$}")).size(14)).width(56.0).height(32.0).align_x(crate::Align::Center).align_y(crate::Align::Center);
    crate::container(crate::row([botao("−", menos).into(), valor.into(), botao("+", mais).into()]).align(crate::Align::Center))
        .style(|t| crate::Style::default().background(t.surface).border(Border::rounded(6.0).color(t.border, 1.0)))
        .into()
}
