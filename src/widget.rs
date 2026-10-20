//! O trait [`Widget`], a árvore de layout e os widgets prontos.
use crate::access::{accesskit, Access};
use crate::anim::{animar, Lerp, Transition};
use crate::{Align, Background, Border, Captured, Color, Event, Font, Image, Length, MouseButton, Padding, Point, Rect, Renderer, Shadow, Size, States, Status, Svg, Theme, WindowCmd};
use crate::{Key, Modifiers, NamedKey};
use std::borrow::Cow;

pub use winit::window::CursorIcon;

/// O resultado do layout: onde cada widget ficou, em pixels lógicos absolutos.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub rect: Rect,
    /// O caminho do widget na árvore: chave do estado dele em [`States`].
    pub key: u64,
    pub children: Vec<Node>,
    /// Espaço interno e externo do widget: só o inspetor (modo debug) lê.
    pub padding: Padding,
    pub margin: Padding,
}

impl Node {
    pub fn leaf(key: u64, size: Size) -> Self {
        Node { rect: Rect::new(Point::ORIGIN, size), key, ..Default::default() }
    }
    pub fn size(&self) -> Size {
        self.rect.size()
    }
    pub(crate) fn translate(&mut self, dx: f32, dy: f32) {
        self.rect.x += dx;
        self.rect.y += dy;
        for c in &mut self.children {
            c.translate(dx, dy);
        }
    }
    /// Move o nó (e os filhos) para que o canto fique em `p`, relativo ao pai durante o layout.
    pub fn place(&mut self, p: Point) {
        let (dx, dy) = (p.x - self.rect.x, p.y - self.rect.y);
        self.translate(dx, dy);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max: Size,
}

impl Limits {
    pub fn new(max: Size) -> Self {
        Limits { max }
    }
    /// O tamanho final de um lado: `Fill` sem limite vira o tamanho do conteúdo.
    pub fn resolve(len: Length, max: f32, conteudo: f32) -> f32 {
        match len {
            Length::Fixed(v) => v.min(max),
            Length::Fill(_) if max.is_finite() => max,
            _ => conteudo.min(max),
        }
    }
    /// O tamanho de quem enche tudo: num lado sem limite (dentro de uma rolagem), `padrao`.
    pub fn fill(&self, padrao: Size) -> Size {
        Size::new(Limits::resolve(Length::FILL, self.max.width, padrao.width), Limits::resolve(Length::FILL, self.max.height, padrao.height))
    }
}

pub struct LayoutCx<'a> {
    pub renderer: &'a mut Renderer,
    pub states: &'a States,
    pub theme: &'a Theme,
    key: u64,
}

impl LayoutCx<'_> {
    pub fn key(&self) -> u64 {
        self.key
    }
    /// Faz o layout do filho `i`, que recebe a própria chave de estado.
    pub fn child<M>(&mut self, i: usize, el: &Element<'_, M>, limits: Limits) -> Node {
        let pai = self.key;
        self.key = child_key(pai, i);
        let n = el.0.layout(self, limits);
        self.key = pai;
        n
    }
}

fn child_key(pai: u64, i: usize) -> u64 {
    (pai ^ (i as u64 + 1)).wrapping_mul(0x0100_0000_01b3).rotate_left(7)
}

pub struct DrawCx<'a> {
    pub renderer: &'a mut Renderer,
    pub states: &'a States,
    pub theme: &'a Theme,
    /// `None` quando o mouse está fora ou coberto por uma camada opaca acima.
    pub cursor: Option<Point>,
    /// A cor herdada pelos textos sem cor própria.
    pub text_color: Color,
    /// O ícone do mouse no próximo quadro; os widgets sob o cursor o trocam.
    pub cursor_icon: CursorIcon,
    overlays: Vec<Box<dyn FnOnce(&mut Renderer) + 'a>>,
    /// Montando a árvore de acessibilidade (há um leitor de tela ligado).
    acesso: Option<&'a mut Access>,
}

impl<'a> DrawCx<'a> {
    /// O cursor está em `r` (e não coberto)?
    pub fn hovered(&self, r: &Rect) -> bool {
        self.cursor.is_some_and(|p| r.contains(p))
    }
    /// Desenha depois de tudo, por cima de qualquer camada (tooltips, menus soltos).
    pub fn overlay(&mut self, f: impl FnOnce(&mut Renderer) + 'a) {
        self.overlays.push(Box::new(f));
    }
    /// `alvo` com transição (o `transition` do CSS): quando ele muda, o valor devolvido anda
    /// até lá em `t`, e a janela desenha os quadros do caminho. `key` é a do widget (`node.key`);
    /// cada tipo de valor dele anda separado.
    pub fn transition<T: Lerp>(&self, key: u64, alvo: T, t: Transition) -> T {
        animar(self.states, key, &alvo, None, t, None)
    }
    /// O estilo com a transição dele ([`Style::transition`]), se tiver.
    pub fn styled(&self, key: u64, st: Style) -> Style {
        match st.transition {
            Some(t) => self.transition(key, st, t),
            None => st,
        }
    }
    /// Um menu ou painel solto que este widget abriu em `r` (desenhado por [`overlay`](Self::overlay)):
    /// com o mouse em cima dele, o resto da árvore não vê o mouse, nem para hover nem para clique.
    pub fn popup(&self, r: Rect) {
        self.states.popups.borrow_mut().push(r);
    }
    /// O cursor para o dono de um popup: o de sempre, ou o que está em cima de um popup.
    pub fn popup_cursor(&self) -> Option<Point> {
        self.cursor.or(self.states.sob_popup.get())
    }
    /// Se há um leitor de tela ouvindo: só aí vale montar nós de acessibilidade.
    pub fn accessible(&self) -> bool {
        self.acesso.is_some()
    }
    /// Registra o widget para os leitores de tela; `node` só roda se houver um ligado.
    pub fn access(&mut self, key: u64, r: Rect, node: impl FnOnce() -> accesskit::Node) {
        if let Some(a) = self.acesso.as_deref_mut() {
            a.push(key, r, node(), false);
        }
    }
    /// Igual a [`access`](Self::access), mas o que for registrado até o [`access_end`](Self::access_end)
    /// fica dentro dele. Num botão, o texto de dentro vira o nome.
    pub fn access_begin(&mut self, key: u64, r: Rect, node: impl FnOnce() -> accesskit::Node) {
        if let Some(a) = self.acesso.as_deref_mut() {
            a.push(key, r, node(), true);
        }
    }
    pub fn access_end(&mut self) {
        if let Some(a) = self.acesso.as_deref_mut() {
            a.end();
        }
    }
    /// Um texto à mostra (o leitor lê os textos soltos e dá nome aos botões com eles).
    pub fn access_text(&mut self, key: u64, r: Rect, s: &str) {
        if let Some(a) = self.acesso.as_deref_mut() {
            a.text(key, r, s);
        }
    }
}

/// Como menus, calendários e dicas abrem: crescem um pouco e aparecem.
pub(crate) const ABRIR: Transition = Transition { duration: std::time::Duration::from_millis(150), easing: crate::Easing::EaseOut, delay: std::time::Duration::ZERO };

/// Desenha um painel solto (menu, calendário, dica) no ponto `p` (0 a 1) da abertura dele.
pub(crate) fn surgir(r: &mut Renderer, painel: Rect, p: f32, f: impl FnOnce(&mut Renderer)) {
    if p >= 1.0 {
        return f(r);
    }
    let area = Rect { x: painel.x - 24.0, y: painel.y - 24.0, width: painel.width + 48.0, height: painel.height + 48.0 };
    r.push_layer(area, 0.95 + 0.05 * p, 0.0, p);
    f(r);
    r.pop_layer();
}

/// Um nó do AccessKit com o papel e as ações dadas.
pub(crate) fn no(role: accesskit::Role, acoes: &[accesskit::Action]) -> accesskit::Node {
    let mut n = accesskit::Node::new(role);
    for a in acoes {
        n.add_action(*a);
    }
    n
}

pub struct EventCx<'a, M> {
    pub renderer: &'a mut Renderer,
    pub states: &'a mut States,
    pub cursor: Option<Point>,
    pub messages: &'a mut Vec<M>,
    pub window: &'a mut Vec<WindowCmd>,
    /// Algum estado visual mudou sem mensagem (rolagem, cursor do campo): redesenhar.
    pub redraw: bool,
    pub clipboard: &'a mut dyn Clipboard,
}

/// A área de transferência (Ctrl+C / Ctrl+V nos campos).
pub trait Clipboard {
    fn read(&mut self) -> Option<String>;
    fn write(&mut self, s: String);
}

/// Sem sistema por trás (testes, antes da janela abrir): guarda na memória.
#[derive(Default)]
pub struct MemoryClipboard(pub Option<String>);

impl Clipboard for MemoryClipboard {
    fn read(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn write(&mut self, s: String) {
        self.0 = Some(s);
    }
}

impl<M> EventCx<'_, M> {
    pub fn publish(&mut self, m: M) {
        self.messages.push(m);
    }
    /// O cursor para o dono de um popup (veja [`DrawCx::popup`]).
    pub fn popup_cursor(&self) -> Option<Point> {
        self.cursor.or(self.states.sob_popup.get())
    }
}

pub trait Widget<M> {
    fn size(&self) -> (Length, Length) {
        (Length::Shrink, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, limits: Limits) -> Node;
    fn draw(&self, node: &Node, cx: &mut DrawCx);
    fn event(&self, _node: &Node, _ev: &Event, _cx: &mut EventCx<M>) -> Captured {
        Captured::No
    }
    /// Se este widget cobre o ponto `p` para quem está abaixo dele numa pilha.
    fn opaque_at(&self, _node: &Node, _p: Point) -> bool {
        false
    }
}

pub struct Element<'a, M>(Box<dyn Widget<M> + 'a>);

impl<'a, M: 'a> Element<'a, M> {
    pub fn new(w: impl Widget<M> + 'a) -> Self {
        Element(Box::new(w))
    }
    /// Converte as mensagens: um componente com mensagens próprias dentro de outro app.
    pub fn map<N: 'a>(self, f: impl Fn(M) -> N + 'a) -> Element<'a, N> {
        Element::new(Map { inner: self, f: Box::new(f) })
    }
    pub fn widget(&self) -> &dyn Widget<M> {
        self.0.as_ref()
    }
    /// O nome que o leitor de tela fala (o `aria-label` do HTML): botão só de ícone, imagem.
    pub fn access_label(self, label: impl Into<Cow<'a, str>>) -> Self {
        Element::new(AccessLabel { content: self, label: label.into() })
    }
}

struct AccessLabel<'a, M> {
    content: Element<'a, M>,
    label: Cow<'a, str>,
}

impl<M> Widget<M> for AccessLabel<'_, M> {
    fn size(&self) -> (Length, Length) {
        self.content.0.size()
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let n = cx.child(0, &self.content, l);
        Node { rect: n.rect, key: cx.key(), children: vec![n], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        if let Some(a) = cx.acesso.as_deref_mut() {
            a.rotulo = Some(self.label.to_string());
        }
        self.content.0.draw(&n.children[0], cx);
        // Ninguém de dentro se registrou (um ícone, uma imagem): o rótulo vira um nó próprio.
        if let Some(a) = cx.acesso.as_deref_mut() {
            if let Some(l) = a.rotulo.take() {
                let mut no = accesskit::Node::new(accesskit::Role::Image);
                no.set_label(l);
                a.push(n.key, n.rect, no, false);
            }
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        self.content.0.event(&n.children[0], ev, cx)
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.content.0.opaque_at(&n.children[0], p)
    }
}

struct Map<'a, M, N> {
    inner: Element<'a, M>,
    f: Box<dyn Fn(M) -> N + 'a>,
}

impl<M, N> Widget<N> for Map<'_, M, N> {
    fn size(&self) -> (Length, Length) {
        self.inner.0.size()
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        self.inner.0.layout(cx, l)
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        self.inner.0.draw(n, cx)
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.inner.0.opaque_at(n, p)
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<N>) -> Captured {
        let mut msgs = Vec::new();
        let mut sub = EventCx { renderer: cx.renderer, states: cx.states, cursor: cx.cursor, messages: &mut msgs, window: cx.window, redraw: false, clipboard: &mut *cx.clipboard };
        let c = self.inner.0.event(n, ev, &mut sub);
        cx.redraw |= sub.redraw;
        cx.messages.extend(msgs.into_iter().map(&self.f));
        c
    }
}

/// Cada widget vira `Element` com `.into()`; um `From` genérico colidiria com `From<T> for T`.
macro_rules! into_element {
    ($($t:ty $(where $b:path)?),* $(,)?) => {$(
        impl<'a, M: 'a $(+ $b)?> From<$t> for Element<'a, M> {
            fn from(w: $t) -> Self {
                Element::new(w)
            }
        }
    )*};
}

into_element!(
    Text<'a>, Space, Container<'a, M>, Flex<'a, M>, Stack<'a, M>, Opaque<'a, M>, Button<'a, M> where Clone,
    MouseArea<'a, M> where Clone, Scrollable<'a, M>, TextInput<'a, M> where Clone, ImageView, SvgView, ProgressBar,
    Tooltip<'a, M>, Canvas<'a>,
);

impl<'a, M: 'a> From<&'a str> for Element<'a, M> {
    fn from(s: &'a str) -> Self {
        Element::new(text(s))
    }
}

impl<'a, M: 'a> From<String> for Element<'a, M> {
    fn from(s: String) -> Self {
        Element::new(text(s))
    }
}

// ---------- A raiz: layout, desenho e eventos de um quadro ----------

/// Layout da árvore inteira em `size` lógico.
pub fn layout<M>(el: &Element<'_, M>, size: Size, renderer: &mut Renderer, states: &States, theme: &Theme) -> Node {
    let mut cx = LayoutCx { renderer, states, theme, key: 0 };
    let mut n = el.0.layout(&mut cx, Limits::new(size));
    n.place(Point::ORIGIN);
    n
}

/// Desenha um quadro já com layout; devolve o ícone do mouse pedido.
/// Com `debug`, desenha por cima o inspetor (veja [`inspect`]).
pub fn draw<M>(el: &Element<'_, M>, node: &Node, renderer: &mut Renderer, states: &States, theme: &Theme, cursor: Option<Point>, debug: bool) -> CursorIcon {
    draw_access(el, node, renderer, states, theme, cursor, debug, None)
}

/// O [`draw`] que também monta a árvore de acessibilidade em `acesso`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_access<M>(el: &Element<'_, M>, node: &Node, renderer: &mut Renderer, states: &States, theme: &Theme, cursor: Option<Point>, debug: bool, acesso: Option<&mut Access>) -> CursorIcon {
    // Os popups do quadro passado decidem quem vê o mouse; os deste quadro se registram de novo.
    let popups = std::mem::take(&mut *states.popups.borrow_mut());
    let sob = cursor.filter(|p| popups.iter().any(|r| r.contains(*p)));
    states.sob_popup.set(sob);
    let inspecionar = cursor;
    let cursor = if sob.is_some() { None } else { cursor };
    let mut cx = DrawCx { renderer, states, theme, cursor, text_color: theme.text, cursor_icon: CursorIcon::Default, overlays: Vec::new(), acesso };
    el.0.draw(node, &mut cx);
    for o in std::mem::take(&mut cx.overlays) {
        o(cx.renderer);
    }
    if debug {
        inspect(node, cx.renderer, inspecionar);
    }
    cx.cursor_icon
}

/// O inspetor, como o do navegador: contorna todo widget e, no que está sob o mouse,
/// pinta a margem (laranja), o padding (verde) e o conteúdo (azul), com o tamanho.
pub fn inspect(node: &Node, r: &mut Renderer, cursor: Option<Point>) {
    fn contornar(n: &Node, r: &mut Renderer) {
        r.fill_quad(n.rect, None, Border::default().color(Color::rgb8(0xe1, 0x1d, 0x48).alpha(0.35), 1.0), None);
        n.children.iter().for_each(|c| contornar(c, r));
    }
    contornar(node, r);
    let Some(p) = cursor else { return };
    // O mais fundo sob o mouse; empatando, o último (o de cima na pilha).
    fn fundo(n: &Node, p: Point, d: usize) -> (usize, &Node) {
        n.children.iter().filter(|c| c.rect.contains(p)).map(|c| fundo(c, p, d + 1)).fold((d, n), |a, b| if b.0 >= a.0 { b } else { a })
    }
    let alvo = fundo(node, p, 0).1;
    let borda = alvo.rect.shrink(alvo.margin);
    let conteudo = borda.shrink(alvo.padding);
    let faixa = |r: &mut Renderer, fora: Rect, dentro: Rect, cor: Color| {
        let mut pb = [
            Rect { height: dentro.y - fora.y, ..fora },
            Rect { y: dentro.y + dentro.height, height: fora.y + fora.height - dentro.y - dentro.height, ..fora },
            Rect { x: fora.x, y: dentro.y, width: dentro.x - fora.x, height: dentro.height },
            Rect { x: dentro.x + dentro.width, y: dentro.y, width: fora.x + fora.width - dentro.x - dentro.width, height: dentro.height },
        ];
        for q in pb.iter_mut().filter(|q| q.width > 0.0 && q.height > 0.0) {
            r.fill_quad(*q, Some(&cor.into()), Border::default(), None);
        }
    };
    faixa(r, alvo.rect, borda, Color::rgb8(0xf9, 0xa8, 0x25).alpha(0.45));
    faixa(r, borda, conteudo, Color::rgb8(0x84, 0xcc, 0x16).alpha(0.45));
    r.fill_quad(conteudo, Some(&Color::rgb8(0x38, 0xbd, 0xf8).alpha(0.3).into()), Border::default(), None);
    let n = |v: f32| format!("{}", (v * 10.0).round() / 10.0);
    let lado = |q: Padding| format!("{} {} {} {}", n(q.top), n(q.right), n(q.bottom), n(q.left));
    let mut rotulo = format!("{} × {}  em ({}, {})", n(borda.width), n(borda.height), n(borda.x), n(borda.y));
    if alvo.padding != Padding::ZERO {
        rotulo += &format!("\npadding {}", lado(alvo.padding));
    }
    if alvo.margin != Padding::ZERO {
        rotulo += &format!("\nmargin {}", lado(alvo.margin));
    }
    let t = r.measure(&rotulo, 11.0, Font::MONO, None);
    let janela_h = r.window_size().height;
    let y = if alvo.rect.y + alvo.rect.height + t.height + 12.0 < janela_h { alvo.rect.y + alvo.rect.height + 4.0 } else { (alvo.rect.y - t.height - 12.0).max(0.0) };
    let caixa = Rect { x: alvo.rect.x.max(0.0), y, width: t.width + 12.0, height: t.height + 8.0 };
    r.fill_quad(caixa, Some(&Color::rgb8(0x11, 0x18, 0x27).alpha(0.92).into()), Border::rounded(4.0), None);
    r.draw_text(&rotulo, 11.0, Font::MONO, Color::WHITE, Point::new(caixa.x + 6.0, caixa.y + 4.0), None);
}

/// Desenha o quadro e devolve o que os leitores de tela recebem dele (testes; a janela manda sozinha).
pub fn access_tree<M>(el: &Element<'_, M>, node: &Node, renderer: &mut Renderer, states: &States, theme: &Theme, title: &str) -> accesskit::TreeUpdate {
    let mut a = Access::default();
    draw_access(el, node, renderer, states, theme, None, false, Some(&mut a));
    a.tree(title, renderer.scale() as f64, states.focus)
}

/// Passa um evento pela árvore; o clique fora de um campo tira o foco dele.
pub fn dispatch<M>(el: &Element<'_, M>, node: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
    if matches!(ev, Event::MousePressed(_)) && cx.states.focus.take().is_some() {
        cx.redraw = true;
    }
    let mouse = matches!(ev, Event::MousePressed(_) | Event::MouseReleased(_) | Event::Wheel { .. } | Event::CursorMoved(_));
    let sob = cx.cursor.filter(|p| mouse && cx.states.popups.borrow().iter().any(|r| r.contains(*p)));
    cx.states.sob_popup.set(sob);
    if sob.is_none() {
        return el.0.event(node, ev, cx);
    }
    let real = cx.cursor.take();
    let c = el.0.event(node, ev, cx);
    cx.cursor = real;
    cx.states.sob_popup.set(None);
    c
}

/// Desenha `el` em `size` lógico e devolve o PNG: testes, capturas e miniaturas sem janela nem GPU.
/// `inspect_at` liga o inspetor com o mouse nesse ponto.
pub fn capture<M>(el: &Element<'_, M>, size: Size, scale: f32, theme: &Theme, renderer: &mut Renderer, inspect_at: Option<Point>) -> Vec<u8> {
    let states = States::default();
    renderer.begin(size, scale, theme.background);
    let n = layout(el, size, renderer, &states, theme);
    draw(el, &n, renderer, &states, theme, inspect_at, inspect_at.is_some());
    renderer.pixmap.encode_png().expect("pixmap em memória sempre codifica")
}

// ---------- Estilo ----------

/// A caixa de qualquer widget: fundo, borda, sombra e a cor do texto de dentro.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub background: Option<Background>,
    pub border: Border,
    pub shadow: Option<Shadow>,
    pub text_color: Option<Color>,
    /// Desfoca o que já foi desenhado atrás da caixa (o `backdrop-filter: blur()` do CSS), em pixels lógicos.
    pub backdrop_blur: f32,
    /// Mudanças de estilo (hover, foco, o app trocou a cor) andam em vez de pular (o `transition` do CSS).
    pub transition: Option<Transition>,
}

impl Style {
    /// Vidro: desfoca o fundo, clareia de leve e marca a borda com um fio de luz.
    pub fn glass(radius: f32) -> Self {
        Style {
            background: Some(Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, Color::WHITE.alpha(0.14)), (1.0, Color::WHITE.alpha(0.05))] }),
            border: Border::rounded(radius).color(Color::WHITE.alpha(0.22), 1.0),
            shadow: Some(Shadow { color: Color::BLACK.alpha(0.25), offset: Point::new(0.0, 8.0), blur: 24.0 }),
            text_color: None,
            backdrop_blur: 18.0,
            transition: None,
        }
    }
    pub fn backdrop_blur(self, b: f32) -> Self {
        Style { backdrop_blur: b, ..self }
    }
    pub fn background(self, b: impl Into<Background>) -> Self {
        Style { background: Some(b.into()), ..self }
    }
    pub fn border(self, border: Border) -> Self {
        Style { border, ..self }
    }
    pub fn shadow(self, s: Shadow) -> Self {
        Style { shadow: Some(s), ..self }
    }
    pub fn text_color(self, c: Color) -> Self {
        Style { text_color: Some(c), ..self }
    }
    pub fn transition(self, t: Transition) -> Self {
        Style { transition: Some(t), ..self }
    }
    pub(crate) fn paint(&self, r: Rect, cx: &mut DrawCx) {
        if self.backdrop_blur > 0.0 {
            if let Some(sh) = self.shadow {
                cx.renderer.fill_quad(r, None, self.border, Some(sh));
            }
            cx.renderer.blur(r, self.backdrop_blur, self.border.radius);
            cx.renderer.fill_quad(r, self.background.as_ref(), self.border, None);
            return;
        }
        if self.background.is_some() || self.border.width > 0.0 || self.shadow.is_some() {
            cx.renderer.fill_quad(r, self.background.as_ref(), self.border, self.shadow);
        }
    }
}

type StyleFn<'a> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

// ---------- Texto ----------

pub struct Text<'a> {
    content: Cow<'a, str>,
    size: f32,
    font: Font,
    color: Option<Color>,
    width: Length,
    height: Length,
    align_x: Align,
    wrap: bool,
    line_height: f32,
}

pub fn text<'a>(s: impl Into<Cow<'a, str>>) -> Text<'a> {
    Text { content: s.into(), size: 14.0, font: Font::DEFAULT, color: None, width: Length::Shrink, height: Length::Shrink, align_x: Align::Start, wrap: true, line_height: 1.3 }
}

impl Text<'_> {
    pub fn size(mut self, s: impl Into<Num>) -> Self {
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
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    pub fn align_x(mut self, a: Align) -> Self {
        self.align_x = a;
        self
    }
    /// Uma linha só, sem quebrar (o que passar é cortado pelo pai).
    pub fn no_wrap(mut self) -> Self {
        self.wrap = false;
        self
    }
    /// Altura da linha em múltiplos do tamanho (o `line-height` do CSS); o padrão é 1.3.
    pub fn line_height(mut self, f: f32) -> Self {
        self.line_height = f;
        self
    }
    fn max_width(&self, w: f32) -> Option<f32> {
        (self.wrap && w.is_finite()).then_some(w)
    }
}

/// Aceita `14`, `14.0` e `14u16` onde se pede um tamanho.
pub struct Num(pub f32);
impl From<f32> for Num {
    fn from(v: f32) -> Self {
        Num(v)
    }
}
impl From<i32> for Num {
    fn from(v: i32) -> Self {
        Num(v as f32)
    }
}
impl From<u16> for Num {
    fn from(v: u16) -> Self {
        Num(v as f32)
    }
}

impl<M> Widget<M> for Text<'_> {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let largura = match self.width {
            Length::Fixed(v) => v.min(l.max.width),
            _ => l.max.width,
        };
        let m = cx.renderer.measure_lh(&self.content, self.size, self.font, self.max_width(largura), self.line_height);
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, m.width), Limits::resolve(self.height, l.max.height, m.height)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let r = n.rect;
        let mw = self.max_width(r.width);
        let w = if self.align_x == Align::Start { 0.0 } else { cx.renderer.measure_lh(&self.content, self.size, self.font, mw, self.line_height).width };
        let at = Point::new(r.x + self.align_x.offset(r.width, w), r.y);
        cx.renderer.draw_text_lh(&self.content, self.size, self.font, self.color.unwrap_or(cx.text_color), at, mw, self.line_height);
        cx.access_text(n.key, r, &self.content);
    }
}

// ---------- Espaço ----------

pub struct Space {
    width: Length,
    height: Length,
}

impl Space {
    pub fn new(width: impl Into<Length>, height: impl Into<Length>) -> Self {
        Space { width: width.into(), height: height.into() }
    }
}

pub fn horizontal_space() -> Space {
    Space { width: Length::FILL, height: Length::Fixed(0.0) }
}

pub fn vertical_space() -> Space {
    Space { width: Length::Fixed(0.0), height: Length::FILL }
}

impl<M> Widget<M> for Space {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 0.0), Limits::resolve(self.height, l.max.height, 0.0)))
    }
    fn draw(&self, _: &Node, _: &mut DrawCx) {}
}

// ---------- Contêiner ----------

pub struct Container<'a, M> {
    content: Element<'a, M>,
    padding: Padding,
    margin: Padding,
    width: Length,
    height: Length,
    max_width: f32,
    align_x: Align,
    align_y: Align,
    clip: bool,
    style: Option<Box<dyn Fn(&Theme) -> Style + 'a>>,
}

pub fn container<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    Container { content: content.into(), padding: Padding::ZERO, margin: Padding::ZERO, width: Length::Shrink, height: Length::Shrink, max_width: f32::INFINITY, align_x: Align::Start, align_y: Align::Start, clip: false, style: None }
}

impl<'a, M> Container<'a, M> {
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    /// Espaço por fora do fundo e da borda (o `margin` do CSS).
    pub fn margin(mut self, m: impl Into<Padding>) -> Self {
        self.margin = m.into();
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = w;
        self
    }
    pub fn align_x(mut self, a: Align) -> Self {
        self.align_x = a;
        self
    }
    pub fn align_y(mut self, a: Align) -> Self {
        self.align_y = a;
        self
    }
    pub fn center(self) -> Self {
        self.width(Length::FILL).height(Length::FILL).align_x(Align::Center).align_y(Align::Center)
    }
    /// Recorta o conteúdo pelo contorno, cantos arredondados inclusive (imagem de fundo, lista rolando).
    pub fn clip(mut self, c: bool) -> Self {
        self.clip = c;
        self
    }
    pub fn style(mut self, f: impl Fn(&Theme) -> Style + 'a) -> Self {
        self.style = Some(Box::new(f));
        self
    }
}

impl<M> Widget<M> for Container<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        // Padding e margem somados para o filho; a margem só sai do desenho do fundo.
        let (p, mg) = (self.padding, self.margin);
        let ext = Padding { top: p.top + mg.top, right: p.right + mg.right, bottom: p.bottom + mg.bottom, left: p.left + mg.left };
        let lim_w = l.max.width.min(self.max_width + mg.horizontal());
        let max_w = Limits::resolve(self.width, lim_w, lim_w);
        let max_h = Limits::resolve(self.height, l.max.height, l.max.height);
        let mut filho = cx.child(0, &self.content, Limits::new(Size::new((max_w - ext.horizontal()).max(0.0), (max_h - ext.vertical()).max(0.0))));
        let c = filho.size();
        let w = Limits::resolve(self.width, lim_w, c.width + ext.horizontal());
        let h = Limits::resolve(self.height, l.max.height, c.height + ext.vertical());
        let (iw, ih) = (w - ext.horizontal(), h - ext.vertical());
        filho.place(Point::new(ext.left + self.align_x.offset(iw, c.width), ext.top + self.align_y.offset(ih, c.height)));
        Node { rect: Rect::new(Point::ORIGIN, Size::new(w, h)), key: cx.key(), children: vec![filho], padding: p, margin: mg }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let caixa = n.rect.shrink(self.margin);
        let st = self.style.as_ref().map(|f| f(cx.theme)).unwrap_or_default();
        let st = cx.styled(n.key, st);
        st.paint(caixa, cx);
        let herdada = cx.text_color;
        if let Some(c) = st.text_color {
            cx.text_color = c;
        }
        if self.clip {
            cx.renderer.push_clip(caixa, st.border.radius);
        }
        self.content.0.draw(&n.children[0], cx);
        if self.clip {
            cx.renderer.pop_clip();
        }
        cx.text_color = herdada;
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        self.content.0.event(&n.children[0], ev, cx)
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.content.0.opaque_at(&n.children[0], p)
    }
}

// ---------- Linha e coluna ----------

#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Row,
    Column,
}

pub struct Flex<'a, M> {
    axis: Axis,
    children: Vec<Element<'a, M>>,
    spacing: f32,
    padding: Padding,
    width: Length,
    height: Length,
    align: Align,
    wrap: bool,
}

pub type Row<'a, M> = Flex<'a, M>;
pub type Column<'a, M> = Flex<'a, M>;

pub fn row<'a, M>(children: impl IntoIterator<Item = Element<'a, M>>) -> Flex<'a, M> {
    Flex { axis: Axis::Row, children: children.into_iter().collect(), spacing: 0.0, padding: Padding::ZERO, width: Length::Shrink, height: Length::Shrink, align: Align::Start, wrap: false }
}

pub fn column<'a, M>(children: impl IntoIterator<Item = Element<'a, M>>) -> Flex<'a, M> {
    Flex { axis: Axis::Column, ..row(children) }
}

#[macro_export]
macro_rules! row {
    ($($e:expr),* $(,)?) => { $crate::row([$($crate::Element::from($e)),*]) };
}

#[macro_export]
macro_rules! column {
    ($($e:expr),* $(,)?) => { $crate::column([$($crate::Element::from($e)),*]) };
}

#[macro_export]
macro_rules! stack {
    ($($e:expr),* $(,)?) => { $crate::stack([$($crate::Element::from($e)),*]) };
}

impl<'a, M: 'a> Flex<'a, M> {
    pub fn push(mut self, e: impl Into<Element<'a, M>>) -> Self {
        self.children.push(e.into());
        self
    }
    pub fn push_maybe(self, e: Option<impl Into<Element<'a, M>>>) -> Self {
        match e {
            Some(e) => self.push(e),
            None => self,
        }
    }
    pub fn spacing(mut self, s: impl Into<Num>) -> Self {
        self.spacing = s.into().0;
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    /// Alinhamento no eixo cruzado (vertical numa linha, horizontal numa coluna).
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    /// Quebra em várias linhas quando não cabe (grade de cartões). Só em `row`.
    pub fn wrap(mut self) -> Self {
        self.wrap = true;
        self
    }
}

impl<M> Flex<'_, M> {
    // (principal, cruzado) de um tamanho, conforme o eixo.
    fn split(&self, s: Size) -> (f32, f32) {
        match self.axis {
            Axis::Row => (s.width, s.height),
            Axis::Column => (s.height, s.width),
        }
    }
    fn join(&self, main: f32, cross: f32) -> Size {
        match self.axis {
            Axis::Row => Size::new(main, cross),
            Axis::Column => Size::new(cross, main),
        }
    }
    fn main_len(&self, (w, h): (Length, Length)) -> Length {
        if self.axis == Axis::Row {
            w
        } else {
            h
        }
    }

    fn layout_wrap(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let p = self.padding;
        let max_w = Limits::resolve(self.width, l.max.width, l.max.width) - p.horizontal();
        let (mut x, mut y, mut linha_h, mut largura) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut nos = Vec::with_capacity(self.children.len());
        for (i, c) in self.children.iter().enumerate() {
            let mut n = cx.child(i, c, Limits::new(Size::new(max_w, f32::INFINITY)));
            let s = n.size();
            if x > 0.0 && x + s.width > max_w {
                x = 0.0;
                y += linha_h + self.spacing;
                linha_h = 0.0;
            }
            n.place(Point::new(p.left + x, p.top + y));
            x += s.width + self.spacing;
            largura = largura.max(x - self.spacing);
            linha_h = linha_h.max(s.height);
            nos.push(n);
        }
        let w = Limits::resolve(self.width, l.max.width, largura + p.horizontal());
        let h = Limits::resolve(self.height, l.max.height, y + linha_h + p.vertical());
        Node { rect: Rect::new(Point::ORIGIN, Size::new(w, h)), key: cx.key(), children: nos, padding: p, ..Default::default() }
    }
}

impl<M> Widget<M> for Flex<'_, M> {
    fn size(&self) -> (Length, Length) {
        // Como no Iced: sem tamanho próprio, a linha/coluna enche se algum filho enche.
        let herda = |own: Length, f: fn((Length, Length)) -> Length| match own {
            Length::Shrink => self.children.iter().map(|c| f(c.0.size())).find(|l| l.fill() > 0).unwrap_or(own),
            _ => own,
        };
        (herda(self.width, |s| s.0), herda(self.height, |s| s.1))
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        if self.wrap && self.axis == Axis::Row {
            return self.layout_wrap(cx, l);
        }
        let p = self.padding;
        let pad_main = if self.axis == Axis::Row { p.horizontal() } else { p.vertical() };
        let pad_cross = if self.axis == Axis::Row { p.vertical() } else { p.horizontal() };
        let (max_main, max_cross) = self.split(l.max);
        // A altura (ou largura) fixa da própria linha/coluna é o espaço que os filhos dividem.
        let max_main = Limits::resolve(self.main_len((self.width, self.height)), max_main, max_main);
        let max_cross_in = (Limits::resolve(if self.axis == Axis::Row { self.height } else { self.width }, max_cross, max_cross) - pad_cross).max(0.0);
        let gaps = self.spacing * self.children.len().saturating_sub(1) as f32;
        let mut sobra = (max_main - pad_main - gaps).max(0.0);
        let mut nos: Vec<Option<Node>> = (0..self.children.len()).map(|_| None).collect();
        // 1º passo: quem não enche pega o que precisa; 2º: a sobra é dividida entre os que enchem.
        // Uma vez por filho: o size() de uma linha/coluna percorre os filhos dela.
        let fill: Vec<u16> = self.children.iter().map(|c| if max_main.is_finite() { self.main_len(c.0.size()).fill() } else { 0 }).collect();
        for (i, c) in self.children.iter().enumerate().filter(|(i, _)| fill[*i] == 0) {
            let n = cx.child(i, c, Limits::new(self.join(sobra, max_cross_in)));
            sobra = (sobra - self.split(n.size()).0).max(0.0);
            nos[i] = Some(n);
        }
        let total: u32 = fill.iter().map(|&f| f as u32).sum();
        let mut resto = sobra;
        for (i, c) in self.children.iter().enumerate().filter(|(i, _)| fill[*i] > 0) {
            let parte = (sobra * fill[i] as f32 / total as f32).floor().min(resto);
            resto -= parte;
            nos[i] = Some(cx.child(i, c, Limits::new(self.join(parte, max_cross_in))));
        }
        let mut nos: Vec<Node> = nos.into_iter().map(|n| n.expect("todo filho passou por um dos dois passos")).collect();
        let cross_conteudo = nos.iter().map(|n| self.split(n.size()).1).fold(0.0, f32::max);
        let cross_len = if self.axis == Axis::Row { self.height } else { self.width };
        let cross = Limits::resolve(cross_len, max_cross, cross_conteudo + pad_cross);
        let mut pos = if self.axis == Axis::Row { p.left } else { p.top };
        let cross0 = if self.axis == Axis::Row { p.top } else { p.left };
        for n in &mut nos {
            let (m, c) = self.split(n.size());
            let off = cross0 + self.align.offset(cross - pad_cross, c);
            n.place(match self.axis {
                Axis::Row => Point::new(pos, off),
                Axis::Column => Point::new(off, pos),
            });
            pos += m + self.spacing;
        }
        let usado = (pos - self.spacing).max(0.0) + if self.axis == Axis::Row { p.right } else { p.bottom };
        let main = Limits::resolve(self.main_len((self.width, self.height)), max_main, usado);
        Node { rect: Rect::new(Point::ORIGIN, self.join(main, cross)), key: cx.key(), children: nos, padding: p, ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        for (c, cn) in self.children.iter().zip(&n.children) {
            // Fora da rolagem (o recorte): não desenha. A folga cobre sombras e o foco.
            if !cx.accessible() && cx.renderer.clipped_out(cn.rect, 48.0) {
                continue;
            }
            c.0.draw(cn, cx);
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        for (c, cn) in self.children.iter().zip(&n.children) {
            if c.0.event(cn, ev, cx) == Captured::Yes {
                return Captured::Yes;
            }
        }
        Captured::No
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.children.iter().zip(&n.children).any(|(c, cn)| c.0.opaque_at(cn, p))
    }
}

// ---------- Pilha ----------

/// Camadas uma sobre a outra; a última fica por cima e recebe os eventos primeiro.
pub struct Stack<'a, M> {
    children: Vec<Element<'a, M>>,
    width: Length,
    height: Length,
}

pub fn stack<'a, M>(children: impl IntoIterator<Item = Element<'a, M>>) -> Stack<'a, M> {
    Stack { children: children.into_iter().collect(), width: Length::Shrink, height: Length::Shrink }
}

impl<'a, M: 'a> Stack<'a, M> {
    pub fn push(mut self, e: impl Into<Element<'a, M>>) -> Self {
        self.children.push(e.into());
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
}

impl<M> Widget<M> for Stack<'_, M> {
    fn size(&self) -> (Length, Length) {
        // Uma pilha enche se a camada de baixo enche: é o caso do fundo de janela.
        let base = self.children.first().map(|c| c.0.size()).unwrap_or((Length::Shrink, Length::Shrink));
        let pick = |own: Length, b: Length| if own == Length::Shrink { b } else { own };
        (pick(self.width, base.0), pick(self.height, base.1))
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let (wl, hl) = Widget::<M>::size(self);
        let max = Size::new(Limits::resolve(wl, l.max.width, l.max.width), Limits::resolve(hl, l.max.height, l.max.height));
        let nos: Vec<Node> = self.children.iter().enumerate().map(|(i, c)| cx.child(i, c, Limits::new(max))).collect();
        let w = nos.iter().map(|n| n.rect.width).fold(0.0, f32::max);
        let h = nos.iter().map(|n| n.rect.height).fold(0.0, f32::max);
        Node { rect: Rect::new(Point::ORIGIN, Size::new(Limits::resolve(wl, l.max.width, w), Limits::resolve(hl, l.max.height, h))), key: cx.key(), children: nos, ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let cursor = cx.cursor;
        for (i, (c, cn)) in self.children.iter().zip(&n.children).enumerate() {
            // O hover só vale na camada de cima que cobre o mouse.
            let coberto = cursor.is_some_and(|p| self.children[i + 1..].iter().zip(&n.children[i + 1..]).any(|(c, cn)| c.0.opaque_at(cn, p)));
            cx.cursor = if coberto { None } else { cursor };
            c.0.draw(cn, cx);
        }
        cx.cursor = cursor;
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let mouse = matches!(ev, Event::MousePressed(_) | Event::MouseReleased(_) | Event::Wheel { .. } | Event::CursorMoved(_));
        for (c, cn) in self.children.iter().zip(&n.children).rev() {
            if c.0.event(cn, ev, cx) == Captured::Yes {
                return Captured::Yes;
            }
            if mouse && cx.cursor.is_some_and(|p| c.0.opaque_at(cn, p)) {
                return Captured::Yes;
            }
        }
        Captured::No
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.children.iter().zip(&n.children).any(|(c, cn)| c.0.opaque_at(cn, p))
    }
}

/// Bloqueia o mouse para o que está abaixo na pilha (fundo de diálogo, menu).
pub struct Opaque<'a, M>(Element<'a, M>);

pub fn opaque<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Opaque<'a, M> {
    Opaque(content.into())
}

impl<M> Widget<M> for Opaque<'_, M> {
    fn size(&self) -> (Length, Length) {
        self.0 .0.size()
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let n = cx.child(0, &self.0, l);
        Node { rect: n.rect, key: cx.key(), children: vec![n], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        self.0 .0.draw(&n.children[0], cx)
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let c = self.0 .0.event(&n.children[0], ev, cx);
        let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
        if c == Captured::Yes || (dentro && matches!(ev, Event::MousePressed(_) | Event::MouseReleased(_) | Event::Wheel { .. })) {
            Captured::Yes
        } else {
            Captured::No
        }
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        n.rect.contains(p)
    }
}

// ---------- Botão ----------

#[derive(Default)]
struct Apertado(bool);

pub struct Button<'a, M> {
    content: Element<'a, M>,
    on_press: Option<M>,
    padding: Padding,
    width: Length,
    height: Length,
    style: Option<StyleFn<'a>>,
}

pub fn button<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Button<'a, M> {
    Button { content: content.into(), on_press: None, padding: [6u16, 12].into(), width: Length::Shrink, height: Length::Shrink, style: None }
}

impl<'a, M> Button<'a, M> {
    pub fn on_press(mut self, m: M) -> Self {
        self.on_press = Some(m);
        self
    }
    /// `None` = desabilitado.
    pub fn on_press_maybe(mut self, m: Option<M>) -> Self {
        self.on_press = m;
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    pub fn style(mut self, f: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.style = Some(Box::new(f));
        self
    }
}

/// O botão padrão: cor primária, mais clara no hover e mais escura apertado.
pub fn button_primary(t: &Theme, s: Status) -> Style {
    let base = match s {
        Status::Hovered => t.primary.mix(Color::WHITE, 0.12),
        Status::Pressed => t.primary.mix(Color::BLACK, 0.15),
        Status::Disabled => t.primary.alpha(0.45),
        _ => t.primary,
    };
    Style::default().background(base).border(Border::rounded(6.0)).text_color(t.on_primary).transition(HOVER)
}

/// A transição dos estilos padrão: o hover acende em vez de piscar.
const HOVER: Transition = Transition { duration: std::time::Duration::from_millis(120), easing: crate::Easing::Ease, delay: std::time::Duration::ZERO };

/// Botão sem fundo, que só acende no hover (barras de ferramentas).
pub fn button_text(t: &Theme, s: Status) -> Style {
    let bg = match s {
        Status::Hovered => Some(t.text.alpha(0.08)),
        Status::Pressed => Some(t.text.alpha(0.14)),
        _ => None,
    };
    Style { background: bg.map(Into::into), border: Border::rounded(6.0), text_color: Some(if s == Status::Disabled { t.muted } else { t.text }), transition: Some(HOVER), ..Default::default() }
}

impl<M: Clone> Widget<M> for Button<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let p = self.padding;
        let max_w = Limits::resolve(self.width, l.max.width, l.max.width) - p.horizontal();
        let max_h = Limits::resolve(self.height, l.max.height, l.max.height) - p.vertical();
        let mut filho = cx.child(0, &self.content, Limits::new(Size::new(max_w.max(0.0), max_h.max(0.0))));
        let c = filho.size();
        let w = Limits::resolve(self.width, l.max.width, c.width + p.horizontal());
        let h = Limits::resolve(self.height, l.max.height, c.height + p.vertical());
        filho.place(Point::new(p.left + Align::Center.offset(w - p.horizontal(), c.width), p.top + Align::Center.offset(h - p.vertical(), c.height)));
        Node { rect: Rect::new(Point::ORIGIN, Size::new(w, h)), key: cx.key(), children: vec![filho], padding: p, ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let hover = cx.hovered(&n.rect);
        let status = if self.on_press.is_none() {
            Status::Disabled
        } else if hover && cx.states.get::<Apertado>(n.key).is_some_and(|a| a.0) {
            Status::Pressed
        } else if hover {
            Status::Hovered
        } else {
            Status::Idle
        };
        if hover && self.on_press.is_some() {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        let st = match &self.style {
            Some(f) => f(cx.theme, status),
            None => button_primary(cx.theme, status),
        };
        let st = cx.styled(n.key, st);
        st.paint(n.rect, cx);
        let herdada = cx.text_color;
        if let Some(c) = st.text_color {
            cx.text_color = c;
        }
        let desabilitado = self.on_press.is_none();
        cx.access_begin(n.key, n.rect, || {
            let mut a = no(accesskit::Role::Button, &[accesskit::Action::Click, accesskit::Action::Focus]);
            if desabilitado {
                a.set_disabled();
            }
            a
        });
        self.content.0.draw(&n.children[0], cx);
        cx.access_end();
        cx.text_color = herdada;
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
        match ev {
            Event::MousePressed(MouseButton::Left) if dentro && self.on_press.is_some() => {
                // Um botão dentro deste (o × de uma aba) recebe o clique primeiro.
                if self.content.0.event(&n.children[0], ev, cx) == Captured::Yes {
                    return Captured::Yes;
                }
                cx.states.get_mut::<Apertado>(n.key).0 = true;
                cx.redraw = true;
                Captured::Yes
            }
            Event::MouseReleased(MouseButton::Left) => {
                if self.content.0.event(&n.children[0], ev, cx) == Captured::Yes {
                    return Captured::Yes;
                }
                let a = std::mem::take(&mut cx.states.get_mut::<Apertado>(n.key).0);
                if a {
                    cx.redraw = true;
                    if let (true, Some(m)) = (dentro, &self.on_press) {
                        cx.publish(m.clone());
                        return Captured::Yes;
                    }
                }
                Captured::No
            }
            _ => self.content.0.event(&n.children[0], ev, cx),
        }
    }
}

// ---------- Área do mouse ----------

pub struct MouseArea<'a, M> {
    content: Element<'a, M>,
    on_press: Option<M>,
    on_release: Option<M>,
    on_right_press: Option<M>,
    on_double_click: Option<M>,
    icon: Option<CursorIcon>,
    on_enter: Option<M>,
    on_exit: Option<M>,
    on_move: Option<Box<dyn Fn(Point) -> M + 'a>>,
    on_scroll: Option<Box<dyn Fn(f32, f32, Modifiers) -> M + 'a>>,
}

/// O mouse estava em cima no último evento ([`MouseArea::on_enter`]).
#[derive(Default)]
struct EmCima(bool);

#[derive(Default)]
struct UltimoClique(Option<std::time::Instant>);

pub fn mouse_area<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> MouseArea<'a, M> {
    MouseArea { content: content.into(), on_press: None, on_release: None, on_right_press: None, on_double_click: None, icon: None, on_enter: None, on_exit: None, on_move: None, on_scroll: None }
}

impl<'a, M> MouseArea<'a, M> {
    pub fn on_enter(mut self, m: M) -> Self {
        self.on_enter = Some(m);
        self
    }
    pub fn on_exit(mut self, m: M) -> Self {
        self.on_exit = Some(m);
        self
    }
    /// O mouse andou em cima (posição na janela, lógica).
    pub fn on_move(mut self, f: impl Fn(Point) -> M + 'a) -> Self {
        self.on_move = Some(Box::new(f));
        self
    }
    /// A roda em cima (`dx`, `dy` como no [`Event::Wheel`]); pega o evento, então o scrollable de fora não rola.
    pub fn on_scroll(mut self, f: impl Fn(f32, f32, Modifiers) -> M + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }
    pub fn on_press(mut self, m: M) -> Self {
        self.on_press = Some(m);
        self
    }
    pub fn on_release(mut self, m: M) -> Self {
        self.on_release = Some(m);
        self
    }
    pub fn on_right_press(mut self, m: M) -> Self {
        self.on_right_press = Some(m);
        self
    }
    /// Duplo clique (até 400 ms): na barra de título, maximizar.
    pub fn on_double_click(mut self, m: M) -> Self {
        self.on_double_click = Some(m);
        self
    }
    pub fn cursor_icon(mut self, i: CursorIcon) -> Self {
        self.icon = Some(i);
        self
    }
}

impl<M: Clone> Widget<M> for MouseArea<'_, M> {
    fn size(&self) -> (Length, Length) {
        self.content.0.size()
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let n = cx.child(0, &self.content, l);
        Node { rect: n.rect, key: cx.key(), children: vec![n], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        // Clicável vira botão para o leitor de tela (a linha de uma lista de apps).
        let clicavel = self.on_press.is_some() || self.on_release.is_some() || self.on_double_click.is_some();
        if clicavel {
            cx.access_begin(n.key, n.rect, || no(accesskit::Role::Button, &[accesskit::Action::Click]));
        }
        self.content.0.draw(&n.children[0], cx);
        if clicavel {
            cx.access_end();
        }
        if let Some(i) = self.icon.filter(|_| cx.hovered(&n.rect)) {
            cx.cursor_icon = i;
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
        if (self.on_enter.is_some() || self.on_exit.is_some()) && matches!(ev, Event::CursorMoved(_) | Event::CursorLeft) {
            let antes = std::mem::replace(&mut cx.states.get_mut::<EmCima>(n.key).0, dentro);
            match (antes, dentro) {
                (false, true) => self.on_enter.iter().for_each(|m| cx.publish(m.clone())),
                (true, false) => self.on_exit.iter().for_each(|m| cx.publish(m.clone())),
                _ => {}
            }
        }
        if self.content.0.event(&n.children[0], ev, cx) == Captured::Yes {
            return Captured::Yes;
        }
        if !dentro {
            return Captured::No;
        }
        match (ev, &self.on_move, &self.on_scroll) {
            (Event::CursorMoved(p), Some(f), _) => cx.publish(f(*p)),
            (Event::Wheel { dx, dy, modifiers }, _, Some(f)) => {
                cx.publish(f(*dx, *dy, *modifiers));
                return Captured::Yes;
            }
            _ => {}
        }
        let m = match ev {
            Event::MousePressed(MouseButton::Left) => {
                let agora = std::time::Instant::now();
                let ultimo = cx.states.get_mut::<UltimoClique>(n.key);
                let duplo = ultimo.0.is_some_and(|t| agora.duration_since(t).as_millis() < 400);
                ultimo.0 = if duplo { None } else { Some(agora) };
                if duplo && self.on_double_click.is_some() {
                    &self.on_double_click
                } else {
                    &self.on_press
                }
            }
            Event::MouseReleased(MouseButton::Left) => &self.on_release,
            Event::MousePressed(MouseButton::Right) => &self.on_right_press,
            _ => &None,
        };
        match m {
            Some(m) => {
                cx.publish(m.clone());
                Captured::Yes
            }
            None => Captured::No,
        }
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.content.0.opaque_at(&n.children[0], p)
    }
}

// ---------- Rolagem ----------

#[derive(Default)]
struct Rolagem {
    offset: Point,
    /// Pedido de [`crate::Task::snap_to`] (0 a 1 em cada eixo), que só o layout sabe converter.
    relativo: Option<Point>,
    /// O polegar arrastado: vertical ou não, e a distância do clique até o começo dele.
    arrastando: Option<(bool, f32)>,
    /// O último deslocamento avisado pelo `on_scroll`.
    avisado: Option<Point>,
}

/// Muda a rolagem de `key` (os pedidos de [`crate::Task::scroll_to`]).
pub(crate) fn rolar(states: &mut States, key: u64, p: Point, relativo: bool) {
    let r = states.get_mut::<Rolagem>(key);
    if relativo {
        r.relativo = Some(p);
    } else {
        r.offset = p;
        r.relativo = None;
    }
}

/// Em que eixos um [`Scrollable`] rola.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

impl Direction {
    fn x(self) -> bool {
        self != Direction::Vertical
    }
    fn y(self) -> bool {
        self != Direction::Horizontal
    }
}

/// Onde a rolagem está, para o [`Scrollable::on_scroll`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub offset: Point,
    /// O tamanho da parte visível.
    pub bounds: Size,
    /// O tamanho do conteúdo inteiro.
    pub content: Size,
}

impl Viewport {
    /// O deslocamento em fração (0 a 1) de cada eixo; sem o que rolar, 0.
    pub fn relative(&self) -> Point {
        let f = |o: f32, c: f32, b: f32| if c > b { o / (c - b) } else { 0.0 };
        Point::new(f(self.offset.x, self.content.width, self.bounds.width), f(self.offset.y, self.content.height, self.bounds.height))
    }
}

pub struct Scrollable<'a, M> {
    content: Element<'a, M>,
    width: Length,
    height: Length,
    direction: Direction,
    id: Option<u64>,
    on_scroll: Option<Box<dyn Fn(Viewport) -> M + 'a>>,
}

pub fn scrollable<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Scrollable<'a, M> {
    Scrollable { content: content.into(), width: Length::FILL, height: Length::FILL, direction: Direction::Vertical, id: None, on_scroll: None }
}

impl<'a, M> Scrollable<'a, M> {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    /// Vertical (padrão), horizontal ou os dois. A roda com Shift rola na horizontal.
    pub fn direction(mut self, d: Direction) -> Self {
        self.direction = d;
        self
    }
    /// O nome para [`crate::Task::scroll_to`] / [`crate::Task::snap_to`].
    pub fn id(mut self, id: impl std::hash::Hash) -> Self {
        self.id = Some(crate::id_hash(&id));
        self
    }
    /// Avisa cada mudança do deslocamento (rolagem do usuário ou pedida).
    pub fn on_scroll(mut self, f: impl Fn(Viewport) -> M + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }
    fn viewport(n: &Node) -> Viewport {
        let c = &n.children[0].rect;
        Viewport { offset: Point::new(n.rect.x - c.x, n.rect.y - c.y), bounds: n.rect.size(), content: c.size() }
    }
    /// O polegar de cada eixo que tem o que rolar: (vertical, horizontal).
    fn thumbs(&self, n: &Node) -> (Option<Rect>, Option<Rect>) {
        let v = Self::viewport(n);
        let (r, c) = (n.rect, v.content);
        let ambos = self.direction.y() && self.direction.x() && c.height > r.height && c.width > r.width;
        let canto = if ambos { 8.0 } else { 0.0 };
        let vertical = (self.direction.y() && c.height > r.height).then(|| {
            let curso = r.height - canto;
            let h = (curso * r.height / c.height).max(24.0);
            Rect { x: r.x + r.width - 8.0, y: r.y + (curso - h) * v.offset.y / (c.height - r.height), width: 6.0, height: h }
        });
        let horizontal = (self.direction.x() && c.width > r.width).then(|| {
            let curso = r.width - canto;
            let w = (curso * r.width / c.width).max(24.0);
            Rect { x: r.x + (curso - w) * v.offset.x / (c.width - r.width), y: r.y + r.height - 8.0, width: w, height: 6.0 }
        });
        (vertical, horizontal)
    }
}

impl<M> Widget<M> for Scrollable<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        // No layout (e não no desenho): o Task acha a rolagem mesmo fora da tela.
        if let Some(id) = self.id {
            cx.states.ids.borrow_mut().insert(id, cx.key());
        }
        let w = Limits::resolve(self.width, l.max.width, l.max.width);
        let hmax = Limits::resolve(self.height, l.max.height, l.max.height);
        let livre = Size::new(if self.direction.x() { f32::INFINITY } else { w }, if self.direction.y() { f32::INFINITY } else { hmax });
        let mut filho = cx.child(0, &self.content, Limits::new(livre));
        let h = Limits::resolve(self.height, l.max.height, filho.rect.height);
        let w = Limits::resolve(self.width, l.max.width, filho.rect.width);
        let max = Point::new((filho.rect.width - w).max(0.0), (filho.rect.height - h).max(0.0));
        let off = match cx.states.get::<Rolagem>(cx.key()) {
            Some(Rolagem { relativo: Some(r), .. }) => Point::new(r.x.clamp(0.0, 1.0) * max.x, r.y.clamp(0.0, 1.0) * max.y),
            Some(r) => Point::new(r.offset.x.clamp(0.0, max.x), r.offset.y.clamp(0.0, max.y)),
            None => Point::ORIGIN,
        };
        filho.place(Point::new(-off.x, -off.y));
        Node { rect: Rect::new(Point::ORIGIN, Size::new(w, h)), key: cx.key(), children: vec![filho], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let cursor = cx.cursor;
        cx.cursor = cursor.filter(|p| n.rect.contains(*p));
        cx.renderer.push_clip(n.rect, [0.0; 4]);
        cx.access_begin(n.key, n.rect, || no(accesskit::Role::ScrollView, &[]));
        self.content.0.draw(&n.children[0], cx);
        cx.access_end();
        cx.renderer.pop_clip();
        cx.cursor = cursor;
        let ativo = cx.hovered(&n.rect) || cx.states.get::<Rolagem>(n.key).is_some_and(|r| r.arrastando.is_some());
        let (v, h) = self.thumbs(n);
        for t in [v, h].into_iter().flatten() {
            cx.renderer.fill_quad(t, Some(&cx.theme.text.alpha(if ativo { 0.35 } else { 0.18 }).into()), Border::rounded(3.0), None);
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let antes = Self::viewport(n);
        // Um snap_to já virou posição no layout: guarda ela, e a rolagem segue dali.
        let r = cx.states.get_mut::<Rolagem>(n.key);
        if r.relativo.take().is_some() {
            r.offset = antes.offset;
        }
        let c = self.capturar(n, ev, cx, antes);
        if let Some(f) = &self.on_scroll {
            let r = cx.states.get_mut::<Rolagem>(n.key);
            let max = Point::new((antes.content.width - antes.bounds.width).max(0.0), (antes.content.height - antes.bounds.height).max(0.0));
            let agora = Point::new(r.offset.x.clamp(0.0, max.x), r.offset.y.clamp(0.0, max.y));
            if r.avisado != Some(agora) {
                let primeira = r.avisado.is_none();
                r.avisado = Some(agora);
                if !primeira || agora != Point::ORIGIN {
                    cx.publish(f(Viewport { offset: agora, ..antes }));
                }
            }
        }
        c
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        n.rect.contains(p) && self.content.0.opaque_at(&n.children[0], p)
    }
}

impl<M> Scrollable<'_, M> {
    fn capturar(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>, v: Viewport) -> Captured {
        let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
        let max = Point::new((v.content.width - v.bounds.width).max(0.0), (v.content.height - v.bounds.height).max(0.0));
        let (tv, th) = self.thumbs(n);
        match ev {
            // A rolagem de dentro (lista dentro da página) tem a vez; só o que ela não usar rola esta.
            Event::Wheel { dx, dy, modifiers } if dentro && (max.x > 0.0 || max.y > 0.0) => {
                if self.content.0.event(&n.children[0], ev, cx) == Captured::Yes {
                    return Captured::Yes;
                }
                // Shift troca o eixo da roda (mouse comum só tem a vertical).
                let (dx, dy) = if modifiers.shift_key() && *dx == 0.0 { (*dy, 0.0) } else { (*dx, *dy) };
                let (dx, dy) = if !self.direction.y() && dx == 0.0 { (dy, 0.0) } else { (dx, dy) };
                let novo = Point::new(if self.direction.x() { (v.offset.x - dx).clamp(0.0, max.x) } else { v.offset.x }, if self.direction.y() { (v.offset.y - dy).clamp(0.0, max.y) } else { v.offset.y });
                if novo != v.offset {
                    cx.states.get_mut::<Rolagem>(n.key).offset = novo;
                    cx.redraw = true;
                    return Captured::Yes;
                }
                // O conteúdo já viu o evento: não manda de novo lá embaixo.
                return Captured::No;
            }
            Event::MousePressed(MouseButton::Left) => {
                if let Some(p) = cx.cursor {
                    for (vertical, t) in [(true, tv), (false, th)] {
                        let Some(t) = t else { continue };
                        let alvo = if vertical { Rect { x: t.x - 4.0, width: t.width + 6.0, ..t } } else { Rect { y: t.y - 4.0, height: t.height + 6.0, ..t } };
                        if alvo.contains(p) {
                            cx.states.get_mut::<Rolagem>(n.key).arrastando = Some((vertical, if vertical { p.y - t.y } else { p.x - t.x }));
                            return Captured::Yes;
                        }
                    }
                }
            }
            Event::MouseReleased(MouseButton::Left) => {
                if cx.states.get_mut::<Rolagem>(n.key).arrastando.take().is_some() {
                    cx.redraw = true;
                    return Captured::Yes;
                }
            }
            Event::CursorMoved(p) => {
                let r = cx.states.get_mut::<Rolagem>(n.key);
                match (r.arrastando, tv, th) {
                    (Some((true, pega)), Some(t), _) => {
                        r.offset.y = ((p.y - pega - n.rect.y) / (n.rect.height - t.height).max(1.0) * max.y).clamp(0.0, max.y);
                        cx.redraw = true;
                        return Captured::Yes;
                    }
                    (Some((false, pega)), _, Some(t)) => {
                        r.offset.x = ((p.x - pega - n.rect.x) / (n.rect.width - t.width).max(1.0) * max.x).clamp(0.0, max.x);
                        cx.redraw = true;
                        return Captured::Yes;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Clique na parte escondida do conteúdo não vale; num menu aberto de dentro, vale.
        if !dentro && matches!(ev, Event::MousePressed(_)) && cx.states.sob_popup.get().is_none() {
            return Captured::No;
        }
        self.content.0.event(&n.children[0], ev, cx)
    }
}

// ---------- Campo de texto ----------

#[derive(Default)]
struct Cursor(usize);

pub struct TextInput<'a, M> {
    placeholder: Cow<'a, str>,
    value: Cow<'a, str>,
    on_input: Option<Box<dyn Fn(String) -> M + 'a>>,
    on_submit: Option<M>,
    size: f32,
    font: Font,
    padding: Padding,
    width: Length,
    secure: bool,
    style: Option<StyleFn<'a>>,
    autofocus: Option<u64>,
    id: Option<u64>,
}

/// O último pedido de foco ([`TextInput::autofocus`]) que o widget já atendeu.
#[derive(Default)]
pub(crate) struct Autofoco(Option<u64>);

/// Há um pedido de foco novo para `key`: a próxima tecla é dele.
pub(crate) fn foco_pendente(states: &States, key: u64, pedido: Option<u64>) -> bool {
    pedido.is_some_and(|p| states.focus != Some(key) && states.get::<Autofoco>(key).and_then(|a| a.0) != Some(p))
}

/// Atende um pedido de foco: a tecla toma o foco, um clique em qualquer lugar só o cancela.
pub(crate) fn atender_foco(ev: &Event, states: &mut States, key: u64, pedido: Option<u64>) {
    if !foco_pendente(states, key, pedido) {
        return;
    }
    match ev {
        Event::Key { pressed: true, .. } => {
            states.get_mut::<Autofoco>(key).0 = pedido;
            states.focus = Some(key);
        }
        Event::MousePressed(_) => states.get_mut::<Autofoco>(key).0 = pedido,
        _ => {}
    }
}

pub fn text_input<'a, M>(placeholder: impl Into<Cow<'a, str>>, value: impl Into<Cow<'a, str>>) -> TextInput<'a, M> {
    TextInput { placeholder: placeholder.into(), value: value.into(), on_input: None, on_submit: None, size: 14.0, font: Font::DEFAULT, padding: [6u16, 10].into(), width: Length::FILL, secure: false, style: None, autofocus: None, id: None }
}

impl<'a, M> TextInput<'a, M> {
    /// O nome para [`crate::Task::focus`].
    pub fn id(mut self, id: impl std::hash::Hash) -> Self {
        self.id = Some(crate::id_hash(&id));
        self
    }
    pub fn on_input(mut self, f: impl Fn(String) -> M + 'a) -> Self {
        self.on_input = Some(Box::new(f));
        self
    }
    pub fn on_submit(mut self, m: M) -> Self {
        self.on_submit = Some(m);
        self
    }
    /// Pede o foco: a próxima tecla digitada vem para o campo (um clique antes cancela).
    /// Mude o número para pedir de novo (ao reabrir um painel de busca, por exemplo).
    pub fn autofocus(mut self, pedido: u64) -> Self {
        self.autofocus = Some(pedido);
        self
    }
    pub fn size(mut self, s: impl Into<Num>) -> Self {
        self.size = s.into().0;
        self
    }
    pub fn font(mut self, f: Font) -> Self {
        self.font = f;
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    /// Mostra bolinhas no lugar das letras (senha).
    pub fn secure(mut self, s: bool) -> Self {
        self.secure = s;
        self
    }
    pub fn style(mut self, f: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.style = Some(Box::new(f));
        self
    }
    fn shown(&self) -> Cow<'_, str> {
        if self.secure {
            "•".repeat(self.value.chars().count()).into()
        } else {
            Cow::Borrowed(&self.value)
        }
    }
    /// Byte do valor real -> byte do texto mostrado (diferem só no modo senha).
    fn shown_index(&self, i: usize) -> usize {
        if self.secure {
            self.value[..i].chars().count() * '•'.len_utf8()
        } else {
            i
        }
    }
    fn caret_x(&self, r: &mut Renderer, i: usize) -> f32 {
        let shown = self.shown();
        let i = self.shown_index(i);
        let carets = r.carets(&shown, self.size, self.font);
        carets.iter().find(|c| c.0 >= i).map_or_else(|| carets.last().map_or(0.0, |c| c.2), |c| c.1)
    }
    fn scroll(&self, r: &mut Renderer, cursor: usize, largura: f32) -> f32 {
        (self.caret_x(r, cursor) - largura + 2.0).max(0.0)
    }
}

/// O campo padrão: superfície com borda que fica na cor primária com foco.
pub fn input_default(t: &Theme, s: Status) -> Style {
    let borda = match s {
        Status::Focused => t.primary,
        Status::Hovered => t.muted,
        _ => t.border,
    };
    Style::default().background(t.surface).border(Border::rounded(6.0).color(borda, 1.0)).text_color(t.text).transition(HOVER)
}

impl<M: Clone> Widget<M> for TextInput<'_, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        if let Some(id) = self.id {
            cx.states.ids.borrow_mut().insert(id, cx.key());
        }
        let h = cx.renderer.measure("Ag", self.size, self.font, None).height + self.padding.vertical();
        let w = Limits::resolve(self.width, l.max.width, 160.0);
        Node::leaf(cx.key(), Size::new(w, h))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let focado = cx.states.focus == Some(n.key) || foco_pendente(cx.states, n.key, self.autofocus);
        let hover = cx.hovered(&n.rect);
        let status = if self.on_input.is_none() {
            Status::Disabled
        } else if focado {
            Status::Focused
        } else if hover {
            Status::Hovered
        } else {
            Status::Idle
        };
        if hover {
            cx.cursor_icon = CursorIcon::Text;
        }
        let st = match &self.style {
            Some(f) => f(cx.theme, status),
            None => input_default(cx.theme, status),
        };
        let st = cx.styled(n.key, st);
        st.paint(n.rect, cx);
        let (secreto, desabilitado) = (self.secure, self.on_input.is_none());
        cx.access(n.key, n.rect, || {
            let mut a = no(if secreto { accesskit::Role::PasswordInput } else { accesskit::Role::TextInput }, &[accesskit::Action::Click, accesskit::Action::Focus]);
            if !secreto {
                a.set_value(&*self.value);
            }
            if !self.placeholder.is_empty() {
                a.set_placeholder(&*self.placeholder);
            }
            if desabilitado {
                a.set_disabled();
            }
            a
        });
        let dentro = n.rect.shrink(self.padding);
        let cor = st.text_color.unwrap_or(cx.text_color);
        let mut cursor = cx.states.get::<Cursor>(n.key).map_or(self.value.len(), |c| c.0).min(self.value.len());
        // O app pode ter trocado o valor: o cursor velho pode cair no meio de um caractere.
        while !self.value.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let rolar = if focado { self.scroll(cx.renderer, cursor, dentro.width) } else { 0.0 };
        cx.renderer.push_clip(dentro, [0.0; 4]);
        if self.value.is_empty() {
            cx.renderer.draw_text(&self.placeholder, self.size, self.font, cx.theme.muted, Point::new(dentro.x, dentro.y), None);
        } else {
            cx.renderer.draw_text(&self.shown(), self.size, self.font, cor, Point::new(dentro.x - rolar, dentro.y), None);
        }
        if focado {
            let x = dentro.x + self.caret_x(cx.renderer, cursor) - rolar;
            cx.renderer.fill_quad(Rect { x, y: dentro.y, width: 1.0, height: dentro.height }, Some(&cor.into()), Border::default(), None);
        }
        cx.renderer.pop_clip();
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let Some(on_input) = &self.on_input else { return Captured::No };
        atender_foco(ev, cx.states, n.key, self.autofocus);
        let v = &self.value;
        let focado = cx.states.focus == Some(n.key);
        let mut c = cx.states.get::<Cursor>(n.key).map_or(v.len(), |c| c.0).min(v.len());
        while !v.is_char_boundary(c) {
            c -= 1;
        }
        let anterior = |c: usize| v[..c].char_indices().next_back().map_or(0, |(i, _)| i);
        let proximo = |c: usize| v[c..].chars().next().map_or(c, |ch| c + ch.len_utf8());
        match ev {
            Event::MousePressed(MouseButton::Left) if cx.cursor.is_some_and(|p| n.rect.contains(p)) => {
                let x = cx.cursor.map_or(0.0, |p| p.x) - n.rect.x - self.padding.left + if focado { self.scroll(cx.renderer, c, n.rect.width - self.padding.horizontal()) } else { 0.0 };
                let shown = self.shown();
                let carets = cx.renderer.carets(&shown, self.size, self.font);
                let i = carets.iter().find(|g| x < (g.1 + g.2) / 2.0).map_or(shown.len(), |g| g.0);
                // No modo senha cada bolinha é um caractere do valor.
                let i = if self.secure { v.char_indices().nth(i / '•'.len_utf8()).map_or(v.len(), |(b, _)| b) } else { i };
                cx.states.focus = Some(n.key);
                cx.states.get_mut::<Cursor>(n.key).0 = i;
                cx.redraw = true;
                Captured::Yes
            }
            Event::Key { key, text, modifiers, pressed: true } if focado => {
                let mut novo: Option<String> = None;
                match key {
                    Key::Named(NamedKey::Backspace) if c > 0 => {
                        let i = anterior(c);
                        novo = Some(format!("{}{}", &v[..i], &v[c..]));
                        c = i;
                    }
                    Key::Named(NamedKey::Delete) if c < v.len() => novo = Some(format!("{}{}", &v[..c], &v[proximo(c)..])),
                    Key::Named(NamedKey::ArrowLeft) => c = anterior(c),
                    Key::Named(NamedKey::ArrowRight) => c = proximo(c),
                    Key::Named(NamedKey::Home) => c = 0,
                    Key::Named(NamedKey::End) => c = v.len(),
                    Key::Named(NamedKey::Enter) => {
                        if let Some(m) = &self.on_submit {
                            cx.publish(m.clone());
                        }
                    }
                    Key::Named(NamedKey::Escape) => cx.states.focus = None,
                    // shortcut: sem seleção ainda, copiar e recortar pegam o campo inteiro; com Shift+setas e arrastar, só o trecho.
                    Key::Character(k) if modifiers.control_key() && k.eq_ignore_ascii_case("v") => {
                        // Uma linha só: quebras de linha coladas viram espaço.
                        let t: String = cx.clipboard.read().unwrap_or_default().chars().map(|ch| if ch == '\n' || ch == '\r' || ch == '\t' { ' ' } else { ch }).filter(|ch| !ch.is_control()).collect();
                        if !t.is_empty() {
                            novo = Some(format!("{}{}{}", &v[..c], t, &v[c..]));
                            c += t.len();
                        }
                    }
                    Key::Character(k) if modifiers.control_key() && !self.secure && (k.eq_ignore_ascii_case("c") || k.eq_ignore_ascii_case("x")) => {
                        if !v.is_empty() {
                            cx.clipboard.write(v.to_string());
                            if k.eq_ignore_ascii_case("x") {
                                novo = Some(String::new());
                                c = 0;
                            }
                        }
                    }
                    _ => match text.as_deref().filter(|t| !modifiers.control_key() && t.chars().all(|ch| !ch.is_control())) {
                        Some(t) if !t.is_empty() => {
                            novo = Some(format!("{}{}{}", &v[..c], t, &v[c..]));
                            c += t.len();
                        }
                        _ => return Captured::No,
                    },
                }
                cx.states.get_mut::<Cursor>(n.key).0 = c;
                if let Some(s) = novo {
                    cx.publish(on_input(s));
                }
                cx.redraw = true;
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
}

// ---------- Imagem e SVG ----------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ContentFit {
    /// Estica até preencher, sem manter a proporção.
    #[default]
    Fill,
    /// Cabe inteira, com sobra.
    Contain,
    /// Cobre tudo, cortando o que passa.
    Cover,
}

fn fit(fit: ContentFit, r: Rect, natural: Size) -> Rect {
    let (sx, sy) = (r.width / natural.width.max(1.0), r.height / natural.height.max(1.0));
    let s = match fit {
        ContentFit::Fill => return r,
        ContentFit::Contain => sx.min(sy),
        ContentFit::Cover => sx.max(sy),
    };
    let (w, h) = (natural.width * s, natural.height * s);
    Rect { x: r.x + (r.width - w) / 2.0, y: r.y + (r.height - h) / 2.0, width: w, height: h }
}

fn natural_layout(key: u64, (wl, hl): (Length, Length), l: Limits, natural: Size) -> Node {
    let w = Limits::resolve(wl, l.max.width, natural.width);
    let h = match (wl, hl) {
        // Só a largura dada: a altura acompanha a proporção.
        (Length::Fixed(_), Length::Shrink) => w * natural.height / natural.width.max(1.0),
        _ => Limits::resolve(hl, l.max.height, natural.height),
    };
    Node::leaf(key, Size::new(w, h))
}

pub struct ImageView {
    image: Image,
    width: Length,
    height: Length,
    fit: ContentFit,
    radius: [f32; 4],
}

pub fn image(image: Image) -> ImageView {
    ImageView { image, width: Length::Shrink, height: Length::Shrink, fit: ContentFit::Contain, radius: [0.0; 4] }
}

impl ImageView {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    pub fn content_fit(mut self, f: ContentFit) -> Self {
        self.fit = f;
        self
    }
    /// Cantos arredondados da própria imagem.
    pub fn radius(mut self, r: f32) -> Self {
        self.radius = [r; 4];
        self
    }
}

impl<M> Widget<M> for ImageView {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        natural_layout(cx.key(), (self.width, self.height), l, self.image.size())
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let alvo = fit(self.fit, n.rect, self.image.size());
        if self.fit == ContentFit::Cover {
            cx.renderer.push_clip(n.rect, self.radius);
            cx.renderer.draw_image(&self.image, alvo, [0.0; 4]);
            cx.renderer.pop_clip();
        } else {
            cx.renderer.draw_image(&self.image, alvo, self.radius);
        }
    }
}

pub struct SvgView {
    svg: Svg,
    width: Length,
    height: Length,
    color: Option<Color>,
}

pub fn svg(svg: Svg) -> SvgView {
    SvgView { svg, width: Length::Shrink, height: Length::Shrink, color: None }
}

impl SvgView {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
    /// Pinta o SVG inteiro nesta cor (ícone que segue o tema).
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
}

impl<M> Widget<M> for SvgView {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        natural_layout(cx.key(), (self.width, self.height), l, self.svg.size())
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let r = fit(ContentFit::Contain, n.rect, self.svg.size());
        cx.renderer.draw_svg(&self.svg, r, self.color);
    }
}

// ---------- Barra de progresso ----------

pub struct ProgressBar {
    fraction: f32,
    width: Length,
    height: f32,
    color: Option<Color>,
}

/// `value` dentro de `range`; fora dele, a barra fica vazia ou cheia.
pub fn progress_bar(range: std::ops::RangeInclusive<f32>, value: f32) -> ProgressBar {
    let span = range.end() - range.start();
    let fraction = if span > 0.0 { ((value - range.start()) / span).clamp(0.0, 1.0) } else { 0.0 };
    ProgressBar { fraction, width: Length::FILL, height: 6.0, color: None }
}

impl ProgressBar {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
}

impl<M> Widget<M> for ProgressBar {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Fixed(self.height))
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 120.0), self.height))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let raio = Border::rounded(self.height / 2.0);
        cx.renderer.fill_quad(n.rect, Some(&cx.theme.text.alpha(0.12).into()), raio, None);
        let f = self.fraction as f64;
        // A barra escorrega até o valor novo em vez de pular.
        let fraction = cx.transition(n.key, self.fraction, Transition::new(300).easing(crate::Easing::EaseOut));
        cx.access(n.key, n.rect, || {
            let mut a = no(accesskit::Role::ProgressIndicator, &[]);
            a.set_numeric_value(f * 100.0);
            a.set_min_numeric_value(0.0);
            a.set_max_numeric_value(100.0);
            a
        });
        if fraction > 0.0 {
            let cheio = Rect { width: n.rect.width * fraction, ..n.rect };
            cx.renderer.fill_quad(cheio, Some(&self.color.unwrap_or(cx.theme.primary).into()), raio, None);
        }
    }
}

// ---------- Tooltip ----------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TooltipPosition {
    Top,
    Bottom,
    Left,
    Right,
}

pub struct Tooltip<'a, M> {
    content: Element<'a, M>,
    tip: Cow<'a, str>,
    position: TooltipPosition,
    padding: Padding,
    style: Option<Box<dyn Fn(&Theme) -> Style + 'a>>,
}

pub fn tooltip<'a, M: 'a>(content: impl Into<Element<'a, M>>, tip: impl Into<Cow<'a, str>>, position: TooltipPosition) -> Tooltip<'a, M> {
    Tooltip { content: content.into(), tip: tip.into(), position, padding: Padding { top: 4.0, right: 8.0, bottom: 4.0, left: 8.0 }, style: None }
}

impl<'a, M> Tooltip<'a, M> {
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    /// A caixa do balão: fundo, borda, sombra e cor do texto (o padrão é a superfície do tema).
    pub fn style(mut self, f: impl Fn(&Theme) -> Style + 'a) -> Self {
        self.style = Some(Box::new(f));
        self
    }
}

impl<'a, M> Widget<M> for Tooltip<'a, M> {
    fn size(&self) -> (Length, Length) {
        self.content.0.size()
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let n = cx.child(0, &self.content, l);
        Node { rect: n.rect, key: cx.key(), children: vec![n], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        self.content.0.draw(&n.children[0], cx);
        // Aparece depois de um tempo parado em cima; some logo.
        let em_cima = cx.hovered(&n.rect);
        let p = cx.transition(n.key, if em_cima { 1.0f32 } else { 0.0 }, if em_cima { Transition::new(120).delay(350) } else { Transition::new(90) });
        if p <= 0.0 {
            return;
        }
        let theme = *cx.theme;
        let st = match &self.style {
            Some(f) => f(&theme),
            None => Style { background: Some(theme.surface.into()), border: Border::rounded(6.0).color(theme.border, 1.0), shadow: Some(Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 2.0), blur: 6.0 }), ..Default::default() },
        };
        let (tip, r, pos, pd) = (self.tip.to_string(), n.rect, self.position, self.padding);
        let janela = cx.renderer.window_size();
        cx.overlay(move |rd| {
            rd.push_opacity(p);
            let t = rd.measure(&tip, 12.0, Font::DEFAULT, Some(320.0));
            let (w, h) = (t.width + pd.horizontal(), t.height + pd.vertical());
            let (x, y) = match pos {
                TooltipPosition::Bottom => (r.x + (r.width - w) / 2.0, r.y + r.height + 6.0),
                TooltipPosition::Top => (r.x + (r.width - w) / 2.0, r.y - h - 6.0),
                TooltipPosition::Left => (r.x - w - 6.0, r.y + (r.height - h) / 2.0),
                TooltipPosition::Right => (r.x + r.width + 6.0, r.y + (r.height - h) / 2.0),
            };
            let x = x.clamp(4.0, (janela.width - w - 4.0).max(4.0));
            let caixa = Rect { x, y, width: w, height: h };
            rd.fill_quad(caixa, st.background.as_ref(), st.border, st.shadow);
            rd.draw_text(&tip, 12.0, Font::DEFAULT, st.text_color.unwrap_or(theme.text), Point::new(x + pd.left, y + pd.top), Some(320.0));
            rd.pop_opacity();
        });
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        self.content.0.event(&n.children[0], ev, cx)
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.content.0.opaque_at(&n.children[0], p)
    }
}

// ---------- Canvas: desenho livre ----------

pub struct Canvas<'a> {
    width: Length,
    height: Length,
    paint: Box<dyn Fn(&mut Renderer, Rect, &Theme) + 'a>,
}

/// Um retângulo onde você desenha o que quiser com o [`Renderer`] — gráficos, réguas, prévias.
pub fn canvas<'a>(width: impl Into<Length>, height: impl Into<Length>, paint: impl Fn(&mut Renderer, Rect, &Theme) + 'a) -> Canvas<'a> {
    Canvas { width: width.into(), height: height.into(), paint: Box::new(paint) }
}

impl<M> Widget<M> for Canvas<'_> {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 0.0), Limits::resolve(self.height, l.max.height, 0.0)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        cx.renderer.push_clip(n.rect, [0.0; 4]);
        (self.paint)(cx.renderer, n.rect, cx.theme);
        cx.renderer.pop_clip();
    }
}

// ---------- Canvas interativo ----------

/// Um desenho que também recebe eventos e guarda estado próprio (arrastar, selecionar,
/// zoom): o `canvas::Program` do Iced. O estado fica guardado pela posição na árvore.
pub trait Program<M> {
    type State: Default + 'static;
    fn draw(&self, state: &Self::State, renderer: &mut Renderer, bounds: Rect, theme: &Theme, cursor: Option<Point>);
    /// Cada evento da janela (também os de fora de `bounds`: soltar o arrasto lá fora).
    /// `publish` manda mensagens; `Captured::Yes` impede que os de baixo vejam o evento.
    fn update(&self, state: &mut Self::State, event: &Event, bounds: Rect, cursor: Option<Point>, publish: &mut dyn FnMut(M)) -> Captured {
        let _ = (state, event, bounds, cursor, publish);
        Captured::No
    }
    /// O ícone do mouse em cima do desenho.
    fn mouse_interaction(&self, state: &Self::State, bounds: Rect, cursor: Option<Point>) -> CursorIcon {
        let _ = (state, bounds, cursor);
        CursorIcon::Default
    }
}

pub struct ProgramCanvas<P> {
    program: P,
    width: Length,
    height: Length,
}

/// O [`Program`] num retângulo; o padrão ocupa todo o espaço.
pub fn program<P>(program: P) -> ProgramCanvas<P> {
    ProgramCanvas { program, width: Length::FILL, height: Length::FILL }
}

impl<P> ProgramCanvas<P> {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn height(mut self, h: impl Into<Length>) -> Self {
        self.height = h.into();
        self
    }
}

impl<M, P: Program<M>> Widget<M> for ProgramCanvas<P> {
    fn size(&self) -> (Length, Length) {
        (self.width, self.height)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        Node::leaf(cx.key(), Size::new(Limits::resolve(self.width, l.max.width, 0.0), Limits::resolve(self.height, l.max.height, 0.0)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let novo;
        let st = match cx.states.get::<P::State>(n.key) {
            Some(s) => s,
            None => {
                novo = P::State::default();
                &novo
            }
        };
        if cx.hovered(&n.rect) {
            cx.cursor_icon = self.program.mouse_interaction(st, n.rect, cx.cursor);
        }
        cx.renderer.push_clip(n.rect, [0.0; 4]);
        self.program.draw(st, cx.renderer, n.rect, cx.theme, cx.cursor);
        cx.renderer.pop_clip();
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let cursor = cx.cursor;
        let mut msgs = Vec::new();
        let c = self.program.update(cx.states.get_mut::<P::State>(n.key), ev, n.rect, cursor, &mut |m| msgs.push(m));
        // O estado pode ter mudado sem mensagem (hover, arrasto): o quadro é gravado e comparado.
        cx.redraw = true;
        for m in msgs {
            cx.publish(m);
        }
        c
    }
}

impl<'a, M: 'a, P: Program<M> + 'a> From<ProgramCanvas<P>> for Element<'a, M> {
    fn from(w: ProgramCanvas<P>) -> Self {
        Element::new(w)
    }
}

// ---------- Responsivo ----------

pub struct Responsive<'a, M> {
    view: Box<dyn Fn(Size) -> Element<'a, M> + 'a>,
    /// O conteúdo montado no último layout (desenho e eventos usam o mesmo).
    montado: std::cell::RefCell<Option<Element<'a, M>>>,
}

/// O conteúdo é montado já sabendo o espaço que tem (caber a página na largura, trocar de
/// layout em tela estreita). Ocupa todo o espaço.
pub fn responsive<'a, M>(view: impl Fn(Size) -> Element<'a, M> + 'a) -> Responsive<'a, M> {
    Responsive { view: Box::new(view), montado: std::cell::RefCell::new(None) }
}

impl<M> Widget<M> for Responsive<'_, M> {
    fn size(&self) -> (Length, Length) {
        (Length::FILL, Length::FILL)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let max = l.fill(Size::new(400.0, 300.0));
        let el = (self.view)(max);
        let n = cx.child(0, &el, Limits::new(max));
        *self.montado.borrow_mut() = Some(el);
        Node { rect: Rect::new(Point::ORIGIN, max), key: cx.key(), children: vec![n], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        if let Some(el) = &*self.montado.borrow() {
            el.0.draw(&n.children[0], cx);
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        match &*self.montado.borrow() {
            Some(el) => el.0.event(&n.children[0], ev, cx),
            None => Captured::No,
        }
    }
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.montado.borrow().as_ref().is_some_and(|el| el.0.opaque_at(&n.children[0], p))
    }
}

impl<'a, M: 'a> From<Responsive<'a, M>> for Element<'a, M> {
    fn from(w: Responsive<'a, M>) -> Self {
        Element::new(w)
    }
}
