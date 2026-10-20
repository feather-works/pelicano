//! Controles prontos: checkbox, toggler, dropdown, slider e régua.
use crate::access::accesskit::{Action, Role};
use crate::anim::{Lerp, Transition};
use crate::widget::{no, surgir, text_input, TextInput, ABRIR, CursorIcon, DrawCx, Element, EventCx, LayoutCx, Limits, Node, Num, Widget};
use crate::{Background, Border, Captured, Color, Event, Font, Key, Length, MouseButton, NamedKey, Padding, Point, Rect, Renderer, Shadow, Size, Status, Theme};
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
        let (papel, rotulo, marcado, desabilitado) = (match self.forma {
            Forma::Caixa => Role::CheckBox,
            Forma::Chave => Role::Switch,
            Forma::Radio => Role::RadioButton,
        }, &self.label, self.checked, self.on_toggle.is_none());
        cx.access(n.key, n.rect, || {
            let mut a = no(papel, &[Action::Click]);
            a.set_label(&**rotulo);
            a.set_toggled(marcado.into());
            if desabilitado {
                a.set_disabled();
            }
            a
        });
        match self.forma {
            Forma::Caixa => {
                // A caixa enche de cor e o visto aparece junto.
                let p = cx.transition(n.key, if self.checked { 1.0f32 } else { 0.0 }, Transition::new(140));
                let vazia = if hover { t.muted } else { t.border };
                let (bg, borda) = (t.surface.lerp(&ativa, p), vazia.lerp(&ativa, p));
                cx.renderer.fill_quad(r, Some(&bg.alpha(apagado).into()), Border::rounded(4.0).color(borda.alpha(apagado), 1.0), None);
                if p > 0.0 {
                    let q = |x: f32, y: f32| Point::new(r.x + x, r.y + y);
                    cx.renderer.stroke(&[q(4.0, 8.5), q(7.0, 11.5), q(12.0, 5.0)], 2.0, t.on_primary.alpha(p));
                }
            }
            Forma::Radio => {
                // O anel engrossa até virar o ponto.
                let p = cx.transition(n.key, if self.checked { 1.0f32 } else { 0.0 }, Transition::new(160));
                let vazia = if hover { t.muted } else { t.border };
                cx.renderer.fill_quad(r, Some(&t.surface.alpha(apagado).into()), Border::rounded(8.0).color(vazia.lerp(&ativa, p).alpha(apagado), 1.0 + 4.0 * p), None);
            }
            Forma::Chave => {
                // A bolinha desliza até o outro lado em vez de pular.
                let p = cx.transition(n.key, if self.checked { 1.0f32 } else { 0.0 }, Transition::new(160));
                let trilho = t.text.alpha(0.2).lerp(&ativa, p);
                cx.renderer.fill_quad(r, Some(&trilho.alpha(trilho.a * apagado).into()), Border::rounded(m.height / 2.0), None);
                let d = m.height - 4.0;
                let x = r.x + 2.0 + (m.width - d - 4.0) * p;
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

/// Se o menu está aberto e quanto ele rolou.
#[derive(Default)]
struct Aberto(bool, f32);

/// Lista suspensa: mostra a opção escolhida e abre as outras embaixo.
pub struct PickList<'a, T, M> {
    options: &'a [T],
    selected: Option<T>,
    on_select: Box<dyn Fn(T) -> M + 'a>,
    placeholder: Cow<'a, str>,
    width: Length,
    size: f32,
    menu_height: f32,
}

pub fn pick_list<'a, T: Clone + PartialEq + ToString, M>(options: &'a [T], selected: Option<T>, on_select: impl Fn(T) -> M + 'a) -> PickList<'a, T, M> {
    PickList { options, selected, on_select: Box::new(on_select), placeholder: "".into(), width: Length::Shrink, size: 14.0, menu_height: f32::INFINITY }
}

const ITEM: f32 = 30.0;

/// O menu solto de `itens` embaixo (ou em cima) do campo, até `max` de altura; devolve também quanto dá para rolar.
fn menu(campo: Rect, itens: usize, max: f32, janela: f32) -> (Rect, f32) {
    let total = itens as f32 * ITEM + 8.0;
    let altura = total.min(max.max(ITEM + 8.0));
    (Rect { x: campo.x, y: crate::pickers::painel_y(campo, altura, janela), width: campo.width, height: altura }, total - altura)
}

fn item_em(menu: Rect, rolagem: f32, p: Point) -> usize {
    ((p.y - menu.y - 4.0 + rolagem) / ITEM).floor().max(0.0) as usize
}

/// A rolagem que deixa o item `i` à vista.
fn mostrar_item(menu: Rect, sobra: f32, rolagem: f32, i: usize) -> f32 {
    let topo = i as f32 * ITEM;
    rolagem.min(topo).max(topo + ITEM - (menu.height - 8.0)).clamp(0.0, sobra)
}

/// Desenha o menu: `escolhido` na cor primária; `destaque` (teclado) e o item sob o mouse, mais claros.
#[allow(clippy::too_many_arguments)]
fn desenhar_menu(rd: &mut Renderer, t: &Theme, menu: Rect, sobra: f32, rolagem: f32, itens: &[String], escolhido: Option<usize>, destaque: Option<usize>, cursor: Option<Point>, size: f32) {
    rd.fill_quad(menu, Some(&t.surface.into()), Border::rounded(8.0).color(t.border, 1.0), Some(Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 6.0), blur: 14.0 }));
    let dentro = Rect { x: menu.x, y: menu.y + 4.0, width: menu.width, height: menu.height - 8.0 };
    let cursor = cursor.filter(|p| dentro.contains(*p));
    rd.push_clip(dentro, [0.0; 4]);
    for (i, s) in itens.iter().enumerate() {
        let r = Rect { x: menu.x + 4.0, y: dentro.y + i as f32 * ITEM - rolagem, width: menu.width - 8.0, height: ITEM };
        if r.y + ITEM < dentro.y || r.y > dentro.y + dentro.height {
            continue;
        }
        let marcado = escolhido == Some(i);
        if marcado || destaque == Some(i) || cursor.is_some_and(|p| r.contains(p)) {
            let bg = if marcado { t.primary.alpha(0.22) } else { t.text.alpha(0.08) };
            rd.fill_quad(r, Some(&bg.into()), Border::rounded(5.0), None);
        }
        let th = rd.measure(s, size, Font::DEFAULT, None).height;
        rd.draw_text(s, size, Font::DEFAULT, t.text, Point::new(r.x + 8.0, r.y + (ITEM - th) / 2.0), None);
    }
    rd.pop_clip();
    if sobra > 0.0 {
        // A barra só indica onde se está; rolar é com a roda ou as setas.
        let h = (dentro.height * dentro.height / (dentro.height + sobra)).max(16.0);
        let y = dentro.y + (dentro.height - h) * rolagem / sobra;
        rd.fill_quad(Rect { x: menu.x + menu.width - 6.0, y, width: 3.0, height: h }, Some(&t.text.alpha(0.25).into()), Border::rounded(1.5), None);
    }
}

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
    /// A altura máxima do menu aberto; passando dela, ele rola.
    pub fn menu_height(mut self, h: impl Into<Num>) -> Self {
        self.menu_height = h.into().0;
        self
    }
    fn menu(&self, campo: Rect, janela: f32) -> (Rect, f32) {
        menu(campo, self.options.len(), self.menu_height, janela)
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
        cx.access(n.key, n.rect, || {
            let mut a = no(Role::ComboBox, &[Action::Click]);
            a.set_value(&*rotulo);
            a.set_expanded(aberto);
            a
        });
        let th = cx.renderer.measure(&rotulo, self.size, Font::DEFAULT, None).height;
        let y = n.rect.y + (n.rect.height - th) / 2.0;
        cx.renderer.push_clip(Rect { width: n.rect.width - 28.0, ..n.rect }, [0.0; 4]);
        cx.renderer.draw_text(&rotulo, self.size, Font::DEFAULT, cor, Point::new(n.rect.x + 10.0, y), None);
        cx.renderer.pop_clip();
        let (ax, ay) = (n.rect.x + n.rect.width - 18.0, n.rect.y + n.rect.height / 2.0);
        let seta = if aberto { [Point::new(ax - 4.0, ay + 2.0), Point::new(ax, ay - 2.0), Point::new(ax + 4.0, ay + 2.0)] } else { [Point::new(ax - 4.0, ay - 2.0), Point::new(ax, ay + 2.0), Point::new(ax + 4.0, ay - 2.0)] };
        cx.renderer.stroke(&seta, 1.5, t.muted);
        // O menu surge ao abrir e se apaga ao fechar.
        let p = cx.transition(n.key, if aberto { 1.0f32 } else { 0.0 }, ABRIR);
        if p <= 0.0 {
            return;
        }
        let (menu, sobra) = self.menu(n.rect, cx.renderer.window_size().height);
        let rolagem = cx.states.get::<Aberto>(n.key).map_or(0.0, |a| a.1).min(sobra);
        let itens: Vec<String> = self.options.iter().map(|o| o.to_string()).collect();
        let escolhido = self.options.iter().position(|o| self.selected.as_ref() == Some(o));
        if aberto {
            cx.popup(menu);
        }
        let (cursor, size) = (cx.popup_cursor().filter(|_| aberto), self.size);
        cx.overlay(move |rd| surgir(rd, menu, p, |rd| desenhar_menu(rd, &t, menu, sobra, rolagem, &itens, escolhido, None, cursor, size)));
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let aberto = cx.states.get::<Aberto>(n.key).is_some_and(|a| a.0);
        let p = cx.popup_cursor();
        match ev {
            Event::MousePressed(MouseButton::Left) if aberto => {
                let a = cx.states.get_mut::<Aberto>(n.key);
                a.0 = false;
                let rolagem = std::mem::take(&mut a.1);
                cx.redraw = true;
                let (menu, sobra) = self.menu(n.rect, cx.renderer.window_size().height);
                if let Some(p) = p.filter(|p| menu.contains(*p)) {
                    let i = item_em(menu, rolagem.min(sobra), p);
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
            Event::Wheel { dy, .. } if aberto => {
                let (menu, sobra) = self.menu(n.rect, cx.renderer.window_size().height);
                if !p.is_some_and(|p| menu.contains(p)) {
                    return Captured::No;
                }
                let a = cx.states.get_mut::<Aberto>(n.key);
                a.1 = (a.1 - dy).clamp(0.0, sobra);
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

// ---------- Busca com lista ----------

/// O texto digitado, se o menu está aberto, o item do teclado e a rolagem.
#[derive(Default)]
struct Busca {
    texto: String,
    aberto: bool,
    destaque: usize,
    rolagem: f32,
}

/// Campo que filtra as opções enquanto se digita e mostra as que sobram numa lista embaixo.
/// Sem foco, mostra a escolhida.
pub struct ComboBox<'a, T, M> {
    options: &'a [T],
    placeholder: Cow<'a, str>,
    selected: Option<&'a T>,
    on_select: Box<dyn Fn(T) -> M + 'a>,
    width: Length,
    size: f32,
    padding: Padding,
    menu_height: f32,
}

pub fn combo_box<'a, T: Clone + ToString, M>(options: &'a [T], placeholder: impl Into<Cow<'a, str>>, selected: Option<&'a T>, on_select: impl Fn(T) -> M + 'a) -> ComboBox<'a, T, M> {
    ComboBox { options, placeholder: placeholder.into(), selected, on_select: Box::new(on_select), width: Length::FILL, size: 14.0, padding: [6u16, 10].into(), menu_height: f32::INFINITY }
}

/// Minúsculas e sem acento, para comparar.
// shortcut: só os acentos latinos comuns; uma tabela Unicode (NFD) se aparecerem outras línguas.
fn dobrar(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

impl<'a, T: Clone + ToString, M> ComboBox<'a, T, M> {
    pub fn width(mut self, w: impl Into<Length>) -> Self {
        self.width = w.into();
        self
    }
    pub fn size(mut self, s: impl Into<Num>) -> Self {
        self.size = s.into().0;
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = p.into();
        self
    }
    /// A altura máxima da lista aberta; passando dela, ela rola.
    pub fn menu_height(mut self, h: impl Into<Num>) -> Self {
        self.menu_height = h.into().0;
        self
    }
    /// As opções que contêm `busca` (índice e texto).
    fn filtradas(&self, busca: &str) -> Vec<(usize, String)> {
        let b = dobrar(busca);
        self.options.iter().map(|o| o.to_string()).enumerate().filter(|(_, s)| dobrar(s).contains(&b)).collect()
    }
    /// O campo de texto de dentro: digitando, mostra a busca; parado, a opção escolhida.
    fn campo(&self, editando: bool, busca: &str) -> TextInput<'_, String> {
        let escolhida = self.selected.map(|s| s.to_string()).unwrap_or_default();
        let (dica, valor) = match (editando, escolhida.is_empty()) {
            (true, false) => (Cow::Owned(escolhida), busca.to_string()),
            (true, true) => (self.placeholder.clone(), busca.to_string()),
            (false, _) => (self.placeholder.clone(), escolhida),
        };
        text_input(dica, valor).on_input(|s| s).size(self.size).padding(self.padding).width(self.width)
    }
    fn menu(&self, n: &Node, itens: usize, janela: f32) -> (Rect, f32) {
        menu(n.rect, itens, self.menu_height, janela)
    }
    /// Passa o evento ao campo de dentro; devolve o texto novo, se ele mudou.
    fn repassar(&self, campo: &TextInput<'_, String>, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> (Captured, Option<String>) {
        let mut saida = Vec::new();
        let mut sub = EventCx { renderer: &mut *cx.renderer, states: &mut *cx.states, cursor: cx.cursor, messages: &mut saida, window: &mut *cx.window, redraw: false, clipboard: &mut *cx.clipboard };
        let c = campo.event(&n.children[0], ev, &mut sub);
        cx.redraw |= sub.redraw;
        (c, saida.pop())
    }
    fn escolher(&self, i: Option<usize>, cx: &mut EventCx<M>, key: u64) {
        *cx.states.get_mut::<Busca>(key) = Busca::default();
        cx.redraw = true;
        if let Some(o) = i.and_then(|i| self.options.get(i)) {
            cx.publish((self.on_select)(o.clone()));
        }
    }
}

impl<'a, T: Clone + ToString + 'a, M> Widget<M> for ComboBox<'a, T, M> {
    fn size(&self) -> (Length, Length) {
        (self.width, Length::Shrink)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let mut n = self.campo(false, "").layout(cx, l);
        // O campo de dentro tem estado (cursor, foco) numa chave própria.
        n.children.push(Node { key: n.key ^ 0x9e37_79b9_7f4a_7c15, ..n.clone() });
        n
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let t = *cx.theme;
        let editando = cx.states.focus == Some(n.children[0].key);
        let (busca, aberto, destaque, rolagem) = cx.states.get::<Busca>(n.key).map_or(("", false, 0, 0.0), |b| (b.texto.as_str(), b.aberto && editando, b.destaque, b.rolagem));
        let rotulo = self.selected.map(|s| s.to_string()).unwrap_or_default();
        // Registrado antes do campo de dentro, com a chave dele: o leitor vê uma caixa de combinação só.
        cx.access(n.children[0].key, n.rect, || {
            let mut a = no(Role::ComboBox, &[Action::Click, Action::Focus]);
            a.set_value(&*rotulo);
            a.set_expanded(aberto);
            a
        });
        self.campo(editando, busca).draw(&n.children[0], cx);
        let p = cx.transition(n.key, if aberto { 1.0f32 } else { 0.0 }, ABRIR);
        if p <= 0.0 {
            return;
        }
        let filtradas = self.filtradas(busca);
        if filtradas.is_empty() {
            return;
        }
        let (menu, sobra) = self.menu(n, filtradas.len(), cx.renderer.window_size().height);
        let escolhido = filtradas.iter().position(|(_, s)| *s == rotulo);
        let itens: Vec<String> = filtradas.into_iter().map(|(_, s)| s).collect();
        if aberto {
            cx.popup(menu);
        }
        let (cursor, size, rolagem) = (cx.popup_cursor().filter(|_| aberto), self.size, rolagem.min(sobra));
        cx.overlay(move |rd| surgir(rd, menu, p, |rd| desenhar_menu(rd, &t, menu, sobra, rolagem, &itens, escolhido, Some(destaque), cursor, size)));
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let editando = cx.states.focus == Some(n.children[0].key);
        let b = cx.states.get::<Busca>(n.key);
        // O foco foi para outro lugar sem clique (o clique o tira de todos antes de chegar aqui).
        if b.is_some_and(|b| b.aberto) && !editando && !matches!(ev, Event::MousePressed(_)) {
            cx.states.get_mut::<Busca>(n.key).aberto = false;
        }
        let (busca, aberto, destaque, rolagem) = cx.states.get::<Busca>(n.key).map_or((String::new(), false, 0, 0.0), |b| (b.texto.clone(), b.aberto, b.destaque, b.rolagem));
        // Fechada, só as setas (que abrem) precisam da lista.
        let setas = matches!(ev, Event::Key { key: Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp), .. });
        let filtradas = if aberto || setas { self.filtradas(&busca) } else { Vec::new() };
        let (menu, sobra) = self.menu(n, filtradas.len(), cx.renderer.window_size().height);
        let p = cx.popup_cursor();
        let no_campo = p.is_some_and(|p| n.rect.contains(p));
        match ev {
            Event::MousePressed(MouseButton::Left) if aberto && !filtradas.is_empty() && p.is_some_and(|p| menu.contains(p)) => {
                let i = item_em(menu, rolagem.min(sobra), p.unwrap_or_default());
                self.escolher(filtradas.get(i).map(|f| f.0), cx, n.key);
                Captured::Yes
            }
            Event::MousePressed(MouseButton::Left) if no_campo => {
                if !aberto {
                    *cx.states.get_mut::<Busca>(n.key) = Busca { aberto: true, ..Busca::default() };
                }
                cx.redraw = true;
                self.repassar(&self.campo(true, &busca), n, ev, cx).0
            }
            Event::MousePressed(_) if aberto => {
                self.escolher(None, cx, n.key);
                Captured::No
            }
            Event::Wheel { dy, .. } if aberto && p.is_some_and(|p| menu.contains(p)) => {
                cx.states.get_mut::<Busca>(n.key).rolagem = (rolagem.min(sobra) - dy).clamp(0.0, sobra);
                cx.redraw = true;
                Captured::Yes
            }
            Event::Key { key, pressed: true, .. } if editando => {
                let mover = |d: isize| (destaque as isize + d).clamp(0, (filtradas.len() as isize - 1).max(0)) as usize;
                match key {
                    Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp) => {
                        let i = if !aberto { 0 } else { mover(if *key == Key::Named(NamedKey::ArrowDown) { 1 } else { -1 }) };
                        *cx.states.get_mut::<Busca>(n.key) = Busca { texto: busca, aberto: true, destaque: i, rolagem: mostrar_item(menu, sobra, rolagem, i) };
                    }
                    Key::Named(NamedKey::Enter) => {
                        if aberto {
                            self.escolher(filtradas.get(destaque).map(|f| f.0), cx, n.key);
                            cx.states.focus = None;
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.escolher(None, cx, n.key);
                        cx.states.focus = None;
                    }
                    _ => {
                        let (c, novo) = self.repassar(&self.campo(true, &busca), n, ev, cx);
                        if let Some(s) = novo {
                            *cx.states.get_mut::<Busca>(n.key) = Busca { texto: s, aberto: true, ..Busca::default() };
                        }
                        return c;
                    }
                }
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
        cx.access(n.key, n.rect, || {
            let mut s = no(Role::Slider, &[]);
            s.set_numeric_value(self.value as f64);
            s.set_min_numeric_value(a as f64);
            s.set_max_numeric_value(b as f64);
            if self.step > 0.0 {
                s.set_numeric_value_step(self.step as f64);
            }
            s
        });
        let ativo = cx.hovered(&n.rect) || cx.states.get::<Arrastando>(n.key).is_some_and(|d| d.0);
        if ativo {
            cx.cursor_icon = CursorIcon::Pointer;
        }
        let d = cx.transition(n.key, if ativo { 16.0f32 } else { 14.0 }, Transition::new(120));
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
impl<'a, T: Clone + ToString + 'a, M: 'a> From<ComboBox<'a, T, M>> for Element<'a, M> {
    fn from(w: ComboBox<'a, T, M>) -> Self {
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
            crate::Style::default().text_color(cor).background(bg).border(Border { radius: [6.0, 6.0, 0.0, 0.0], ..Border::default() }).transition(Transition::new(160))
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
