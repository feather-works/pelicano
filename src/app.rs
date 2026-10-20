//! A janela, o laço de eventos, as tarefas assíncronas e a apresentação do quadro.
use crate::widget::{self, CursorIcon, Element, EventCx};
use crate::render::Quadro;
use crate::{Changes, Color, Event, Modifiers, Point, Renderer, ResizeDirection, Size, States, Theme};
use futures::future::{AbortHandle, Abortable, BoxFuture};
use futures::stream::BoxStream;
use futures::{FutureExt, Stream, StreamExt};
use std::future::Future;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Icon, Window, WindowLevel};

/// Um aplicativo: estado, como ele muda (`update`) e como aparece (`view`).
pub trait App: Sized + 'static {
    type Msg: Clone + Send + 'static;
    fn update(&mut self, msg: Self::Msg) -> Task<Self::Msg>;
    fn view(&self) -> Element<'_, Self::Msg>;
    fn theme(&self) -> Theme {
        Theme::dark()
    }
    /// Todo evento da janela passa aqui também (atalhos, fechar, redimensionar).
    fn on_event(&self, _ev: &Event) -> Option<Self::Msg> {
        None
    }
    /// O que aparece numa janela aberta com [`Task::open_window`] (a principal usa `view`).
    fn view_window(&self, id: WindowId) -> Element<'_, Self::Msg> {
        let _ = id;
        self.view()
    }
    /// Os eventos de cada janela; na principal, o padrão chama `on_event`.
    fn on_window_event(&self, id: WindowId, ev: &Event) -> Option<Self::Msg> {
        if id == WindowId::MAIN {
            self.on_event(ev)
        } else {
            None
        }
    }
}

/// Qual janela: o app escolhe o número de cada uma que abre; a principal é a [`WindowId::MAIN`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u64);

impl WindowId {
    pub const MAIN: WindowId = WindowId(0);
}

#[derive(Clone)]
pub struct Settings {
    pub title: String,
    pub size: Size,
    pub min_size: Option<Size>,
    /// Sem a barra do sistema: o app desenha a própria (veja [`WindowCmd::Drag`]).
    pub decorations: bool,
    /// Janela com alfa: cantos arredondados e fundo translúcido.
    pub transparent: bool,
    /// Pede ao compositor para desfocar o que está atrás da janela (KDE, Windows, macOS; os outros ignoram).
    pub blur: bool,
    pub resizable: bool,
    /// `false`: o pedido de fechar só chega como [`Event::CloseRequested`] e o app decide.
    pub close_on_request: bool,
    /// Nome da janela para o sistema (Linux: casa com o arquivo .desktop).
    pub app_id: Option<String>,
    /// Ícone da janela e da barra de tarefas (Windows e X11; no Wayland vem do .desktop do `app_id`).
    pub icon: Option<Icon>,
    /// Onde a janela abre (lógico, canto de cima à esquerda); `None` = o sistema escolhe.
    /// shortcut: o Wayland ignora (o compositor decide onde as janelas ficam).
    pub position: Option<Point>,
    /// Fora da barra de tarefas e da dock (notas soltas, paletas). Windows e X11; o Wayland não deixa.
    pub skip_taskbar: bool,
    /// Linux: usa o X11 (XWayland, num Wayland) em vez do Wayland. Lá o app pode escolher a posição,
    /// prender no fundo e ficar fora da dock. Vale para o app todo (um laço de eventos só); sem X, cai no padrão.
    pub x11: bool,
    /// Fechar a última janela (ou a principal) não acaba o app: ele segue em segundo plano até um
    /// [`WindowCmd::Exit`]. Para apps de bandeja e de notas soltas.
    pub run_in_background: bool,
    /// A janela principal não abre sozinha: só com [`Task::open_window`] de [`WindowId::MAIN`].
    pub start_hidden: bool,
    /// Libera o inspetor de layout no F12. Desligado por padrão; `PELICANO_DEBUG=1` também libera (e já abre ligado).
    pub debug: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { title: "Pelicano".into(), size: Size::new(1024.0, 720.0), min_size: None, decorations: true, transparent: false, blur: false, resizable: true, close_on_request: true, app_id: None, icon: None, position: None, skip_taskbar: false, x11: false, run_in_background: false, start_hidden: false, debug: false }
    }
}

/// Ordens para a janela, disparadas por um [`Task`].
#[derive(Debug, Clone, PartialEq)]
pub enum WindowCmd {
    /// Começa a arrastar a janela (chame no clique da barra de título).
    Drag,
    /// Começa a redimensionar pela borda — funciona sem moldura.
    DragResize(ResizeDirection),
    Minimize,
    ToggleMaximize,
    /// Tela cheia sem moldura no monitor atual, ou volta.
    ToggleFullscreen,
    Close,
    Resize(Size),
    Title(String),
    Blur(bool),
    /// Sempre por cima ou sempre no fundo (presa na área de trabalho).
    /// shortcut: o Wayland não deixa o app escolher; lá não faz nada (X11, Windows e macOS fazem).
    Level(WindowLevel),
    /// Acaba o app (fecha todas as janelas).
    Exit,
    /// Traz para a frente (e tira do minimizado).
    /// shortcut: no Wayland o compositor só obedece com um token de ativação; sem ele a janela só pisca na barra.
    Focus,
}

enum Action<M> {
    Future(BoxFuture<'static, M>),
    Stream(BoxStream<'static, M>),
    Window(WindowId, WindowCmd),
    Open(WindowId, Settings),
}

/// Trabalho que o `update` devolve: futuros, fluxos e ordens para a janela.
pub struct Task<M>(Vec<Action<M>>);

/// Cancela o que um [`Task::abortable`] está fazendo.
#[derive(Debug, Clone)]
pub struct Handle(Vec<AbortHandle>);

impl Handle {
    pub fn abort(&self) {
        self.0.iter().for_each(AbortHandle::abort);
    }
}

impl<M: Send + 'static> Task<M> {
    pub fn none() -> Self {
        Task(Vec::new())
    }
    /// Uma mensagem já pronta, entregue no próximo giro do laço.
    pub fn done(m: M) -> Self {
        Task(vec![Action::Future(futures::future::ready(m).boxed())])
    }
    pub fn perform<T>(fut: impl Future<Output = T> + Send + 'static, f: impl FnOnce(T) -> M + Send + 'static) -> Self {
        Task(vec![Action::Future(fut.map(f).boxed())])
    }
    /// Cada item do fluxo vira uma mensagem (progresso de download, por exemplo).
    pub fn run<T>(s: impl Stream<Item = T> + Send + 'static, f: impl FnMut(T) -> M + Send + 'static) -> Self {
        Task(vec![Action::Stream(s.map(f).boxed())])
    }
    /// Uma ordem para a janela principal.
    pub fn window(cmd: WindowCmd) -> Self {
        Self::window_of(WindowId::MAIN, cmd)
    }
    pub fn window_of(id: WindowId, cmd: WindowCmd) -> Self {
        Task(vec![Action::Window(id, cmd)])
    }
    /// Abre outra janela; ela mostra o [`App::view_window`] de `id`. Fechar a principal fecha todas.
    // shortcut: o app acaba quando a principal fecha; deixar as outras vivas quando alguém pedir (post-it sem o bloco aberto).
    pub fn open_window(id: WindowId, settings: Settings) -> Self {
        Task(vec![Action::Open(id, settings)])
    }
    pub fn batch(tasks: impl IntoIterator<Item = Task<M>>) -> Self {
        Task(tasks.into_iter().flat_map(|t| t.0).collect())
    }
    pub fn map<N: Send + 'static>(self, f: impl Fn(M) -> N + Send + Sync + 'static) -> Task<N> {
        let f = Arc::new(f);
        Task(
            self.0
                .into_iter()
                .map(|a| match a {
                    Action::Future(x) => {
                        let f = f.clone();
                        Action::Future(x.map(move |m| f(m)).boxed())
                    }
                    Action::Stream(x) => {
                        let f = f.clone();
                        Action::Stream(x.map(move |m| f(m)).boxed())
                    }
                    Action::Window(id, c) => Action::Window(id, c),
                    Action::Open(id, s) => Action::Open(id, s),
                })
                .collect(),
        )
    }
    /// O mesmo trabalho, com uma alça para cancelar (pausar download).
    pub fn abortable(self) -> (Self, Handle) {
        let mut alcas = Vec::new();
        let acoes = self
            .0
            .into_iter()
            .filter_map(|a| {
                let (h, reg) = AbortHandle::new_pair();
                alcas.push(h);
                match a {
                    // Futuro cancelado não manda mensagem nenhuma.
                    Action::Future(x) => Some(Action::Stream(futures::stream::once(Abortable::new(x, reg)).filter_map(|r| futures::future::ready(r.ok())).boxed())),
                    Action::Stream(x) => Some(Action::Stream(Abortable::new(x, reg).boxed())),
                    w => Some(w),
                }
            })
            .collect();
        (Task(acoes), Handle(alcas))
    }
}

#[derive(Debug)]
pub enum Error {
    EventLoop(winit::error::EventLoopError),
    Window(String),
    Gpu(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EventLoop(e) => write!(f, "laço de eventos: {e}"),
            Error::Window(e) => write!(f, "janela: {e}"),
            Error::Gpu(e) => write!(f, "apresentação: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// Abre a janela e roda o app até ela fechar.
pub fn run<A: App>(settings: Settings, init: impl FnOnce() -> (A, Task<A::Msg>)) -> Result<(), Error> {
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    let x11 = settings.x11.then(|| {
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoop::<A::Msg>::with_user_event().with_x11().build().ok()
    });
    #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
    let x11: Option<Option<EventLoop<A::Msg>>> = None;
    let event_loop = match x11.flatten() {
        Some(el) => el,
        None => EventLoop::<A::Msg>::with_user_event().build().map_err(Error::EventLoop)?,
    };
    let pool = futures::executor::ThreadPool::new().map_err(|e| Error::Window(e.to_string()))?;
    let (app, task) = init();
    let mut rt = Runtime { app, settings, proxy: event_loop.create_proxy(), pool, clipboard: Box::new(widget::MemoryClipboard::default()), janelas: Vec::new(), gpu: None, renderer: Renderer::new(), modifiers: Modifiers::empty(), abrir: Vec::new(), pendentes: Vec::new(), erro: None, sair: false, iniciou: false, inspecionar: false, intervalo: None };
    if std::env::var_os("PELICANO_DEBUG").is_some_and(|v| v != "0") {
        rt.settings.debug = true;
        rt.inspecionar = true;
    }
    rt.run_task(task);
    event_loop.run_app(&mut rt).map_err(Error::EventLoop)?;
    rt.erro.map_or(Ok(()), Err)
}

struct Runtime<A: App> {
    app: A,
    /// As da janela principal (o `debug` vale para todas).
    settings: Settings,
    proxy: EventLoopProxy<A::Msg>,
    pool: futures::executor::ThreadPool,
    /// A do sistema, ligada quando a primeira janela abre (antes disso, na memória).
    clipboard: Box<dyn widget::Clipboard>,
    janelas: Vec<Janela>,
    gpu: Option<Gpu>,
    /// Um só para todas as janelas: as fontes e os caches são o que pesa.
    renderer: Renderer,
    modifiers: Modifiers,
    /// Janelas pedidas: abrem no próximo evento (criar janela precisa do laço).
    abrir: Vec<(WindowId, Settings)>,
    /// Ordens para janelas que ainda não abriram.
    pendentes: Vec<(WindowId, WindowCmd)>,
    erro: Option<Error>,
    sair: bool,
    /// O `resumed` já abriu (ou deixou fechada) a principal.
    iniciou: bool,
    /// O inspetor está à mostra (F12, só com `Settings::debug`).
    inspecionar: bool,
    /// Um quadro do monitor (perguntar a cada evento custa caro); some quando a janela muda de tela.
    intervalo: Option<std::time::Duration>,
}

/// Cada janela aberta: o que é dela e não pode ser dividido.
struct Janela {
    id: WindowId,
    w: Arc<Window>,
    tela: Tela,
    quadro: Quadro,
    states: States,
    cursor: Option<Point>,
    icon: CursorIcon,
    transparente: bool,
    fechar_ao_pedir: bool,
    /// O próximo quadro precisa ser inteiro (janela nova, resize, quadro perdido).
    tudo: bool,
    /// Um movimento do mouse ainda não entregue: chegam até 1000 por segundo, entregamos no
    /// máximo um por quadro do monitor (o último).
    mover: bool,
    ultimo_mover: std::time::Instant,
}

impl<A: App> Runtime<A> {
    fn run_task(&mut self, t: Task<A::Msg>) {
        for a in t.0 {
            let proxy = self.proxy.clone();
            match a {
                Action::Future(f) => self.pool.spawn_ok(async move {
                    let _ = proxy.send_event(f.await);
                }),
                Action::Stream(mut s) => self.pool.spawn_ok(async move {
                    while let Some(m) = s.next().await {
                        if proxy.send_event(m).is_err() {
                            break;
                        }
                    }
                }),
                Action::Window(id, c) => self.window_cmd(id, c),
                Action::Open(id, s) => {
                    if !self.janelas.iter().any(|j| j.id == id) && !self.abrir.iter().any(|(i, _)| *i == id) {
                        self.abrir.push((id, s));
                    }
                }
            }
        }
    }

    fn window_cmd(&mut self, id: WindowId, c: WindowCmd) {
        if c == WindowCmd::Exit {
            self.sair = true;
            return;
        }
        let Some(i) = self.janelas.iter().position(|j| j.id == id) else {
            if id == WindowId::MAIN || self.abrir.iter().any(|(a, _)| *a == id) {
                self.pendentes.push((id, c));
            }
            return;
        };
        let w = &self.janelas[i].w;
        match c {
            WindowCmd::Drag => drop(w.drag_window()),
            WindowCmd::DragResize(d) => drop(w.drag_resize_window(d)),
            WindowCmd::Minimize => w.set_minimized(true),
            WindowCmd::ToggleMaximize => w.set_maximized(!w.is_maximized()),
            WindowCmd::ToggleFullscreen => w.set_fullscreen(w.fullscreen().is_none().then_some(winit::window::Fullscreen::Borderless(None))),
            WindowCmd::Close => self.fechar(i),
            WindowCmd::Exit => self.sair = true,
            WindowCmd::Resize(s) => drop(w.request_inner_size(LogicalSize::new(s.width, s.height))),
            WindowCmd::Title(t) => w.set_title(&t),
            WindowCmd::Blur(b) => w.set_blur(b),
            WindowCmd::Level(l) => w.set_window_level(l),
            WindowCmd::Focus => {
                w.set_minimized(false);
                w.focus_window();
            }
        }
    }

    /// Some com a janela `i`. Sem `run_in_background`, fechar a principal (ou a última) acaba o app.
    fn fechar(&mut self, i: usize) {
        let principal = self.janelas[i].id == WindowId::MAIN;
        if !self.settings.run_in_background && (principal || self.janelas.len() == 1) {
            self.sair = true;
            return;
        }
        self.janelas.remove(i);
    }

    fn update(&mut self, msgs: Vec<A::Msg>) {
        for m in msgs {
            let t = self.app.update(m);
            self.run_task(t);
        }
        // O estado é um só: toda janela pode ter mudado (as que não mudaram gravam e não pintam nada).
        for j in &self.janelas {
            j.w.request_redraw();
        }
    }

    /// Abre as janelas pedidas (a principal no `resumed`).
    fn abrir_janelas(&mut self, el: &ActiveEventLoop) {
        for (id, s) in std::mem::take(&mut self.abrir) {
            let w = match el.create_window(atributos(&s)) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    self.erro = Some(Error::Window(e.to_string()));
                    if id == WindowId::MAIN {
                        return el.exit();
                    }
                    continue;
                }
            };
            let tela = match &self.gpu {
                Some(g) => g.tela(w.clone(), s.transparent),
                None => Gpu::new(w.clone(), s.transparent).map(|(g, t)| {
                    self.gpu = Some(g);
                    t
                }),
            };
            let tela = match tela {
                Ok(t) => t,
                Err(e) => {
                    self.erro = Some(Error::Gpu(e));
                    if id == WindowId::MAIN {
                        return el.exit();
                    }
                    continue;
                }
            };
            if self.janelas.is_empty() {
                // SAFETY: a conexão com o display vive até o loop acabar; o `exiting` solta a área de
                // transferência antes disso.
                if let Ok(c) = unsafe { window_clipboard::Clipboard::connect(&*w) } {
                    self.clipboard = Box::new(Sistema(c));
                }
            }
            // O X11 do GNOME ignora a posição da criação (o winit não marca como escolhida pelo
            // programa); mover depois de aberta ele obedece.
            if let Some(p) = s.position {
                w.set_outer_position(winit::dpi::LogicalPosition::new(p.x, p.y));
            }
            w.request_redraw();
            self.janelas.push(Janela { id, w, tela, quadro: Quadro::default(), states: States::default(), cursor: None, icon: CursorIcon::Default, transparente: s.transparent, fechar_ao_pedir: s.close_on_request, tudo: true, mover: false, ultimo_mover: std::time::Instant::now() });
            let (dela, outras) = std::mem::take(&mut self.pendentes).into_iter().partition(|(i, _)| *i == id);
            self.pendentes = outras;
            for (i, c) in dela {
                self.window_cmd(i, c);
            }
        }
    }

    fn logical_size(w: &Window) -> Size {
        let s = w.inner_size().to_logical::<f32>(w.scale_factor());
        Size::new(s.width, s.height)
    }

    /// Passa o evento pela árvore da janela `i` e depois pelo `App::on_window_event`.
    fn event(&mut self, i: usize, ev: Event) {
        if self.settings.debug && matches!(&ev, Event::Key { key: crate::Key::Named(crate::NamedKey::F12), pressed: true, .. }) {
            self.inspecionar = !self.inspecionar;
            return self.update(Vec::new());
        }
        let theme = self.app.theme();
        let j = &mut self.janelas[i];
        let (id, size) = (j.id, Self::logical_size(&j.w));
        let mut msgs = Vec::new();
        let mut cmds = Vec::new();
        let redraw = {
            let el = self.app.view_window(id);
            let node = widget::layout(&el, size, &mut self.renderer, &j.states, &theme);
            let mut cx = EventCx { renderer: &mut self.renderer, states: &mut j.states, cursor: j.cursor, messages: &mut msgs, window: &mut cmds, redraw: false, clipboard: &mut *self.clipboard };
            widget::dispatch(&el, &node, &ev, &mut cx);
            cx.redraw
        };
        // O mouse andou: o quadro é gravado e comparado; se nada mudou, nem pinta.
        let mexeu = matches!(ev, Event::CursorMoved(_) | Event::CursorLeft);
        msgs.extend(self.app.on_window_event(id, &ev));
        for c in cmds {
            self.window_cmd(id, c);
        }
        if !msgs.is_empty() || redraw || mexeu || matches!(ev, Event::Resized(_)) {
            self.update(msgs);
        }
    }

    fn entregar_mover(&mut self, i: usize) {
        let j = &mut self.janelas[i];
        if std::mem::take(&mut j.mover) {
            j.ultimo_mover = std::time::Instant::now();
            if let Some(p) = j.cursor {
                self.event(i, Event::CursorMoved(p));
            }
        }
    }

    fn redraw(&mut self, i: usize) {
        let theme = self.app.theme();
        let j = &mut self.janelas[i];
        let w = j.w.clone();
        let size = Self::logical_size(&w);
        let fundo = if j.transparente { Color::TRANSPARENT } else { theme.background };
        let escala = w.scale_factor() as f32;
        let tudo = std::mem::replace(&mut j.tudo, false);
        self.renderer.trocar(&mut j.quadro);
        let t0 = std::time::Instant::now();
        let el = self.app.view_window(j.id);
        let t1 = t0.elapsed();
        let node = widget::layout(&el, size, &mut self.renderer, &j.states, &theme);
        let t2 = t0.elapsed();
        let (states, cursor, inspecionar) = (&j.states, j.cursor, self.inspecionar);
        let pintar = |r: &mut Renderer| widget::draw(&el, &node, r, states, &theme, cursor, inspecionar);
        // Primeiro grava (barato: só áreas e hashes) e compara com o quadro na tela.
        self.renderer.record();
        let icon = pintar(&mut self.renderer);
        let mudou = self.renderer.changes();
        if std::env::var_os("PELICANO_DANO").is_some() { eprintln!("[dano] {mudou:?} view {t1:?} layout {t2:?} gravar {:?}", t0.elapsed()); }
        let regioes = match mudou {
            _ if tudo => None,
            Changes::None => Some(Vec::new()),
            Changes::All => None,
            Changes::Regions(r) => Some(r),
        };
        let parcial = regioes.as_ref().is_some_and(|rs| rs.iter().all(|r| self.renderer.begin_parcial(size, escala, fundo, *r) && {
            pintar(&mut self.renderer);
            self.renderer.end();
            true
        }));
        if !parcial {
            self.renderer.begin(size, escala, fundo);
            pintar(&mut self.renderer);
        }
        drop(el);
        if icon != j.icon {
            j.icon = icon;
            w.set_cursor(icon);
        }
        // Quadro perdido (superfície velha no meio do resize): pede outro, senão a janela
        // fica com o conteúdo antigo até o próximo evento.
        let nada = parcial && regioes.is_some_and(|r| r.is_empty());
        if !nada && self.gpu.as_ref().is_some_and(|g| !j.tela.present(g, &self.renderer.pixmap, self.renderer.changed_rows())) {
            j.tudo = true;
            w.request_redraw();
        }
        self.renderer.trocar(&mut j.quadro);
    }
}

fn atributos(s: &Settings) -> winit::window::WindowAttributes {
    #[allow(unused_mut)]
    let mut attrs = Window::default_attributes()
        .with_title(&s.title)
        .with_inner_size(LogicalSize::new(s.size.width, s.size.height))
        .with_decorations(s.decorations)
        .with_transparent(s.transparent)
        .with_blur(s.blur)
        .with_resizable(s.resizable)
        .with_window_icon(s.icon.clone());
    if let Some(p) = s.position {
        attrs = attrs.with_position(winit::dpi::LogicalPosition::new(p.x, p.y));
    }
    if let Some(m) = s.min_size {
        attrs = attrs.with_min_inner_size(LogicalSize::new(m.width, m.height));
    }
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    {
        // O nome vale para os dois (o WM_CLASS do X11 e o app_id do Wayland): é por ele que a dock junta as janelas.
        if let Some(id) = &s.app_id {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs = attrs.with_name(id, id);
        }
        if s.skip_taskbar {
            use winit::platform::x11::{WindowAttributesExtX11, WindowType};
            attrs = attrs.with_x11_window_type(vec![WindowType::Utility]);
        }
    }
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs = attrs.with_skip_taskbar(s.skip_taskbar);
    }
    attrs
}

impl<A: App> ApplicationHandler<A::Msg> for Runtime<A> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if std::mem::replace(&mut self.iniciou, true) {
            return;
        }
        if !self.settings.start_hidden {
            self.abrir.insert(0, (WindowId::MAIN, self.settings.clone()));
        }
        self.abrir_janelas(el);
    }

    // shortcut: entre dois quadros só o último ponto do mouse chega aos widgets; um traço de caneta quer todos (guardar a lista quando houver canvas de desenho).
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.sair {
            return el.exit();
        }
        if !self.janelas.iter().any(|j| j.mover) {
            return el.set_control_flow(ControlFlow::Wait);
        }
        let janela = self.janelas.first().map(|j| j.w.clone());
        let intervalo = *self.intervalo.get_or_insert_with(|| {
            let hz = janela.and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).unwrap_or(60_000);
            std::time::Duration::from_micros(1_000_000_000 / hz.max(1000) as u64)
        });
        let agora = std::time::Instant::now();
        let mut prox: Option<std::time::Instant> = None;
        let mut i = 0;
        while i < self.janelas.len() {
            let j = &self.janelas[i];
            if j.mover {
                let quando = j.ultimo_mover + intervalo;
                if agora >= quando {
                    self.entregar_mover(i);
                } else {
                    prox = Some(prox.map_or(quando, |p| p.min(quando)));
                }
            }
            i += 1;
        }
        self.abrir_janelas(el);
        el.set_control_flow(prox.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        if self.sair {
            el.exit();
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.clipboard = Box::new(widget::MemoryClipboard::default());
    }

    fn user_event(&mut self, el: &ActiveEventLoop, m: A::Msg) {
        self.update(vec![m]);
        self.abrir_janelas(el);
        if self.sair {
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, wid: winit::window::WindowId, we: WindowEvent) {
        let Some(i) = self.janelas.iter().position(|j| j.w.id() == wid) else { return };
        let j = &mut self.janelas[i];
        let escala = j.w.scale_factor();
        let ev = match we {
            WindowEvent::RedrawRequested => return self.redraw(i),
            WindowEvent::CloseRequested => {
                if j.fechar_ao_pedir {
                    // Avisa o app antes de a janela sumir.
                    let id = j.id;
                    let m = self.app.on_window_event(id, &Event::CloseRequested);
                    self.fechar(i);
                    self.update(m.into_iter().collect());
                    self.abrir_janelas(el);
                    if self.sair {
                        el.exit();
                    }
                    return;
                }
                Some(Event::CloseRequested)
            }
            WindowEvent::Resized(s) => {
                if let Some(g) = &self.gpu {
                    j.tela.resize(g, s.width, s.height);
                }
                j.tudo = true;
                let l = s.to_logical::<f32>(escala);
                Some(Event::Resized(Size::new(l.width, l.height)))
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(escala);
                j.cursor = Some(Point::new(p.x, p.y));
                j.mover = true;
                None
            }
            WindowEvent::Moved(p) => {
                let l = p.to_logical::<f32>(escala);
                Some(Event::Moved(Point::new(l.x, l.y)))
            }
            WindowEvent::CursorLeft { .. } => {
                j.cursor = None;
                Some(Event::CursorLeft)
            }
            WindowEvent::MouseInput { state, button, .. } => Some(match state {
                ElementState::Pressed => Event::MousePressed(button),
                ElementState::Released => Event::MouseReleased(button),
            }),
            WindowEvent::MouseWheel { delta, .. } => Some(match delta {
                MouseScrollDelta::LineDelta(x, y) => Event::Wheel { dx: x * 48.0, dy: y * 48.0, modifiers: self.modifiers },
                MouseScrollDelta::PixelDelta(p) => Event::Wheel { dx: (p.x / escala) as f32, dy: (p.y / escala) as f32, modifiers: self.modifiers },
            }),
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                None
            }
            WindowEvent::KeyboardInput { event, .. } => Some(Event::Key { key: event.logical_key.clone(), text: event.text.map(|t| t.to_string()), modifiers: self.modifiers, pressed: event.state.is_pressed() }),
            WindowEvent::Focused(f) => Some(Event::Focused(f)),
            WindowEvent::ScaleFactorChanged { .. } => {
                j.tudo = true;
                self.intervalo = None;
                j.w.request_redraw();
                None
            }
            _ => None,
        };
        if let Some(ev) = ev {
            // Clique e tecla veem o mouse onde ele está.
            self.entregar_mover(i);
            // Com o modo debug, o que chega da janela vai para o stderr (para achar tecla ou clique perdido).
            if self.settings.debug && !matches!(ev, Event::CursorMoved(_)) {
                eprintln!("[pelicano] {ev:?} cursor={:?}", self.janelas.get(i).and_then(|j| j.cursor));
            }
            if i < self.janelas.len() {
                self.event(i, ev);
            }
        }
        self.abrir_janelas(el);
        if self.sair {
            el.exit();
        }
    }
}

struct Sistema(window_clipboard::Clipboard);

impl widget::Clipboard for Sistema {
    fn read(&mut self) -> Option<String> {
        self.0.read().ok()
    }
    fn write(&mut self, s: String) {
        let _ = self.0.write(s);
    }
}

/// Só leva o quadro pronto para a tela: uma textura e um triângulo que a cobre. O aparelho é
/// um só; cada janela tem a sua [`Tela`].
struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

struct Tela {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    textura: Option<(wgpu::Texture, wgpu::BindGroup)>,
    /// O último quadro não chegou à tela: não pede de novo em seguida (janela minimizada
    /// falha sempre e viraria um laço).
    falhou: bool,
}

const SHADER: &str = r#"
@group(0) @binding(0) var quadro: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(quadro, vec2<i32>(p.xy), 0);
}
"#;

impl Gpu {
    /// O aparelho, escolhido para a primeira janela, e a tela dela.
    fn new(w: Arc<Window>, transparente: bool) -> Result<(Self, Tela), String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(w.clone()).map_err(|e| e.to_string())?;
        let adapter = futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).ok_or("nenhum adaptador gráfico")?;
        let (device, queue) = futures::executor::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()) },
            None,
        ))
        .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        // O quadro já vem em sRGB: um formato "Srgb" converteria de novo.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or(caps.formats.first().copied()).ok_or("superfície sem formato")?;
        let modulo = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            }],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&layout], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &modulo, entry_point: "vs", buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState { module: &modulo, entry_point: "fs", targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            multiview: None,
        });
        let g = Gpu { instance, adapter, device, queue, format, pipeline, layout };
        let t = g.configurar(surface, &w, transparente)?;
        Ok((g, t))
    }

    /// A tela de mais uma janela, no mesmo aparelho.
    fn tela(&self, w: Arc<Window>, transparente: bool) -> Result<Tela, String> {
        let surface = self.instance.create_surface(w.clone()).map_err(|e| e.to_string())?;
        self.configurar(surface, &w, transparente)
    }

    fn configurar(&self, surface: wgpu::Surface<'static>, w: &Window, transparente: bool) -> Result<Tela, String> {
        let caps = surface.get_capabilities(&self.adapter);
        // shortcut: o pipeline é de um formato só; uma janela num monitor de outro aparelho não abre (um pipeline por formato quando acontecer).
        if !caps.formats.contains(&self.format) {
            return Err(format!("a superfície não aceita {:?}", self.format));
        }
        let alpha_mode = if transparente && caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) { wgpu::CompositeAlphaMode::PreMultiplied } else { caps.alpha_modes[0] };
        let tamanho = w.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.format,
            width: tamanho.width.max(1),
            height: tamanho.height.max(1),
            // Só desenhamos quando algo muda: Mailbox (sem fila, sem rasgo) e um quadro de folga
            // dão a menor demora entre a rodinha e a tela; o FIFO segurava até dois quadros.
            present_mode: if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) { wgpu::PresentMode::Mailbox } else { wgpu::PresentMode::AutoVsync },
            desired_maximum_frame_latency: 1,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&self.device, &config);
        Ok(Tela { surface, config, textura: None, falhou: false })
    }
}

impl Tela {
    fn resize(&mut self, g: &Gpu, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&g.device, &self.config);
        }
    }

    /// `false` se o quadro não chegou à tela e vale tentar de novo (só na primeira falha seguida).
    fn present(&mut self, g: &Gpu, pm: &tiny_skia::Pixmap, linhas: Option<(u32, u32)>) -> bool {
        let (w, h) = (pm.width(), pm.height());
        if (w, h) != (self.config.width, self.config.height) {
            self.resize(g, w, h);
        }
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        // Textura nova (ou quadro anterior perdido) precisa do quadro todo; senão só as linhas que mudaram.
        let mut linhas = linhas.filter(|_| !self.falhou).unwrap_or((0, h));
        if self.textura.as_ref().map_or(true, |(t, _)| t.size() != extent) {
            linhas = (0, h);
            let t = g.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = t.create_view(&Default::default());
            let bg = g.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &g.layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }] });
            self.textura = Some((t, bg));
        }
        let (t, bg) = self.textura.as_ref().expect("criada acima");
        let (y0, y1) = (linhas.0.min(h), linhas.1.min(h));
        g.queue.write_texture(
            wgpu::ImageCopyTexture { texture: t, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: y0, z: 0 }, aspect: wgpu::TextureAspect::All },
            &pm.data()[(y0 * 4 * w) as usize..(y1 * 4 * w) as usize],
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(y1 - y0) },
            wgpu::Extent3d { height: y1 - y0, ..extent },
        );
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            // Superfície velha (janela mudou de tamanho no meio): reconfigura e espera o próximo quadro.
            Err(_) => {
                self.surface.configure(&g.device, &self.config);
                return std::mem::replace(&mut self.falhou, true);
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&g.pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
        }
        g.queue.submit([enc.finish()]);
        frame.present();
        self.falhou = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::widget::{self, button, container, Element};
    use crate::render::Quadro;
    use crate::{Changes, Color, Point, Renderer, Size, States, Theme};

    /// Grava um quadro com o mouse em `cursor` e diz o que mudou desde o anterior.
    fn quadro(r: &mut Renderer, el: &Element<'_, ()>, cursor: Option<Point>) -> Changes {
        let (st, tema, tam) = (States::default(), Theme::dark(), Size::new(400.0, 300.0));
        r.begin(tam, 1.0, Color::BLACK);
        let n = widget::layout(el, tam, r, &st, &tema);
        r.record();
        widget::draw(el, &n, r, &st, &tema, cursor, false);
        r.changes()
    }

    #[test]
    fn mouse_so_redesenha_o_botao_em_que_entra() {
        let el: Element<()> = container(button("ok").on_press(())).padding(50.0).into();
        let mut r = Renderer::new();
        assert_eq!(quadro(&mut r, &el, Some(Point::new(5.0, 5.0))), Changes::All);
        // Andar no vazio não redesenha nada.
        assert_eq!(quadro(&mut r, &el, Some(Point::new(20.0, 20.0))), Changes::None);
        // Entrar no botão redesenha só perto dele.
        let Changes::Regions(rs) = quadro(&mut r, &el, Some(Point::new(55.0, 55.0))) else { panic!("devia ser regiões") };
        assert_eq!(rs.len(), 1);
        assert!(rs[0].contains(Point::new(55.0, 55.0)) && rs[0].width < 200.0 && rs[0].height < 120.0, "{:?}", rs[0]);
        // Sair de novo também.
        assert!(matches!(quadro(&mut r, &el, None), Changes::Regions(_)));
        assert_eq!(quadro(&mut r, &el, None), Changes::None);
    }

    /// Duas janelas no mesmo renderizador: cada uma compara com o próprio quadro anterior.
    #[test]
    fn cada_janela_guarda_o_seu_quadro() {
        let (st, tema, tam) = (States::default(), Theme::dark(), Size::new(200.0, 100.0));
        let a: Element<()> = container(button("a").on_press(())).padding(10.0).into();
        let b: Element<()> = container(button("outro botão").on_press(())).padding(40.0).into();
        let mut r = Renderer::new();
        let (mut qa, mut qb) = (Quadro::default(), Quadro::default());
        let mut passo = |q: &mut Quadro, el: &Element<'_, ()>| {
            r.trocar(q);
            let n = widget::layout(el, tam, &mut r, &st, &tema);
            r.record();
            widget::draw(el, &n, &mut r, &st, &tema, None, false);
            let c = r.changes();
            r.begin(tam, 1.0, Color::BLACK);
            widget::draw(el, &n, &mut r, &st, &tema, None, false);
            let px = r.pixmap.data().to_vec();
            r.trocar(q);
            (c, px)
        };
        assert_eq!(passo(&mut qa, &a).0, Changes::All);
        assert_eq!(passo(&mut qb, &b).0, Changes::All);
        // Voltar à primeira não acha diferença nenhuma, e o quadro dela é o dela.
        let (c, px_a) = passo(&mut qa, &a);
        assert_eq!(c, Changes::None);
        let (c, px_b) = passo(&mut qb, &b);
        assert_eq!(c, Changes::None);
        assert_ne!(px_a, px_b);
    }
}
