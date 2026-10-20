//! Transições e animações: o `transition`, o `@starting-style` e o `@keyframes` do CSS.
//!
//! Tudo anda no desenho: o widget pede o valor de agora ([`DrawCx::transition`]) e, enquanto
//! algum não chegou ao fim, a janela desenha mais um quadro no ritmo do monitor. Parado, nada roda.
use crate::widget::{DrawCx, Element, EventCx, LayoutCx, Limits, Node, Widget};
use crate::{Background, Border, Captured, Color, Event, Length, MouseButton, Point, Rect, Shadow, States, Style};
use std::any::TypeId;
use std::time::{Duration, Instant};

/// A curva de velocidade, como no CSS.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Easing {
    Linear,
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// `cubic-bezier(x1, y1, x2, y2)`; `y` fora de 0..1 passa do alvo e volta.
    CubicBezier(f32, f32, f32, f32),
}

impl Easing {
    /// O progresso com a curva, para `t` de 0 a 1.
    pub fn apply(self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let (x1, y1, x2, y2) = match self {
            Easing::Linear => return t,
            Easing::Ease => (0.25, 0.1, 0.25, 1.0),
            Easing::EaseIn => (0.42, 0.0, 1.0, 1.0),
            Easing::EaseOut => (0.0, 0.0, 0.58, 1.0),
            Easing::EaseInOut => (0.42, 0.0, 0.58, 1.0),
            Easing::CubicBezier(a, b, c, d) => (a.clamp(0.0, 1.0), b, c.clamp(0.0, 1.0), d),
        };
        let curva = |p1: f32, p2: f32, s: f32| 3.0 * p1 * s * (1.0 - s) * (1.0 - s) + 3.0 * p2 * s * s * (1.0 - s) + s * s * s;
        // Com x1 e x2 entre 0 e 1, x(s) só cresce: a bisseção acha o s em que x(s) = t.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..24 {
            let s = (lo + hi) / 2.0;
            if curva(x1, x2, s) < t {
                lo = s;
            } else {
                hi = s;
            }
        }
        curva(y1, y2, (lo + hi) / 2.0)
    }
}

/// Quanto tempo uma mudança leva, com que curva e depois de quanta espera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}

impl Transition {
    pub fn new(ms: u64) -> Self {
        Transition { duration: Duration::from_millis(ms), easing: Easing::Ease, delay: Duration::ZERO }
    }
    pub fn easing(self, easing: Easing) -> Self {
        Transition { easing, ..self }
    }
    pub fn delay(self, ms: u64) -> Self {
        Transition { delay: Duration::from_millis(ms), ..self }
    }
    /// Quantas durações já passaram (sem a curva), descontada a espera.
    fn ciclos(&self, passou: Duration) -> f32 {
        if passou < self.delay {
            return 0.0;
        }
        if self.duration.is_zero() {
            return 1.0;
        }
        (passou - self.delay).as_secs_f32() / self.duration.as_secs_f32()
    }
}

/// Repetir sem parar (o `animation-iteration-count: infinite`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// Da entrada ao alvo, e recomeça.
    Forever,
    /// Vai e volta (o `animation-direction: alternate`).
    Alternate,
}

/// Um valor que sabe ir de um a outro: o que a transição consegue animar.
pub trait Lerp: Clone + PartialEq + 'static {
    /// `t` = 0 é `self`, 1 é `to`.
    fn lerp(&self, to: &Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        self + (to - self) * t
    }
}

impl Lerp for Point {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Point::new(self.x.lerp(&to.x, t), self.y.lerp(&to.y, t))
    }
}

impl Lerp for Color {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        // Do transparente para uma cor, só o alfa anda: senão o meio do caminho escurece.
        let a = if self.a == 0.0 { to.alpha(0.0) } else { *self };
        let b = if to.a == 0.0 { self.alpha(0.0) } else { *to };
        a.mix(b, t)
    }
}

impl Lerp for Border {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let mut radius = self.radius;
        for (r, d) in radius.iter_mut().zip(to.radius) {
            *r = r.lerp(&d, t);
        }
        Border { color: self.color.lerp(&to.color, t), width: self.width.lerp(&to.width, t), radius }
    }
}

impl Lerp for Shadow {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Shadow { color: self.color.lerp(&to.color, t), offset: self.offset.lerp(&to.offset, t), blur: self.blur.lerp(&to.blur, t) }
    }
}

impl Lerp for Background {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let paradas = |stops: &[(f32, Color)], c: Color, ida: bool| stops.iter().map(|(p, s)| (*p, if ida { c.lerp(s, t) } else { s.lerp(&c, t) })).collect();
        match (self, to) {
            (Background::Color(a), Background::Color(b)) => Background::Color(a.lerp(b, t)),
            (Background::Linear { angle: a1, stops: s1 }, Background::Linear { angle: a2, stops: s2 }) if s1.len() == s2.len() => {
                Background::Linear { angle: a1.lerp(a2, t), stops: s1.iter().zip(s2).map(|((p1, c1), (p2, c2))| (p1.lerp(p2, t), c1.lerp(c2, t))).collect() }
            }
            (Background::Color(c), Background::Linear { angle, stops }) => Background::Linear { angle: *angle, stops: paradas(stops, *c, true) },
            (Background::Linear { angle, stops }, Background::Color(c)) => Background::Linear { angle: *angle, stops: paradas(stops, *c, false) },
            // Degradês com número de paradas diferente: troca no meio.
            _ if t < 0.5 => self.clone(),
            _ => to.clone(),
        }
    }
}

impl<T: Lerp> Lerp for Option<T> {
    /// O lado que falta não anima (o texto sem cor própria herda a do pai): troca no fim.
    fn lerp(&self, to: &Self, t: f32) -> Self {
        match (self, to) {
            (Some(a), Some(b)) => Some(a.lerp(b, t)),
            _ => to.clone(),
        }
    }
}

impl Lerp for Style {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        if t >= 1.0 {
            return to.clone();
        }
        // Fundo e sombra que surgem ou somem vêm do transparente.
        let background = match (&self.background, &to.background) {
            (Some(a), None) => Some(a.lerp(&a.faded(0.0), t)),
            (None, Some(b)) => Some(b.faded(0.0).lerp(b, t)),
            (a, b) => a.lerp(b, t),
        };
        let shadow = match (self.shadow, to.shadow) {
            (Some(a), None) => Some(a.lerp(&Shadow { color: a.color.alpha(0.0), ..a }, t)),
            (None, Some(b)) => Some(Shadow { color: b.color.alpha(0.0), ..b }.lerp(&b, t)),
            (a, b) => a.lerp(&b, t),
        };
        Style {
            background,
            border: self.border.lerp(&to.border, t),
            shadow,
            text_color: self.text_color.lerp(&to.text_color, t),
            backdrop_blur: self.backdrop_blur.lerp(&to.backdrop_blur, t),
            transition: to.transition,
        }
    }
}

struct Estado<T> {
    de: T,
    para: T,
    inicio: Instant,
    /// O último quadro que desenhou este valor.
    visto: Option<u64>,
    agora: T,
}

/// O valor de agora a caminho de `alvo`. Quando o widget entra na tela parte de `entrada`
/// (ou já nasce no alvo); quando o alvo muda, parte de onde está.
pub(crate) fn animar<T: Lerp>(states: &States, key: u64, alvo: &T, entrada: Option<&T>, tr: Transition, repetir: Option<Repeat>) -> T {
    let Some(agora) = states.agora else { return alvo.clone() };
    let mut mapa = states.animacoes.borrow_mut();
    let caixa = mapa.entry((key, TypeId::of::<T>())).or_insert_with(|| Box::new(Estado { de: alvo.clone(), para: alvo.clone(), inicio: agora, visto: None, agora: alvo.clone() }));
    let e = caixa.downcast_mut::<Estado<T>>().expect("a chave inclui o tipo");
    // Um quadro inteiro sem ser desenhado (o menu fechou e abriu de novo) conta como entrar.
    if e.visto.is_none_or(|v| v + 1 < states.quadro) {
        let de = entrada.unwrap_or(alvo).clone();
        *e = Estado { de: de.clone(), para: alvo.clone(), inicio: agora, visto: None, agora: de };
    } else if e.para != *alvo {
        e.de = e.agora.clone();
        e.para = alvo.clone();
        e.inicio = agora;
    }
    e.visto = Some(states.quadro);
    let c = tr.ciclos(agora.saturating_duration_since(e.inicio));
    let (f, fim) = match repetir {
        None => (c.min(1.0), c >= 1.0),
        Some(r) => {
            let volta = r == Repeat::Alternate && c.floor() as u64 % 2 == 1;
            (if volta { 1.0 - c.fract() } else { c.fract() }, false)
        }
    };
    if !fim && e.de != e.para {
        states.animando.set(true);
    }
    e.agora = if fim { e.para.clone() } else { e.de.lerp(&e.para, tr.easing.apply(f)) };
    e.agora.clone()
}

/// O último valor desenhado (o evento precisa saber onde o widget está agora).
fn ultimo<T: Lerp>(states: &States, key: u64) -> Option<T> {
    states.animacoes.borrow().get(&(key, TypeId::of::<T>()))?.downcast_ref::<Estado<T>>().map(|e| e.agora.clone())
}

/// Como um widget aparece num instante da animação.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub opacity: f32,
    /// Em pixels lógicos, sem mexer no layout (o `transform: translate` do CSS).
    pub offset: Point,
    /// Em volta do centro (o `scale`); 1 = o tamanho do layout.
    pub scale: f32,
    /// Graus no sentido horário, em volta do centro (o `rotate`).
    pub rotate: f32,
}

impl Default for Keyframe {
    fn default() -> Self {
        Keyframe { opacity: 1.0, offset: Point::ORIGIN, scale: 1.0, rotate: 0.0 }
    }
}

impl Keyframe {
    pub fn opacity(self, opacity: f32) -> Self {
        Keyframe { opacity, ..self }
    }
    pub fn offset(self, x: f32, y: f32) -> Self {
        Keyframe { offset: Point::new(x, y), ..self }
    }
    pub fn scale(self, scale: f32) -> Self {
        Keyframe { scale, ..self }
    }
    pub fn rotate(self, graus: f32) -> Self {
        Keyframe { rotate: graus, ..self }
    }
}

impl Lerp for Keyframe {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Keyframe { opacity: self.opacity.lerp(&to.opacity, t), offset: self.offset.lerp(&to.offset, t), scale: self.scale.lerp(&to.scale, t), rotate: self.rotate.lerp(&to.rotate, t) }
    }
}

#[derive(Default)]
struct Pressionado(bool);

pub struct Animate<'a, M> {
    content: Element<'a, M>,
    alvo: Keyframe,
    entrada: Option<Keyframe>,
    hover: Option<Keyframe>,
    pressed: Option<Keyframe>,
    saida: Option<Keyframe>,
    mostrar: bool,
    transition: Transition,
    repetir: Option<Repeat>,
}

/// Anima a opacidade, a posição, a escala e a rotação de `content`. Mude o alvo (`.opacity()`,
/// `.offset()`, `.scale()`, `.rotate()`) e ele vai até lá com a `.transition()`; `.hover()` e
/// `.pressed()` trocam o alvo com o mouse; `.enter()` e `.show(false)` animam a entrada e a saída;
/// `.repeat()` faz o laço.
pub fn animate<'a, M>(content: impl Into<Element<'a, M>>) -> Animate<'a, M> {
    Animate { content: content.into(), alvo: Keyframe::default(), entrada: None, hover: None, pressed: None, saida: None, mostrar: true, transition: Transition::new(250), repetir: None }
}

impl<M> Animate<'_, M> {
    pub fn opacity(mut self, o: f32) -> Self {
        self.alvo.opacity = o;
        self
    }
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.alvo.offset = Point::new(x, y);
        self
    }
    pub fn scale(mut self, k: f32) -> Self {
        self.alvo.scale = k;
        self
    }
    pub fn rotate(mut self, graus: f32) -> Self {
        self.alvo.rotate = graus;
        self
    }
    pub fn transition(mut self, t: Transition) -> Self {
        self.transition = t;
        self
    }
    /// Como ele está ao entrar na tela; dali anima até o alvo (o `@starting-style` do CSS).
    pub fn enter(mut self, k: Keyframe) -> Self {
        self.entrada = Some(k);
        self
    }
    /// O alvo com o mouse em cima (o `:hover`): cartão que cresce, ícone que sobe.
    pub fn hover(mut self, k: Keyframe) -> Self {
        self.hover = Some(k);
        self
    }
    /// O alvo enquanto o botão do mouse está apertado em cima (o `:active`).
    pub fn pressed(mut self, k: Keyframe) -> Self {
        self.pressed = Some(k);
        self
    }
    /// `false` anima até a saída ([`exit`](Self::exit); o padrão só some) e então para de
    /// desenhar e de receber eventos. O lugar no layout continua ocupado: numa pilha não se nota.
    pub fn show(mut self, mostrar: bool) -> Self {
        self.mostrar = mostrar;
        self
    }
    /// Como ele sai com `.show(false)`.
    pub fn exit(mut self, k: Keyframe) -> Self {
        self.saida = Some(k);
        self
    }
    /// Repete da entrada (ou do normal) até o alvo, sem parar: piscar, pulsar, girar.
    pub fn repeat(mut self, r: Repeat) -> Self {
        self.repetir = Some(r);
        self
    }
    fn alvo(&self, n: &Node, cx: &DrawCx) -> Keyframe {
        if !self.mostrar {
            return self.saida.unwrap_or(Keyframe { opacity: 0.0, ..self.alvo });
        }
        let apertado = cx.states.get::<Pressionado>(n.key).is_some_and(|p| p.0);
        match (self.pressed, self.hover) {
            (Some(k), _) if apertado => k,
            (_, Some(k)) if cx.hovered(&n.rect) => k,
            _ => self.alvo,
        }
    }
}

impl<M> Widget<M> for Animate<'_, M> {
    fn size(&self) -> (Length, Length) {
        self.content.widget().size()
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        let n = cx.child(0, &self.content, l);
        Node { rect: n.rect, key: cx.key(), children: vec![n], ..Default::default() }
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let alvo = self.alvo(n, cx);
        // O laço sem entrada própria parte do normal; escondido já nasce escondido.
        let entrada = if self.mostrar { self.entrada.or(self.repetir.map(|_| Keyframe::default())) } else { None };
        let k = animar(cx.states, n.key, &alvo, entrada.as_ref(), self.transition, self.repetir);
        // Escondido e já no fim da saída: não desenha mais.
        if k.opacity <= 0.0 || (!self.mostrar && k == alvo) {
            return;
        }
        let movido;
        let filho = if k.offset == Point::ORIGIN {
            &n.children[0]
        } else {
            let mut c = n.children[0].clone();
            c.translate(k.offset.x, k.offset.y);
            movido = c;
            &movido
        };
        if k.scale != 1.0 || k.rotate != 0.0 {
            // Escala e rotação precisam do grupo inteiro numa camada; a folga em volta é a da sombra.
            let r = filho.rect;
            let area = Rect { x: r.x - 32.0, y: r.y - 32.0, width: r.width + 64.0, height: r.height + 64.0 };
            cx.renderer.push_layer(area, k.scale, k.rotate, k.opacity);
            self.content.widget().draw(filho, cx);
            cx.renderer.pop_layer();
        } else {
            cx.renderer.push_opacity(k.opacity);
            self.content.widget().draw(filho, cx);
            cx.renderer.pop_opacity();
        }
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        if !self.mostrar {
            return Captured::No;
        }
        if self.pressed.is_some() {
            let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
            match ev {
                Event::MousePressed(MouseButton::Left) if dentro => {
                    cx.states.get_mut::<Pressionado>(n.key).0 = true;
                    cx.redraw = true;
                }
                Event::MouseReleased(MouseButton::Left) if std::mem::take(&mut cx.states.get_mut::<Pressionado>(n.key).0) => cx.redraw = true,
                _ => {}
            }
        }
        // O clique vale onde o widget aparece, não onde o layout o pôs.
        // shortcut: a escala e a rotação não entram no clique; só o deslocamento.
        match ultimo::<Keyframe>(cx.states, n.key).map(|k| k.offset).filter(|o| *o != Point::ORIGIN) {
            Some(o) => {
                let mut c = n.children[0].clone();
                c.translate(o.x, o.y);
                self.content.widget().event(&c, ev, cx)
            }
            None => self.content.widget().event(&n.children[0], ev, cx),
        }
    }
    // shortcut: a camada opaca de baixo de um deslocado ainda olha a posição do layout; mover junto se um menu animado reclamar.
    fn opaque_at(&self, n: &Node, p: Point) -> bool {
        self.mostrar && self.content.widget().opaque_at(&n.children[0], p)
    }
}

impl<'a, M: 'a> From<Animate<'a, M>> for Element<'a, M> {
    fn from(a: Animate<'a, M>) -> Self {
        Element::new(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curvas_vao_de_zero_a_um() {
        for e in [Easing::Linear, Easing::Ease, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut] {
            assert_eq!(e.apply(0.0), 0.0);
            assert_eq!(e.apply(1.0), 1.0);
            let mut antes = 0.0;
            for i in 1..10 {
                let v = e.apply(i as f32 / 10.0);
                assert!(v >= antes, "{e:?} voltou em {i}");
                antes = v;
            }
        }
        // ease-in começa devagar, ease-out começa rápido.
        assert!(Easing::EaseIn.apply(0.25) < 0.25 && Easing::EaseOut.apply(0.25) > 0.25);
        // O ease do CSS no meio do tempo já passou de 80%.
        assert!((Easing::Ease.apply(0.5) - 0.8024).abs() < 0.01, "{}", Easing::Ease.apply(0.5));
    }

    #[test]
    fn transicao_anda_com_o_relogio_e_entra_de_novo() {
        let mut st = States::default();
        let t0 = Instant::now();
        let tr = Transition::new(100).easing(Easing::Linear);
        let em = |ms: u64| t0 + Duration::from_millis(ms);
        // Sem relógio, já está no alvo.
        assert_eq!(animar(&st, 1, &1.0f32, Some(&0.0), tr, None), 1.0);
        st.tick(em(0));
        assert_eq!(animar(&st, 1, &1.0f32, Some(&0.0), tr, None), 0.0);
        assert!(st.animating());
        st.tick(em(50));
        assert!((animar(&st, 1, &1.0f32, Some(&0.0), tr, None) - 0.5).abs() < 1e-3);
        // O alvo muda no meio: parte de onde estava.
        st.tick(em(60));
        assert!((animar(&st, 1, &0.0f32, Some(&0.0), tr, None) - 0.5).abs() < 1e-3);
        st.tick(em(160));
        assert_eq!(animar(&st, 1, &0.0f32, Some(&0.0), tr, None), 0.0);
        assert!(!st.animating());
        // Um quadro sem ele: volta a entrar.
        st.tick(em(200));
        st.tick(em(210));
        assert_eq!(animar(&st, 1, &1.0f32, Some(&0.0), tr, None), 0.0);
    }

    #[test]
    fn repetir_vai_e_volta() {
        let mut st = States::default();
        let t0 = Instant::now();
        let tr = Transition::new(100).easing(Easing::Linear);
        let v = |st: &mut States, ms: u64| {
            st.tick(t0 + Duration::from_millis(ms));
            animar(st, 1, &1.0f32, Some(&0.0), tr, Some(Repeat::Alternate))
        };
        assert_eq!(v(&mut st, 0), 0.0);
        assert!((v(&mut st, 150) - 0.5).abs() < 1e-3);
        assert!((v(&mut st, 175) - 0.25).abs() < 1e-3);
        assert!(st.animating());
    }

    #[test]
    fn estilo_surge_do_transparente() {
        let a = Style::default();
        let b = Style::default().background(Color::rgb(1.0, 0.0, 0.0));
        let meio = a.lerp(&b, 0.5);
        assert_eq!(meio.background, Some(Background::Color(Color::rgba(1.0, 0.0, 0.0, 0.5))));
        assert_eq!(a.lerp(&b, 1.0), b);
    }
}
