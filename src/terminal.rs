//! Terminal de verdade: um shell num PTY, com o motor do Alacritty (`alacritty_terminal`)
//! interpretando a saída. Liga com a feature `terminal`.
use crate::widget::{CursorIcon, DrawCx, EventCx, Node, Widget};
use crate::{Border, Captured, Color, Event, Font, Key, LayoutCx, Length, Limits, Modifiers, MouseButton, NamedKey, Point, Rect, Size, Subscription};
use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{viewport_to_point, Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color as Cor, CursorShape, NamedColor, Rgb};
use futures::channel::mpsc;
use futures::StreamExt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// O que abrir no terminal.
#[derive(Debug, Clone, Default)]
pub struct TerminalSettings {
    /// `None`: o shell do usuário (`$SHELL`).
    pub program: Option<String>,
    pub args: Vec<String>,
    /// A pasta onde ele começa.
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

/// O que o terminal avisa ao app (pela [`Terminal::subscription`]).
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalEvent {
    /// Saiu texto novo: só pede um quadro.
    Output,
    Title(String),
    Bell,
    /// O processo acabou (com o código, quando se sabe).
    Exit(Option<i32>),
}

#[derive(Clone, Copy)]
struct Dim {
    colunas: usize,
    linhas: usize,
}

impl Dimensions for Dim {
    fn total_lines(&self) -> usize {
        self.linhas
    }
    fn screen_lines(&self) -> usize {
        self.linhas
    }
    fn columns(&self) -> usize {
        self.colunas
    }
}

/// Recebe os avisos do motor (na thread do PTY).
#[derive(Clone)]
struct Ouvinte {
    saida: mpsc::UnboundedSender<TerminalEvent>,
    /// Já há um `Output` a caminho que o desenho não consumiu: não manda outro.
    pendente: Arc<AtomicBool>,
    /// Para responder ao programa (posição do cursor, cores): o canal do PTY, que nasce depois.
    pty: Arc<OnceLock<EventLoopSender>>,
    tamanho: Arc<Mutex<WindowSize>>,
    /// O código de saída do filho: o motor manda `ChildExit` e depois `Exit`; o app recebe um só.
    codigo: Arc<Mutex<Option<i32>>>,
}

impl EventListener for Ouvinte {
    fn send_event(&self, ev: TermEvent) {
        let aviso = match ev {
            TermEvent::Wakeup | TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => {
                if self.pendente.swap(true, Ordering::AcqRel) {
                    return;
                }
                TerminalEvent::Output
            }
            TermEvent::Title(t) => TerminalEvent::Title(t),
            TermEvent::ResetTitle => TerminalEvent::Title(String::new()),
            TermEvent::Bell => TerminalEvent::Bell,
            TermEvent::Exit => TerminalEvent::Exit(self.codigo.lock().unwrap_or_else(|e| e.into_inner()).take()),
            TermEvent::ChildExit(c) => {
                *self.codigo.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
                return;
            }
            TermEvent::PtyWrite(t) => return self.responder(t),
            TermEvent::TextAreaSizeRequest(f) => {
                let t = f(*self.tamanho.lock().unwrap_or_else(|e| e.into_inner()));
                return self.responder(t);
            }
            TermEvent::ColorRequest(i, f) => return self.responder(f(rgb(paleta(i, None)))),
            // shortcut: OSC 52 (o programa lendo/escrevendo a área de transferência) é ignorado; ligar se algum TUI precisar.
            TermEvent::ClipboardStore(..) | TermEvent::ClipboardLoad(..) => return,
        };
        let _ = self.saida.unbounded_send(aviso);
    }
}

impl Ouvinte {
    fn responder(&self, t: String) {
        if let Some(p) = self.pty.get() {
            let _ = p.send(Msg::Input(Cow::Owned(t.into_bytes())));
        }
    }
}

/// Um shell rodando. Guarde no estado do app; mostre com [`terminal`] e ouça a
/// [`Terminal::subscription`]. Soltar o `Terminal` encerra o processo.
pub struct Terminal {
    id: u64,
    term: Arc<FairMutex<Term<Ouvinte>>>,
    pty: EventLoopSender,
    pendente: Arc<AtomicBool>,
    tamanho: Arc<Mutex<WindowSize>>,
    eventos: Arc<Mutex<Option<mpsc::UnboundedReceiver<TerminalEvent>>>>,
}

impl Terminal {
    pub fn new(s: TerminalSettings) -> std::io::Result<Terminal> {
        static PROXIMO: AtomicU64 = AtomicU64::new(1);
        let id = PROXIMO.fetch_add(1, Ordering::Relaxed);
        let tamanho = WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 16 };
        let mut env = s.env;
        env.entry("TERM".into()).or_insert_with(|| "xterm-256color".into());
        env.entry("COLORTERM".into()).or_insert_with(|| "truecolor".into());
        let opcoes = tty::Options { shell: s.program.map(|p| tty::Shell::new(p, s.args)), working_directory: s.cwd, drain_on_exit: true, env, ..Default::default() };
        let pty = tty::new(&opcoes, tamanho, id)?;
        let (tx, rx) = mpsc::unbounded();
        let ouvinte = Ouvinte { saida: tx, pendente: Arc::new(AtomicBool::new(false)), pty: Arc::new(OnceLock::new()), tamanho: Arc::new(Mutex::new(tamanho)), codigo: Arc::default() };
        let config = Config { scrolling_history: 10_000, ..Default::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &Dim { colunas: 80, linhas: 24 }, ouvinte.clone())));
        let laco = EventLoop::new(term.clone(), ouvinte.clone(), pty, true, false)?;
        let canal = laco.channel();
        let _ = ouvinte.pty.set(canal.clone());
        laco.spawn();
        Ok(Terminal { id, term, pty: canal, pendente: ouvinte.pendente, tamanho: ouvinte.tamanho, eventos: Arc::new(Mutex::new(Some(rx))) })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Os avisos deste terminal enquanto ele existir. Mapeie para a mensagem do app.
    pub fn subscription(&self) -> Subscription<TerminalEvent> {
        Subscription::run_with_id(("pelicano::terminal", self.id), self.events())
    }

    /// Os mesmos avisos como fluxo, para o app montar a própria assinatura (com o [`Terminal::id`]
    /// na mensagem, que o `Subscription::map` não deixa capturar). Só o primeiro fluxo recebe.
    pub fn events(&self) -> impl futures::Stream<Item = TerminalEvent> + Send + 'static {
        let eventos = self.eventos.clone();
        // O receptor só é tirado quando o fluxo começa de fato (uma assinatura repetida é descartada sem rodar).
        futures::stream::once(async move { eventos.lock().unwrap_or_else(|e| e.into_inner()).take() }).flat_map(|rx| match rx {
            Some(rx) => rx.boxed(),
            None => futures::stream::empty().boxed(),
        })
    }

    /// Manda bytes para o programa, como se fossem digitados.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let b = bytes.into();
        if !b.is_empty() {
            let _ = self.pty.send(Msg::Input(b));
        }
    }

    /// O texto selecionado com o mouse.
    pub fn selection(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// Todo o texto à vista (útil em testes e para "copiar tudo").
    pub fn screen_text(&self) -> String {
        let t = self.term.lock();
        let mut linhas = vec![String::new(); t.screen_lines()];
        for c in t.renderable_content().display_iter {
            let l = (c.point.line.0 + t.grid().display_offset() as i32) as usize;
            if l < linhas.len() && !c.flags.contains(Flags::WIDE_CHAR_SPACER) {
                linhas[l].push(c.c);
            }
        }
        linhas.iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n").trim_end().to_owned()
    }

    fn redimensionar(&self, colunas: usize, linhas: usize, cw: f32, ch: f32) {
        let novo = WindowSize { num_lines: linhas as u16, num_cols: colunas as u16, cell_width: cw as u16, cell_height: ch as u16 };
        let mut atual = self.tamanho.lock().unwrap_or_else(|e| e.into_inner());
        if (atual.num_lines, atual.num_cols) == (novo.num_lines, novo.num_cols) {
            return;
        }
        *atual = novo;
        self.term.lock().resize(Dim { colunas, linhas });
        let _ = self.pty.send(Msg::Resize(novo));
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.pty.send(Msg::Shutdown);
    }
}

// ---------- Cores ----------

fn rgb(c: Color) -> Rgb {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb { r: f(c.r), g: f(c.g), b: f(c.b) }
}

/// As 16 cores ANSI (as do tema escuro do VS Code), o cubo de 256 e os tons de cinza.
fn paleta(i: usize, base: Option<&[Color; 16]>) -> Color {
    const ANSI: [u32; 16] = [0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff];
    let hex = |h: u32| Color::rgb8((h >> 16) as u8, (h >> 8) as u8, h as u8);
    match i {
        0..=15 => base.map_or_else(|| hex(ANSI[i]), |b| b[i]),
        16..=231 => {
            let n = i - 16;
            let v = |k: usize| if k == 0 { 0 } else { (55 + k * 40) as u8 };
            Color::rgb8(v(n / 36), v(n / 6 % 6), v(n % 6))
        }
        232..=255 => {
            let v = (8 + (i - 232) * 10) as u8;
            Color::rgb8(v, v, v)
        }
        _ => hex(0xcccccc),
    }
}

struct Cores<'a> {
    texto: Color,
    fundo: Color,
    base: Option<&'a [Color; 16]>,
    trocadas: &'a alacritty_terminal::term::color::Colors,
}

impl Cores<'_> {
    fn de(&self, c: Cor) -> Color {
        let i = match c {
            Cor::Spec(Rgb { r, g, b }) => return Color::rgb8(r, g, b),
            Cor::Indexed(i) => i as usize,
            Cor::Named(n) => n as usize,
        };
        // O programa pode ter trocado a cor (OSC 4/10/11).
        if let Some(Rgb { r, g, b }) = self.trocadas[i] {
            return Color::rgb8(r, g, b);
        }
        match i {
            0..=255 => paleta(i, self.base),
            _ if i == NamedColor::Background as usize => self.fundo,
            _ if i == NamedColor::Foreground as usize || i == NamedColor::BrightForeground as usize || i == NamedColor::Cursor as usize => self.texto,
            _ if i == NamedColor::DimForeground as usize => self.texto.alpha(0.66),
            // DimBlack..DimWhite: as 8 primeiras, apagadas.
            _ => paleta(i - NamedColor::DimBlack as usize, self.base).alpha(0.66),
        }
    }
}

// ---------- O widget ----------

pub struct TerminalView<'a> {
    term: &'a Terminal,
    size: f32,
    font: Font,
    text_color: Option<Color>,
    background: Option<Color>,
    palette: Option<[Color; 16]>,
    padding: f32,
    id: Option<u64>,
}

/// Mostra um [`Terminal`]. Clique dá o foco; com foco, o teclado vai para o programa
/// (Ctrl+Shift+C/V copiam e colam, Shift+PageUp/PageDown rolam o histórico).
pub fn terminal(t: &Terminal) -> TerminalView<'_> {
    TerminalView { term: t, size: 13.0, font: Font::MONO, text_color: None, background: None, palette: None, padding: 6.0, id: None }
}

impl TerminalView<'_> {
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn font(mut self, f: Font) -> Self {
        self.font = f;
        self
    }
    /// A cor padrão do texto. Padrão: `theme.text`.
    pub fn text_color(mut self, c: Color) -> Self {
        self.text_color = Some(c);
        self
    }
    /// O fundo. Padrão: `theme.background`.
    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }
    /// As 16 cores ANSI (preto, vermelho, verde, amarelo, azul, magenta, ciano, branco, e as claras).
    pub fn palette(mut self, p: [Color; 16]) -> Self {
        self.palette = Some(p);
        self
    }
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }
    /// O nome para [`crate::Task::focus`] (levar o teclado ao terminal sem clicar).
    pub fn id(mut self, id: impl std::hash::Hash) -> Self {
        self.id = Some(crate::id_hash(&id));
        self
    }

    fn celula(&self, r: &mut crate::Renderer) -> (f32, f32) {
        let w = r.measure("MMMMMMMMMM", self.size, self.font, None).width / 10.0;
        (w.max(1.0), (self.size * 1.3).ceil())
    }

    fn area(&self, n: &Node) -> Rect {
        let p = self.padding;
        Rect { x: n.rect.x + p, y: n.rect.y + p, width: (n.rect.width - 2.0 * p).max(1.0), height: (n.rect.height - 2.0 * p).max(1.0) }
    }

    /// A célula sob `p` (linha e coluna da tela) e o lado dela.
    fn celula_em(&self, n: &Node, p: Point, cw: f32, ch: f32) -> (usize, usize, Side) {
        let a = self.area(n);
        let t = self.term.tamanho.lock().unwrap_or_else(|e| e.into_inner());
        let (x, y) = ((p.x - a.x).max(0.0), (p.y - a.y).max(0.0));
        let col = ((x / cw) as usize).min(t.num_cols.max(1) as usize - 1);
        let lin = ((y / ch) as usize).min(t.num_lines.max(1) as usize - 1);
        let lado = if x - col as f32 * cw < cw / 2.0 { Side::Left } else { Side::Right };
        (lin, col, lado)
    }
}

#[derive(Default)]
struct Mouse {
    selecionando: bool,
    ultimo: Option<(std::time::Instant, u8)>,
}

impl<M> Widget<M> for TerminalView<'_> {
    fn size(&self) -> (Length, Length) {
        (Length::FILL, Length::FILL)
    }
    fn layout(&self, cx: &mut LayoutCx, l: Limits) -> Node {
        if let Some(id) = self.id {
            cx.states.ids.borrow_mut().insert(id, cx.key());
        }
        Node::leaf(cx.key(), l.fill(Size::new(400.0, 300.0)))
    }
    fn draw(&self, n: &Node, cx: &mut DrawCx) {
        let (cw, ch) = self.celula(cx.renderer);
        let a = self.area(n);
        self.term.redimensionar(((a.width / cw) as usize).max(2), ((a.height / ch) as usize).max(1), cw, ch);
        self.term.pendente.store(false, Ordering::Release);
        let focado = cx.states.focus == Some(n.key);
        if cx.hovered(&n.rect) {
            cx.cursor_icon = CursorIcon::Text;
        }
        let fundo = self.background.unwrap_or(cx.theme.background);
        let texto = self.text_color.unwrap_or(cx.theme.text);
        cx.renderer.fill_quad(n.rect, Some(&fundo.into()), Border::default(), None);
        cx.renderer.push_clip(n.rect, [0.0; 4]);

        let term = self.term.term.lock();
        let conteudo = term.renderable_content();
        let cores = Cores { texto, fundo, base: self.palette.as_ref(), trocadas: conteudo.colors };
        let deslocamento = conteudo.display_offset as i32;
        let selecao = conteudo.selection;
        let cursor = conteudo.cursor;
        let cor_selecao = cx.theme.primary.alpha(0.35);
        // Um trecho: (linha, coluna inicial, texto, cor, negrito, itálico, sublinhado, tachado).
        let mut trechos: Vec<(i32, usize, String, Color, bool, bool, bool, bool)> = Vec::new();
        let mut fundos: Vec<(i32, usize, usize, Color)> = Vec::new();
        for c in conteudo.display_iter {
            if c.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let lin = c.point.line.0 + deslocamento;
            let col = c.point.column.0;
            let (mut fg, mut bg) = (cores.de(c.fg), cores.de(c.bg));
            if c.flags.contains(Flags::BOLD) {
                // Negrito numa das 8 primeiras vira a clara, como nos terminais de sempre.
                if let Cor::Named(nc) = c.fg {
                    if (nc as usize) < 8 {
                        fg = cores.de(Cor::Indexed(nc as u8 + 8));
                    }
                }
            }
            if c.flags.contains(Flags::DIM) {
                fg = fg.alpha(0.66);
            }
            if c.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if c.flags.contains(Flags::HIDDEN) {
                fg = bg;
            }
            let largura = if c.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
            if selecao.is_some_and(|s| s.contains(c.point)) {
                bg = cor_selecao;
            }
            if bg != fundo {
                match fundos.last_mut() {
                    Some((l, ini, n, cor)) if *l == lin && *ini + *n == col && *cor == bg => *n += largura,
                    _ => fundos.push((lin, col, largura, bg)),
                }
            }
            if c.c == ' ' || c.c == '\t' {
                continue;
            }
            let (neg, ita, sub, tac) = (c.flags.contains(Flags::BOLD), c.flags.contains(Flags::ITALIC), c.flags.intersects(Flags::ALL_UNDERLINES), c.flags.contains(Flags::STRIKEOUT));
            let mut s = c.c.to_string();
            if let Some(z) = c.cell.zerowidth() {
                s.extend(z);
            }
            // Letras seguidas com o mesmo estilo saem num texto só; largas vão sozinhas (a fonte
            // de fallback não respeita a grade).
            match trechos.last_mut() {
                Some((l, ini, t, cor, n, i, u, k)) if largura == 1 && *l == lin && *ini + t.chars().count() == col && *cor == fg && (*n, *i, *u, *k) == (neg, ita, sub, tac) && t.is_ascii() && c.c.is_ascii() => t.push_str(&s),
                _ => trechos.push((lin, col, s, fg, neg, ita, sub, tac)),
            }
        }
        drop(term);

        for (lin, col, n, cor) in fundos {
            cx.renderer.fill_quad(Rect { x: a.x + col as f32 * cw, y: a.y + lin as f32 * ch, width: n as f32 * cw, height: ch }, Some(&cor.into()), Border::default(), None);
        }
        let desce = ((ch - cx.renderer.measure("M", self.size, self.font, None).height) / 2.0).max(0.0);
        for (lin, col, t, cor, neg, ita, sub, tac) in trechos {
            let mut f = self.font;
            if neg {
                f = f.weight(700);
            }
            if ita {
                f = f.italic();
            }
            let (x, y) = (a.x + col as f32 * cw, a.y + lin as f32 * ch);
            cx.renderer.draw_text(&t, self.size, f, cor, Point::new(x, y + desce), None);
            let largura = t.chars().count() as f32 * cw;
            if sub {
                cx.renderer.fill_quad(Rect { x, y: y + ch - 2.0, width: largura, height: 1.0 }, Some(&cor.into()), Border::default(), None);
            }
            if tac {
                cx.renderer.fill_quad(Rect { x, y: y + ch / 2.0, width: largura, height: 1.0 }, Some(&cor.into()), Border::default(), None);
            }
        }
        let lin = cursor.point.line.0 + deslocamento;
        if cursor.shape != CursorShape::Hidden && lin >= 0 {
            let r = Rect { x: a.x + cursor.point.column.0 as f32 * cw, y: a.y + lin as f32 * ch, width: cw, height: ch };
            let forma = if focado { cursor.shape } else { CursorShape::HollowBlock };
            let cor = texto.alpha(0.8);
            match forma {
                CursorShape::Beam => cx.renderer.fill_quad(Rect { width: 2.0, ..r }, Some(&cor.into()), Border::default(), None),
                CursorShape::Underline => cx.renderer.fill_quad(Rect { y: r.y + r.height - 2.0, height: 2.0, ..r }, Some(&cor.into()), Border::default(), None),
                CursorShape::HollowBlock => cx.renderer.fill_quad(r, None, Border::default().color(cor, 1.0), None),
                _ => cx.renderer.fill_quad(r, Some(&texto.alpha(0.45).into()), Border::default(), None),
            }
        }
        cx.renderer.pop_clip();
        cx.access_text(n.key, n.rect, "terminal");
    }
    fn event(&self, n: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        let dentro = cx.cursor.is_some_and(|p| n.rect.contains(p));
        let focado = cx.states.focus == Some(n.key);
        let (cw, ch) = self.celula(cx.renderer);
        match ev {
            Event::MousePressed(MouseButton::Left) if dentro => {
                cx.states.focus = Some(n.key);
                let (lin, col, lado) = self.celula_em(n, cx.cursor.unwrap_or(Point::ORIGIN), cw, ch);
                let agora = std::time::Instant::now();
                let m = cx.states.get_mut::<Mouse>(n.key);
                let vezes = match m.ultimo {
                    Some((t, k)) if agora.duration_since(t).as_millis() < 400 => k % 3 + 1,
                    _ => 1,
                };
                m.ultimo = Some((agora, vezes));
                m.selecionando = true;
                let tipo = match vezes {
                    1 => SelectionType::Simple,
                    2 => SelectionType::Semantic,
                    _ => SelectionType::Lines,
                };
                let mut t = self.term.term.lock();
                let p = viewport_to_point(t.grid().display_offset(), alacritty_terminal::index::Point::new(lin, Column(col)));
                t.selection = Some(Selection::new(tipo, p, lado));
                cx.redraw = true;
                Captured::Yes
            }
            Event::CursorMoved(p) if cx.states.get::<Mouse>(n.key).is_some_and(|m| m.selecionando) => {
                let (lin, col, lado) = self.celula_em(n, *p, cw, ch);
                let mut t = self.term.term.lock();
                let q = viewport_to_point(t.grid().display_offset(), alacritty_terminal::index::Point::new(lin, Column(col)));
                if let Some(s) = t.selection.as_mut() {
                    s.update(q, lado);
                }
                cx.redraw = true;
                Captured::Yes
            }
            Event::MouseReleased(MouseButton::Left) if cx.states.get::<Mouse>(n.key).is_some_and(|m| m.selecionando) => {
                cx.states.get_mut::<Mouse>(n.key).selecionando = false;
                let mut t = self.term.term.lock();
                if t.selection.as_ref().is_some_and(|s| s.is_empty()) {
                    t.selection = None;
                }
                Captured::Yes
            }
            Event::Wheel { dy, modifiers, .. } if dentro => {
                let linhas = (dy / ch).round() as i32;
                let linhas = if linhas == 0 { dy.signum() as i32 } else { linhas };
                let mut t = self.term.term.lock();
                if t.mode().contains(TermMode::ALT_SCREEN) && !modifiers.shift_key() {
                    // Tela cheia (less, vim): a roda vira setas.
                    // shortcut: sem relatório de mouse para o programa (cliques no htop/vim); ligar quando um TUI precisar.
                    let seta: &[u8] = match (linhas > 0, t.mode().contains(TermMode::APP_CURSOR)) {
                        (true, true) => b"\x1bOA",
                        (true, false) => b"\x1b[A",
                        (false, true) => b"\x1bOB",
                        (false, false) => b"\x1b[B",
                    };
                    drop(t);
                    self.term.write(seta.repeat(linhas.unsigned_abs() as usize));
                } else {
                    t.scroll_display(Scroll::Delta(linhas));
                }
                cx.redraw = true;
                Captured::Yes
            }
            Event::Key { key, text, modifiers, pressed: true } if focado => self.tecla(key, text.as_deref(), *modifiers, cx),
            _ => Captured::No,
        }
    }
}

impl TerminalView<'_> {
    fn tecla<M>(&self, key: &Key, text: Option<&str>, m: Modifiers, cx: &mut EventCx<M>) -> Captured {
        let (ctrl, shift, alt) = (m.control_key(), m.shift_key(), m.alt_key());
        let modo = *self.term.term.lock().mode();
        let app = modo.contains(TermMode::APP_CURSOR);
        // Ctrl+Shift: atalhos do terminal, não do programa.
        if ctrl && shift {
            if let Key::Character(k) = key {
                match k.to_lowercase().as_str() {
                    "c" => {
                        if let Some(t) = self.term.selection() {
                            cx.clipboard.write(t);
                        }
                        return Captured::Yes;
                    }
                    "v" => {
                        if let Some(t) = cx.clipboard.read() {
                            self.colar(colagem(&t, modo.contains(TermMode::BRACKETED_PASTE)));
                        }
                        return Captured::Yes;
                    }
                    _ => {}
                }
            }
        }
        if shift && matches!(key, Key::Named(NamedKey::PageUp | NamedKey::PageDown)) {
            let mut t = self.term.term.lock();
            t.scroll_display(if matches!(key, Key::Named(NamedKey::PageUp)) { Scroll::PageUp } else { Scroll::PageDown });
            cx.redraw = true;
            return Captured::Yes;
        }
        let seta = |c: char| -> Vec<u8> {
            // Com modificador: CSI 1;mod X. Sem: SS3 X no modo de aplicação.
            let mods = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
            if mods > 1 {
                format!("\x1b[1;{mods}{c}").into_bytes()
            } else if app {
                format!("\x1bO{c}").into_bytes()
            } else {
                format!("\x1b[{c}").into_bytes()
            }
        };
        let til = |n: u8| -> Vec<u8> {
            let mods = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
            if mods > 1 { format!("\x1b[{n};{mods}~") } else { format!("\x1b[{n}~") }.into_bytes()
        };
        let bytes: Vec<u8> = match key {
            Key::Named(NamedKey::Enter) => b"\r".to_vec(),
            Key::Named(NamedKey::Backspace) => if ctrl { b"\x08".to_vec() } else if alt { b"\x1b\x7f".to_vec() } else { b"\x7f".to_vec() },
            Key::Named(NamedKey::Tab) => if shift { b"\x1b[Z".to_vec() } else { b"\t".to_vec() },
            Key::Named(NamedKey::Escape) => b"\x1b".to_vec(),
            Key::Named(NamedKey::ArrowUp) => seta('A'),
            Key::Named(NamedKey::ArrowDown) => seta('B'),
            Key::Named(NamedKey::ArrowRight) => seta('C'),
            Key::Named(NamedKey::ArrowLeft) => seta('D'),
            Key::Named(NamedKey::Home) => seta('H'),
            Key::Named(NamedKey::End) => seta('F'),
            Key::Named(NamedKey::Insert) => til(2),
            Key::Named(NamedKey::Delete) => til(3),
            Key::Named(NamedKey::PageUp) => til(5),
            Key::Named(NamedKey::PageDown) => til(6),
            Key::Named(NamedKey::F1) => b"\x1bOP".to_vec(),
            Key::Named(NamedKey::F2) => b"\x1bOQ".to_vec(),
            Key::Named(NamedKey::F3) => b"\x1bOR".to_vec(),
            Key::Named(NamedKey::F4) => b"\x1bOS".to_vec(),
            Key::Named(NamedKey::F5) => til(15),
            Key::Named(NamedKey::F6) => til(17),
            Key::Named(NamedKey::F7) => til(18),
            Key::Named(NamedKey::F8) => til(19),
            Key::Named(NamedKey::F9) => til(20),
            Key::Named(NamedKey::F10) => til(21),
            Key::Named(NamedKey::F11) => til(23),
            Key::Named(NamedKey::F12) => til(24),
            Key::Named(NamedKey::Space) if ctrl => vec![0],
            Key::Character(k) if ctrl => {
                // Ctrl+letra: o caractere de controle (Ctrl+C = 3).
                let c = k.chars().next().unwrap_or('\0').to_ascii_lowercase();
                let b = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    '[' | '3' => 27,
                    '\\' | '4' => 28,
                    ']' | '5' => 29,
                    '^' | '6' => 30,
                    '_' | '7' | '/' => 31,
                    '@' | '2' => 0,
                    _ => return Captured::No,
                };
                if alt { vec![0x1b, b] } else { vec![b] }
            }
            _ => match text.filter(|t| !t.is_empty()) {
                Some(t) => {
                    let mut b = Vec::new();
                    if alt {
                        b.push(0x1b);
                    }
                    b.extend_from_slice(t.as_bytes());
                    b
                }
                None => return Captured::No,
            },
        };
        self.colar(bytes);
        cx.redraw = true;
        Captured::Yes
    }

    /// Manda para o programa e volta a rolagem para o fim (digitar mostra onde se digita).
    fn colar(&self, bytes: Vec<u8>) {
        {
            let mut t = self.term.term.lock();
            t.scroll_display(Scroll::Bottom);
            t.selection = None;
        }
        self.term.write(bytes);
    }
}

/// O que vai ao PTY ao colar `texto`. Entre as marcas, um ESC colado fecharia a colagem antes
/// da hora (e o resto viraria comando): sai, como no Alacritty.
fn colagem(texto: &str, bracketed: bool) -> Vec<u8> {
    let t = texto.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed { format!("\x1b[200~{}\x1b[201~", t.replace('\x1b', "")) } else { t }.into_bytes()
}

impl<'a, M: 'a> From<TerminalView<'a>> for crate::Element<'a, M> {
    fn from(w: TerminalView<'a>) -> Self {
        crate::Element::new(w)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn roda_um_comando_e_mostra_a_saida() {
        let t = Terminal::new(TerminalSettings { program: Some("/bin/sh".into()), args: vec!["-c".into(), "printf 'ola \\033[31mvermelho\\033[0m'; sleep 5".into()], ..Default::default() }).expect("pty");
        let fim = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !t.screen_text().contains("vermelho") && std::time::Instant::now() < fim {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(t.screen_text().starts_with("ola vermelho"), "{:?}", t.screen_text());
        let term = t.term.lock();
        let cor = term.renderable_content().display_iter.find(|c| c.c == 'v').map(|c| c.fg);
        assert_eq!(cor, Some(Cor::Named(NamedColor::Red)));
    }

    #[test]
    fn colagem_nao_escapa_das_marcas() {
        assert_eq!(super::colagem("ls\x1b[201~rm -rf ~\r", true), b"\x1b[200~ls[201~rm -rf ~\r\x1b[201~");
        assert_eq!(super::colagem("a\r\nb\n", false), b"a\rb\r");
    }
}
