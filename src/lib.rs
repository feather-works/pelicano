//! Pelicano: a biblioteca de interface da suíte FeatherWorks.
//!
//! Feita para não travar onde o Iced travou na suíte:
//! - janela sem moldura que redimensiona e arrasta de verdade ([`WindowCmd`]);
//! - imagem, gradiente e qualquer filho recortados pelo raio do contêiner;
//! - raio por canto, sombra e borda em todo contêiner;
//! - o quadro é desenhado na CPU (tiny-skia): o mesmo código gera a janela e o
//!   PNG de um teste ou captura ([`capture`]), sem GPU nem tela;
//! - vidro em qualquer caixa (`Style::backdrop_blur`, [`Style::glass`]) e janela
//!   translúcida com desfoque do desktop atrás ([`Settings`]);
//! - o trait [`Widget`] é pequeno e público, e todo estilo é uma closure que
//!   recebe o [`Status`] e o [`Theme`] — nada de tema fechado num enum.
mod access;
mod anim;
#[cfg(feature = "highlighter")]
mod highlight;
mod app;
mod controls;
mod editor;
mod pickers;
mod render;
mod markdown;
mod rich;
#[cfg(feature = "terminal")]
mod terminal;
pub mod widget;

pub use access::{accesskit, Access};
#[cfg(feature = "highlighter")]
pub use highlight::Highlight;
pub use anim::{animate, Animate, Easing, Keyframe, Lerp, Repeat, Transition};
pub use app::{run, App, Error, Handle, Settings, Subscription, Task, WindowCmd, WindowId};
/// Canais e fluxos para as tarefas ([`Task::run`]).
pub use futures;
/// A alça crua da janela ([`Task::window_handle`]).
pub use winit::raw_window_handle;
pub use render::vetor::{FillRule, LineCap, LineJoin, Path, PathBuilder, Segment, Stroke, Transform};
pub use render::{load_font, Changes, Font, Image, Renderer, Svg};
#[doc(hidden)]
pub use render::gpu::GpuOffscreen;
pub use controls::*;
pub use editor::{is_invisible, text_editor, Content, TextEditor};
pub use pickers::*;
pub use markdown::markdown;
#[cfg(feature = "terminal")]
pub use terminal::{terminal, Terminal, TerminalEvent, TerminalSettings, TerminalView};
pub use rich::{rich_text, span, RichText, Span};
pub use widget::*;
pub use winit::keyboard::{Key, ModifiersState as Modifiers, NamedKey};
pub use winit::window::{Icon, ResizeDirection, WindowLevel};

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ORIGIN: Point = Point { x: 0.0, y: 0.0 };
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0.0, height: 0.0 };
    pub const INFINITE: Size = Size { width: f32::INFINITY, height: f32::INFINITY };
    pub const fn new(width: f32, height: f32) -> Self {
        Size { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(p: Point, s: Size) -> Self {
        Rect { x: p.x, y: p.y, width: s.width, height: s.height }
    }
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.x + self.width && p.y >= self.y && p.y < self.y + self.height
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let w = ((self.x + self.width).min(o.x + o.width) - x).max(0.0);
        let h = ((self.y + self.height).min(o.y + o.height) - y).max(0.0);
        Rect { x, y, width: w, height: h }
    }
    pub fn shrink(&self, p: Padding) -> Rect {
        Rect { x: self.x + p.left, y: self.y + p.top, width: (self.width - p.left - p.right).max(0.0), height: (self.height - p.top - p.bottom).max(0.0) }
    }
}

/// Cor em sRGB, componentes de 0 a 1, alfa não pré-multiplicado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Color { r, g, b, a: 1.0 }
    }
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Color { r, g, b, a }
    }
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Color::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }
    /// `#rrggbb` ou `#rrggbbaa`; `None` se não for hexadecimal válido.
    pub fn hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        let byte = |i: usize| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
        let c = Color::rgb8(byte(0)?, byte(2)?, byte(4)?);
        match s.len() {
            6 => Some(c),
            8 => Some(c.alpha(byte(6)? as f32 / 255.0)),
            _ => None,
        }
    }
    pub const fn alpha(self, a: f32) -> Self {
        Color { a, ..self }
    }
    /// Mistura com `o` na proporção `t` (0 = self, 1 = o): base dos estados hover/pressionado.
    pub fn mix(self, o: Color, t: f32) -> Self {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Color { r: l(self.r, o.r), g: l(self.g, o.g), b: l(self.b, o.b), a: l(self.a, o.a) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub const ZERO: Padding = Padding { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

impl From<f32> for Padding {
    fn from(p: f32) -> Self {
        Padding { top: p, right: p, bottom: p, left: p }
    }
}
impl From<u16> for Padding {
    fn from(p: u16) -> Self {
        (p as f32).into()
    }
}
/// `[vertical, horizontal]`, como no Iced.
impl From<[u16; 2]> for Padding {
    fn from([v, h]: [u16; 2]) -> Self {
        Padding { top: v as f32, right: h as f32, bottom: v as f32, left: h as f32 }
    }
}
impl From<[f32; 2]> for Padding {
    fn from([v, h]: [f32; 2]) -> Self {
        Padding { top: v, right: h, bottom: v, left: h }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// O tamanho do conteúdo.
    Shrink,
    /// Ocupa a sobra; o número é a proporção entre os irmãos que também enchem.
    Fill(u16),
    Fixed(f32),
}

impl Length {
    pub const FILL: Length = Length::Fill(1);
    fn fill(&self) -> u16 {
        match self {
            Length::Fill(n) => *n,
            _ => 0,
        }
    }
}

impl From<f32> for Length {
    fn from(v: f32) -> Self {
        Length::Fixed(v)
    }
}
impl From<u16> for Length {
    fn from(v: u16) -> Self {
        Length::Fixed(v as f32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

impl Align {
    /// Deslocamento de algo de tamanho `item` dentro de `espaco`.
    pub fn offset(self, espaco: f32, item: f32) -> f32 {
        match self {
            Align::Start => 0.0,
            Align::Center => ((espaco - item) / 2.0).max(0.0),
            Align::End => (espaco - item).max(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Border {
    pub color: Color,
    pub width: f32,
    /// Raio de cada canto: superior esquerdo, superior direito, inferior direito, inferior esquerdo.
    pub radius: [f32; 4],
}

impl Default for Color {
    fn default() -> Self {
        Color::TRANSPARENT
    }
}

impl Border {
    pub fn rounded(r: f32) -> Self {
        Border { radius: [r; 4], ..Default::default() }
    }
    pub fn color(self, color: Color, width: f32) -> Self {
        Border { color, width, ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Shadow {
    pub color: Color,
    pub offset: Point,
    /// Quanto a sombra espalha para fora, em pixels lógicos.
    pub blur: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Color(Color),
    /// Ângulo em radianos (0 = da esquerda para a direita) e as paradas `(posição 0..1, cor)`.
    Linear { angle: f32, stops: Vec<(f32, Color)> },
}

impl Background {
    /// As cores com o alfa multiplicado por `a` (opacidade de um grupo).
    pub(crate) fn faded(&self, a: f32) -> Background {
        match self {
            Background::Color(c) => Background::Color(c.alpha(c.a * a)),
            Background::Linear { angle, stops } => Background::Linear { angle: *angle, stops: stops.iter().map(|(p, c)| (*p, c.alpha(c.a * a))).collect() },
        }
    }
}

impl From<Color> for Background {
    fn from(c: Color) -> Self {
        Background::Color(c)
    }
}

/// Paleta padrão dos widgets; cada widget aceita `.style(...)` para fugir dela.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub muted: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub border: Color,
    pub danger: Color,
}

impl Theme {
    pub fn dark() -> Self {
        Theme {
            background: Color::rgb8(0x16, 0x18, 0x1d),
            surface: Color::rgb8(0x21, 0x24, 0x2b),
            text: Color::rgb8(0xe8, 0xea, 0xee),
            muted: Color::rgb8(0x9a, 0xa0, 0xab),
            primary: Color::rgb8(0x3b, 0x82, 0xf6),
            on_primary: Color::WHITE,
            border: Color::rgb8(0x33, 0x37, 0x40),
            danger: Color::rgb8(0xef, 0x44, 0x44),
        }
    }
    pub fn light() -> Self {
        Theme {
            background: Color::rgb8(0xf6, 0xf7, 0xf9),
            surface: Color::WHITE,
            text: Color::rgb8(0x1b, 0x1f, 0x27),
            muted: Color::rgb8(0x5f, 0x66, 0x73),
            primary: Color::rgb8(0x25, 0x63, 0xeb),
            on_primary: Color::WHITE,
            border: Color::rgb8(0xd9, 0xdc, 0xe1),
            danger: Color::rgb8(0xdc, 0x26, 0x26),
        }
    }
}

/// O que acontece na janela, já em pixels lógicos.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    CursorMoved(Point),
    CursorLeft,
    MousePressed(MouseButton),
    MouseReleased(MouseButton),
    /// Rolagem em pixels lógicos; positivo = conteúdo desce (roda para cima).
    /// `modifiers`: com Ctrl, os apps costumam dar zoom em vez de rolar.
    Wheel { dx: f32, dy: f32, modifiers: Modifiers },
    Key { key: Key, text: Option<String>, modifiers: Modifiers, pressed: bool },
    Resized(Size),
    /// A janela foi para outro lugar da tela (lógico). O Wayland nunca conta.
    Moved(Point),
    CloseRequested,
    Focused(bool),
    /// Um arquivo arrastado de fora foi solto na janela (um evento por arquivo).
    FileDropped(std::path::PathBuf),
    /// Um arquivo arrastado está em cima da janela (para mostrar onde vai cair).
    FileHovered(std::path::PathBuf),
    /// O arrasto saiu da janela sem soltar.
    FileHoverCancelled,
    ModifiersChanged(Modifiers),
    /// A escala da tela (1.0, 1.25, 2.0…): na abertura da janela e quando muda de monitor.
    ScaleChanged(f32),
    /// Toque na tela (o `touch::Event` do Iced). `id` separa os dedos. O primeiro dedo que nenhum
    /// widget pegar também chega como mouse (andar + botão esquerdo), para botão e cia. funcionarem.
    FingerPressed { id: u64, position: Point },
    FingerMoved { id: u64, position: Point },
    FingerLifted { id: u64, position: Point },
    /// O sistema cancelou o toque (virou gesto, a janela perdeu o foco).
    FingerLost { id: u64, position: Point },
}

pub use winit::event::MouseButton;

/// Se o widget consumiu o evento: `Captured` não deixa ele chegar aos de baixo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Captured {
    Yes,
    No,
}

/// Estado de interação passado às closures de estilo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Hovered,
    Pressed,
    Focused,
    Disabled,
}

/// Estado que sobrevive entre quadros (rolagem, cursor do campo, botão apertado),
/// guardado pelo caminho do widget na árvore.
#[derive(Default)]
pub struct States {
    /// Por widget e tipo: o mesmo widget guarda mais de um estado (foco e cursor, por exemplo).
    map: HashMap<(u64, TypeId), Box<dyn Any>>,
    pub focus: Option<u64>,
    /// A hora do quadro. `None` (capturas e testes sem [`tick`](Self::tick)): toda transição já está no fim.
    pub(crate) agora: Option<Instant>,
    /// Conta os quadros: quem não foi desenhado no anterior está entrando na tela.
    pub(crate) quadro: u64,
    /// O estado das transições, por widget e tipo do valor; o desenho (que só lê `States`) muda ele.
    pub(crate) animacoes: RefCell<HashMap<(u64, TypeId), Box<dyn Any>>>,
    pub(crate) animando: Cell<bool>,
    /// Os menus e painéis abertos no último quadro ([`DrawCx::popup`]): o mouse em cima deles não
    /// chega ao resto da árvore.
    pub(crate) popups: RefCell<Vec<Rect>>,
    /// O mouse sobre um popup: só o dono dele vê ([`DrawCx::popup_cursor`]).
    pub(crate) sob_popup: Cell<Option<Point>>,
    /// Os widgets com `.id(..)` do último quadro: id → chave. É por aqui que um [`Task`] acha
    /// o campo a focar ou a rolagem a mexer.
    pub(crate) ids: RefCell<HashMap<u64, u64>>,
}

/// O sistema está no tema escuro? (GNOME/KDE pelo portal, Windows e macOS pelas preferências.)
/// Sem como saber, `false`.
pub fn system_dark() -> bool {
    matches!(dark_light::detect(), dark_light::Mode::Dark)
}

pub(crate) fn id_hash(id: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

impl States {
    /// Começa um quadro na hora `now`: as transições andam até ela. A janela chama a cada
    /// quadro; um teste chama para desenhar o meio de uma animação.
    pub fn tick(&mut self, now: Instant) {
        self.agora = Some(now);
        self.quadro += 1;
        self.animando.set(false);
        self.ids.get_mut().clear();
    }
    /// Alguma transição do último quadro ainda não chegou ao fim: a janela pede outro quadro.
    pub fn animating(&self) -> bool {
        self.animando.get()
    }
    pub fn get<T: 'static>(&self, key: u64) -> Option<&T> {
        self.map.get(&(key, TypeId::of::<T>()))?.downcast_ref()
    }
    /// shortcut: o caminho muda se a árvore muda de forma; o widget recebe o estado padrão de novo. Um `.id()` estável resolve se incomodar.
    pub fn get_mut<T: Default + 'static>(&mut self, key: u64) -> &mut T {
        self.map
            .entry((key, TypeId::of::<T>()))
            .or_insert_with(|| Box::new(T::default()))
            .downcast_mut()
            .expect("a chave leva o tipo")
    }
}
