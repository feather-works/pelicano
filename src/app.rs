//! A janela, o laço de eventos, as tarefas assíncronas e a apresentação do quadro.
use crate::access::{accesskit, Access};
use crate::widget::{self, CursorIcon, Element, EventCx};
use crate::render::{gpu, Quadro};
use crate::{Changes, Color, Event, Modifiers, Point, Renderer, ResizeDirection, Size, States, Theme};
use futures::future::{AbortHandle, Abortable, BoxFuture};
use futures::stream::BoxStream;
use futures::{FutureExt, Stream, StreamExt};
use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::raw_window_handle::RawWindowHandle;
use winit::window::{Icon, Window, WindowLevel};

/// Um aplicativo: estado, como ele muda (`update`) e como aparece (`view`).
pub trait App: Sized + 'static {
    type Msg: Clone + Send + 'static;
    fn update(&mut self, msg: Self::Msg) -> Task<Self::Msg>;
    fn view(&self) -> Element<'_, Self::Msg>;
    /// O trabalho que dura enquanto o estado pedir (processo, conexão, relógio). Chamado depois
    /// de cada `update`; veja [`Subscription`].
    fn subscription(&self) -> Subscription<Self::Msg> {
        Subscription::none()
    }
    fn theme(&self) -> Theme {
        Theme::dark()
    }
    /// Todo evento da janela passa aqui também (atalhos, fechar, redimensionar).
    fn on_event(&self, _ev: &Event) -> Option<Self::Msg> {
        None
    }
    /// Como `on_window_event`, mas só com o que nenhum widget usou: atalhos globais que não
    /// disparam enquanto se digita num campo.
    fn on_unhandled_event(&self, id: WindowId, ev: &Event) -> Option<Self::Msg> {
        let _ = (id, ev);
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
    /// Guarda o texto já preparado de um quadro para o outro (só o da tela, 4 quadros). Rolar
    /// fica ~2× mais rápido, com um pouco mais de RAM. Desligado (padrão), o texto é reaproveitado
    /// só dentro do quadro.
    pub text_cache: bool,
    /// Desenha na GPU (o padrão). Sem placa de vídeo de verdade (só um rasterizador em software,
    /// como o llvmpipe), ou com `PELICANO_GPU=0`, desenha na CPU; `PELICANO_GPU=1` força a GPU.
    /// O quadro com algo que a GPU ainda não faz (camada, vidro, curva) sai na CPU sozinho.
    /// Sem aparelho gráfico nenhum (ou `PELICANO_GPU=nenhuma`), a janela é mostrada sem wgpu.
    pub gpu: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { title: "Pelicano".into(), size: Size::new(1024.0, 720.0), min_size: None, decorations: true, transparent: false, blur: false, resizable: true, close_on_request: true, app_id: None, icon: None, position: None, skip_taskbar: false, x11: false, run_in_background: false, start_hidden: false, debug: false, text_cache: false, gpu: true }
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
    /// Tela cheia (`true`) ou janela (`false`), sem alternar.
    Fullscreen(bool),
    Maximize(bool),
    /// Põe o canto da janela em `p` (lógico, na área de trabalho).
    /// shortcut: o Wayland ignora, como em `Settings::position`.
    Move(Point),
    /// Segue um roteiro nesta janela, com o cursor desenhado (mesma língua do
    /// `PELICANO_ROTEIRO`, ver [`validar_roteiro`]). Com um roteiro já andando na mesma
    /// janela, os passos entram no fim; o cursor continua de onde o último parou.
    Roteiro(String),
}

/// Confere um roteiro (o texto de [`WindowCmd::Roteiro`]) sem rodar: o erro diz a linha.
pub fn validar_roteiro(texto: &str) -> Result<(), String> {
    crate::roteiro::ler(texto).map(|_| ())
}

/// Ordens para um widget, pelo `id` dele.
#[derive(Clone, Copy)]
pub(crate) enum Op {
    Focus(u64),
    /// Deslocamento absoluto, ou relativo (0 a 1) quando o `bool` diz.
    Scroll(u64, Point, bool),
}

type Leitura<M> = Box<dyn FnOnce(Option<String>) -> M + Send>;
type Alca<M> = Box<dyn FnOnce(RawWindowHandle) -> M + Send>;

enum Action<M> {
    Future(BoxFuture<'static, M>),
    Stream(BoxStream<'static, M>),
    Window(WindowId, WindowCmd),
    Open(WindowId, Settings),
    Widget(Op),
    Copiar(String),
    Colar(Leitura<M>),
    Alca(WindowId, Alca<M>),
    Evento(WindowId, Event),
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
    /// Abre outra janela; ela mostra o [`App::view_window`] de `id`. Sem
    /// [`Settings::run_in_background`], fechar a principal fecha todas.
    pub fn open_window(id: WindowId, settings: Settings) -> Self {
        Task(vec![Action::Open(id, settings)])
    }
    /// Rola o `scrollable(..).id(id)` até o deslocamento `offset` (lógico; `f32::INFINITY` = o fim).
    pub fn scroll_to(id: impl Hash, offset: Point) -> Self {
        Task(vec![Action::Widget(Op::Scroll(crate::id_hash(&id), offset, false))])
    }
    /// Rola até a fração `rel` (0 = começo, 1 = fim) de cada eixo.
    pub fn snap_to(id: impl Hash, rel: Point) -> Self {
        Task(vec![Action::Widget(Op::Scroll(crate::id_hash(&id), rel, true))])
    }
    /// Põe o foco no campo `.id(id)` (já desenhado).
    pub fn focus(id: impl Hash) -> Self {
        Task(vec![Action::Widget(Op::Focus(crate::id_hash(&id)))])
    }
    /// Copia `texto` para a área de transferência.
    pub fn write_clipboard(texto: impl Into<String>) -> Self {
        Task(vec![Action::Copiar(texto.into())])
    }
    /// Lê a área de transferência e manda `f(texto)`.
    pub fn read_clipboard(f: impl FnOnce(Option<String>) -> M + Send + 'static) -> Self {
        Task(vec![Action::Colar(Box::new(f))])
    }
    /// Chama `f` na thread da janela `id` com a alça crua dela (o `run_with_handle` do Iced):
    /// ligar leitura da caneta, subclasse no Windows, o id X11. Antes de ela abrir, espera.
    pub fn window_handle(id: WindowId, f: impl FnOnce(RawWindowHandle) -> M + Send + 'static) -> Self {
        Task(vec![Action::Alca(id, Box::new(f))])
    }
    /// Passa `ev` pela árvore da janela `id` como se viesse do sistema (entrada sintética,
    /// automação). `Event::CursorMoved` também move o cursor que os widgets veem.
    pub fn window_event(id: WindowId, ev: Event) -> Self {
        Task(vec![Action::Evento(id, ev)])
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
                    Action::Widget(o) => Action::Widget(o),
                    Action::Copiar(t) => Action::Copiar(t),
                    Action::Colar(g) => {
                        let f = f.clone();
                        Action::Colar(Box::new(move |t| f(g(t))))
                    }
                    Action::Alca(id, g) => {
                        let f = f.clone();
                        Action::Alca(id, Box::new(move |h| f(g(h))))
                    }
                    Action::Evento(id, e) => Action::Evento(id, e),
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
            .map(|a| {
                let mut abortavel = || {
                    let (h, reg) = AbortHandle::new_pair();
                    alcas.push(h);
                    reg
                };
                match a {
                    // Futuro cancelado não manda mensagem nenhuma.
                    Action::Future(x) => Action::Stream(futures::stream::once(Abortable::new(x, abortavel())).filter_map(|r| futures::future::ready(r.ok())).boxed()),
                    Action::Stream(x) => Action::Stream(Abortable::new(x, abortavel()).boxed()),
                    w => w,
                }
            })
            .collect();
        (Task(acoes), Handle(alcas))
    }
}

/// O que chega ao laço de fora da janela: as mensagens das tarefas e os pedidos do leitor de tela.
enum Evento<M> {
    App(M),
    /// A resposta de um [`Action::Future`]: ao sair, o laço espera essas (o salvar do fechamento).
    Feito(M),
    Janela(WindowId, Event),
    Acesso(accesskit_winit::Event),
}

impl<M> From<accesskit_winit::Event> for Evento<M> {
    fn from(e: accesskit_winit::Event) -> Self {
        Evento::Acesso(e)
    }
}

/// Fluxos com identidade (o `Subscription` do Iced). Cada um roda enquanto o
/// [`App::subscription`] devolver o mesmo id: quando o id some, o fluxo é derrubado, e o que
/// ele segura cai junto (um processo com `kill_on_drop`, uma conexão); outro id começa outro.
/// Tudo roda no tokio: `tokio::process`, `tokio::time` e cia. funcionam dentro dele.
pub struct Subscription<M>(Vec<(u64, BoxStream<'static, M>)>);

impl<M: Send + 'static> Subscription<M> {
    pub fn none() -> Self {
        Subscription(Vec::new())
    }
    /// `stream` só é usado na primeira vez que o id aparece; nas outras ele é descartado sem rodar.
    pub fn run_with_id(id: impl Hash, stream: impl Stream<Item = M> + Send + 'static) -> Self {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut h);
        Subscription(vec![(h.finish(), stream.boxed())])
    }
    pub fn batch(subs: impl IntoIterator<Item = Subscription<M>>) -> Self {
        Subscription(subs.into_iter().flat_map(|s| s.0).collect())
    }
    /// Converte as mensagens. O id passa a contar também o tipo de `f`: o mesmo relógio
    /// mapeado para duas mensagens diferentes vira dois.
    /// `f` não pode capturar nada: com o mesmo id ela não é trocada, e o valor capturado ficaria
    /// o da primeira vez. O que muda vai no id ([`run_with_id`](Self::run_with_id)).
    pub fn map<N: Send + 'static, F: Fn(M) -> N + Send + Sync + 'static>(self, f: F) -> Subscription<N> {
        debug_assert_eq!(std::mem::size_of::<F>(), 0, "Subscription::map: a closure não pode capturar nada (ficaria com o valor velho); ponha o dado no id");
        let f = Arc::new(f);
        Subscription(
            self.0
                .into_iter()
                .map(|(id, s)| {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    (id, std::any::TypeId::of::<F>()).hash(&mut h);
                    let f = f.clone();
                    (h.finish(), s.map(move |m| f(m)).boxed())
                })
                .collect(),
        )
    }
}

impl Subscription<Instant> {
    /// A hora, a cada `intervalo` (o `time::every` do Iced).
    pub fn every(intervalo: Duration) -> Self {
        let primeiro = tokio::time::Instant::now() + intervalo;
        let s = futures::stream::unfold(primeiro, move |quando| async move {
            tokio::time::sleep_until(quando).await;
            // Depois de parado (suspensão), não manda de uma vez os toques perdidos.
            Some((Instant::now(), (quando + intervalo).max(tokio::time::Instant::now())))
        });
        Subscription::run_with_id(("pelicano::every", intervalo), s)
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
        EventLoop::<Evento<A::Msg>>::with_user_event().with_x11().build().ok()
    });
    #[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
    let x11: Option<Option<EventLoop<Evento<A::Msg>>>> = None;
    let event_loop = match x11.flatten() {
        Some(el) => el,
        None => EventLoop::<Evento<A::Msg>>::with_user_event().build().map_err(Error::EventLoop)?,
    };
    let tokio = tokio::runtime::Builder::new_multi_thread().thread_name("pelicano").enable_all().build().map_err(|e| Error::Window(e.to_string()))?;
    let (app, task) = init();
    let mut renderer = Renderer::new();
    renderer.cache_textos = settings.text_cache;
    let mut rt = Runtime { app, settings, proxy: event_loop.create_proxy(), tokio: tokio.handle().clone(), assinaturas: HashMap::new(), clipboard: Box::new(widget::MemoryClipboard::default()), janelas: Vec::new(), gpu: None, sem_gpu: std::env::var("PELICANO_GPU").is_ok_and(|v| v == "nenhuma"), renderer, modifiers: Modifiers::empty(), abrir: Vec::new(), pendentes: Vec::new(), alcas: Vec::new(), erro: None, sair: false, iniciou: false, inspecionar: false, intervalo: None, avisados: Default::default(), roteiro: None, roteiro_janela: WindowId::MAIN, roteiro_cursor: Point::new(40.0, 40.0), gravar: std::env::var("PELICANO_GRAVAR").ok(), gravador: None, trabalhos: 0, saindo: None };
    if let Some(arq) = std::env::var_os("PELICANO_ROTEIRO") {
        let texto = std::fs::read_to_string(&arq).map_err(|e| Error::Window(format!("{}: {e}", arq.to_string_lossy())))?;
        rt.roteiro = Some(crate::roteiro::Roteiro::new(crate::roteiro::ler(&texto).map_err(Error::Window)?));
    }
    if std::env::var_os("PELICANO_DEBUG").is_some_and(|v| v != "0") {
        rt.settings.debug = true;
        rt.inspecionar = true;
    }
    rt.run_task(task);
    rt.assinar();
    let fim = event_loop.run_app(&mut rt);
    rt.parar_assinaturas();
    // A janela já fechou: as tarefas têm um segundo para soltar o que seguram.
    tokio.shutdown_timeout(Duration::from_secs(1));
    fim.map_err(Error::EventLoop)?;
    rt.erro.map_or(Ok(()), Err)
}

struct Runtime<A: App> {
    app: A,
    /// As da janela principal (o `debug` vale para todas).
    settings: Settings,
    proxy: EventLoopProxy<Evento<A::Msg>>,
    tokio: tokio::runtime::Handle,
    /// As assinaturas rodando, pelo id ([`Subscription`]).
    assinaturas: HashMap<u64, tokio::task::AbortHandle>,
    /// A do sistema, ligada quando a primeira janela abre (antes disso, na memória).
    clipboard: Box<dyn widget::Clipboard>,
    janelas: Vec<Janela>,
    gpu: Option<Gpu>,
    /// Nenhum aparelho gráfico (ou `PELICANO_GPU=nenhuma`): as janelas mostram pela [`Saida::Cpu`].
    sem_gpu: bool,
    /// Um só para todas as janelas: as fontes e os caches são o que pesa.
    renderer: Renderer,
    modifiers: Modifiers,
    /// Janelas pedidas: abrem no próximo evento (criar janela precisa do laço).
    abrir: Vec<(WindowId, Settings)>,
    /// Ordens para janelas que ainda não abriram.
    pendentes: Vec<(WindowId, WindowCmd)>,
    /// Pedidos de alça ([`Task::window_handle`]) para janelas que ainda não abriram.
    alcas: Vec<(WindowId, Alca<A::Msg>)>,
    erro: Option<Error>,
    sair: bool,
    /// O `resumed` já abriu (ou deixou fechada) a principal.
    iniciou: bool,
    /// O inspetor está à mostra (F12, só com `Settings::debug`).
    inspecionar: bool,
    /// Um quadro do monitor (perguntar a cada evento custa caro); some quando a janela muda de tela.
    intervalo: Option<std::time::Duration>,
    /// Os motivos de quadro na CPU já contados no `PELICANO_DANO` (um aviso por motivo).
    avisados: std::collections::HashSet<&'static str>,
    /// `PELICANO_ROTEIRO`: a demonstração que a janela principal segue ([`crate::roteiro`]).
    roteiro: Option<crate::roteiro::Roteiro>,
    /// Em que janela o roteiro anda (o `PELICANO_ROTEIRO` é sempre a principal).
    roteiro_janela: WindowId,
    /// Onde o cursor desenhado parou, para o roteiro seguinte sair dali.
    roteiro_cursor: Point,
    /// `PELICANO_GRAVAR`: para onde vai o vídeo; o gravador abre no primeiro quadro (com o tamanho).
    gravar: Option<String>,
    gravador: Option<crate::roteiro::Gravador>,
    /// Os [`Action::Future`] cuja mensagem ainda não chegou.
    trabalhos: usize,
    /// Quando o app pediu para sair: as tarefas têm até [`ESPERA_AO_SAIR`] para terminar.
    saindo: Option<std::time::Instant>,
}

/// Quanto o laço espera, ao sair, as tarefas que o app deixou rodando (salvar no fechamento).
const ESPERA_AO_SAIR: Duration = Duration::from_secs(5);

/// Cada janela aberta: o que é dela e não pode ser dividido.
struct Janela {
    id: WindowId,
    w: Arc<Window>,
    tela: Saida,
    quadro: Quadro,
    states: States,
    cursor: Option<Point>,
    icon: CursorIcon,
    transparente: bool,
    fechar_ao_pedir: bool,
    /// O próximo quadro precisa ser inteiro (janela nova, resize, quadro perdido).
    tudo: bool,
    /// O último quadro saiu na GPU: o `pixmap` desta janela ficou para trás, e um quadro na CPU
    /// precisa ser inteiro.
    na_gpu: bool,
    /// Um movimento do mouse ainda não entregue: chegam até 1000 por segundo, entregamos no
    /// máximo um por quadro do monitor (o último).
    mover: bool,
    ultimo_mover: std::time::Instant,
    /// Uma animação pediu o próximo quadro: quando ele vem.
    proximo: Option<std::time::Instant>,
    /// A roda ainda não entregue, somada. O touchpad manda dezenas por quadro, e cada entrega
    /// monta a árvore de novo: sem somar, a rolagem ficava atrás do dedo (a barra, que vem pelo
    /// `mover`, já era uma por quadro).
    roda: Option<Event>,
    /// O que falta andar dos cliques da roda do mouse: vai aos pedaços, um por quadro, para a
    /// rolagem deslizar em vez de pular 48 px de uma vez.
    roda_suave: Option<Event>,
    acesso: accesskit_winit::Adapter,
    /// Um leitor de tela pediu a árvore: cada quadro manda a dele (se mudou).
    leitor: bool,
    /// A última árvore mandada: não repetir a mesma, e achar onde fica o alvo de uma ação.
    arvore: Option<accesskit::TreeUpdate>,
    /// O dedo que está fazendo as vezes de mouse ([`mouse_do_dedo`]).
    dedo: Option<u64>,
}

impl<A: App> Runtime<A> {
    fn run_task(&mut self, t: Task<A::Msg>) {
        for a in t.0 {
            let proxy = self.proxy.clone();
            match a {
                Action::Future(f) => {
                    self.trabalhos += 1;
                    drop(self.tokio.spawn(async move {
                        let _ = proxy.send_event(Evento::Feito(f.await));
                    }));
                }
                Action::Stream(s) => drop(self.espalhar(s)),
                Action::Window(id, c) => self.window_cmd(id, c),
                Action::Widget(o) => self.widget_op(o),
                Action::Copiar(t) => self.clipboard.write(t),
                Action::Colar(f) => {
                    let m = f(self.clipboard.read());
                    let _ = self.proxy.send_event(Evento::App(m));
                }
                Action::Alca(id, f) => self.alca(id, f),
                // Pelo laço, não aqui dentro: o evento chega depois do `update` que o pediu.
                Action::Evento(id, ev) => drop(self.proxy.send_event(Evento::Janela(id, ev))),
                Action::Open(id, s) => {
                    if !self.janelas.iter().any(|j| j.id == id) && !self.abrir.iter().any(|(i, _)| *i == id) {
                        self.abrir.push((id, s));
                    }
                }
            }
        }
    }

    /// A janela `id` está para abrir: pedida, ou a principal antes do `resumed`.
    fn vai_abrir(&self, id: WindowId) -> bool {
        (id == WindowId::MAIN && !self.iniciou) || self.abrir.iter().any(|(a, _)| *a == id)
    }

    fn alca(&mut self, id: WindowId, f: Alca<A::Msg>) {
        use winit::raw_window_handle::HasWindowHandle;
        let Some(j) = self.janelas.iter().find(|j| j.id == id) else {
            if self.vai_abrir(id) {
                self.alcas.push((id, f));
            }
            return;
        };
        if let Ok(h) = j.w.window_handle() {
            let m = f(h.as_raw());
            let _ = self.proxy.send_event(Evento::App(m));
        }
    }

    fn window_cmd(&mut self, id: WindowId, c: WindowCmd) {
        if c == WindowCmd::Exit {
            self.sair = true;
            return;
        }
        let Some(i) = self.janelas.iter().position(|j| j.id == id) else {
            // A principal só espera antes de abrir a primeira vez; fechada, a ordem não vale mais.
            if self.vai_abrir(id) {
                self.pendentes.push((id, c));
            }
            return;
        };
        if let WindowCmd::Roteiro(texto) = c {
            match crate::roteiro::ler(&texto) {
                Ok(passos) => match self.roteiro.as_mut().filter(|_| self.roteiro_janela == id) {
                    Some(rt) => rt.acrescentar(passos),
                    None => {
                        let mut rt = crate::roteiro::Roteiro::new(passos);
                        rt.cursor = self.roteiro_cursor;
                        self.roteiro = Some(rt);
                        self.roteiro_janela = id;
                    }
                },
                Err(e) => eprintln!("[roteiro] {e}"),
            }
            self.janelas[i].w.request_redraw();
            return;
        }
        let w = &self.janelas[i].w;
        match c {
            WindowCmd::Roteiro(_) => unreachable!("tratado acima"),
            WindowCmd::Drag => drop(w.drag_window()),
            WindowCmd::DragResize(d) => drop(w.drag_resize_window(d)),
            WindowCmd::Minimize => w.set_minimized(true),
            WindowCmd::ToggleMaximize => w.set_maximized(!w.is_maximized()),
            WindowCmd::ToggleFullscreen => w.set_fullscreen(w.fullscreen().is_none().then_some(winit::window::Fullscreen::Borderless(None))),
            WindowCmd::Close => self.fechar(i),
            WindowCmd::Exit => unreachable!("tratado no começo"),
            WindowCmd::Resize(s) => drop(w.request_inner_size(LogicalSize::new(s.width, s.height))),
            WindowCmd::Title(t) => w.set_title(&t),
            WindowCmd::Blur(b) => w.set_blur(b),
            WindowCmd::Level(l) => w.set_window_level(l),
            WindowCmd::Focus => {
                w.set_minimized(false);
                w.focus_window();
            }
            WindowCmd::Fullscreen(f) => w.set_fullscreen(f.then_some(winit::window::Fullscreen::Borderless(None))),
            WindowCmd::Maximize(m) => w.set_maximized(m),
            WindowCmd::Move(p) => w.set_outer_position(winit::dpi::LogicalPosition::new(p.x, p.y)),
        }
    }

    fn widget_op(&mut self, o: Op) {
        for j in &mut self.janelas {
            let id = match o {
                Op::Focus(id) | Op::Scroll(id, ..) => id,
            };
            let Some(key) = j.states.ids.borrow().get(&id).copied() else { continue };
            match o {
                Op::Focus(_) => j.states.focus = Some(key),
                Op::Scroll(_, p, rel) => widget::rolar(&mut j.states, key, p, rel),
            }
            j.w.request_redraw();
        }
    }

    /// Some com a janela `i`. Sem `run_in_background`, fechar a principal (ou a última) acaba o app.
    fn fechar(&mut self, i: usize) {
        let (id, principal) = (self.janelas[i].id, self.janelas[i].id == WindowId::MAIN);
        if !self.settings.run_in_background && (principal || self.janelas.len() == 1) {
            // As outras somem junto: as que cuidam do próprio fechamento (documento não salvo) ficam sabendo.
            let outras: Vec<_> = self.janelas.iter().filter(|j| j.id != id && !j.fechar_ao_pedir).map(|j| j.id).collect();
            let m: Vec<_> = outras.into_iter().filter_map(|o| self.app.on_window_event(o, &Event::CloseRequested)).collect();
            self.sair = true;
            self.update(m);
            return;
        }
        self.janelas.remove(i);
    }

    /// Roda o fluxo no tokio, mandando cada item para o laço como mensagem.
    fn espalhar(&self, mut s: BoxStream<'static, A::Msg>) -> tokio::task::AbortHandle {
        let proxy = self.proxy.clone();
        self.tokio
            .spawn(async move {
                while let Some(m) = s.next().await {
                    if proxy.send_event(Evento::App(m)).is_err() {
                        break;
                    }
                }
            })
            .abort_handle()
    }

    /// Começa as assinaturas novas e derruba as que o app não pediu de novo.
    fn assinar(&mut self) {
        let mut vivas = HashMap::new();
        for (id, s) in self.app.subscription().0 {
            if vivas.contains_key(&id) {
                continue;
            }
            let h = self.assinaturas.remove(&id).unwrap_or_else(|| self.espalhar(s));
            vivas.insert(id, h);
        }
        self.parar_assinaturas();
        self.assinaturas = vivas;
    }

    fn parar_assinaturas(&mut self) {
        for (_, h) in self.assinaturas.drain() {
            h.abort();
        }
    }

    fn update(&mut self, msgs: Vec<A::Msg>) {
        let mudou = !msgs.is_empty();
        for m in msgs {
            let t = self.app.update(m);
            self.run_task(t);
        }
        // Só mensagem muda o estado: o mouse andando não pergunta de novo.
        if mudou {
            self.assinar();
        }
        // O estado é um só: toda janela pode ter mudado (as que não mudaram gravam e não pintam nada).
        for j in &self.janelas {
            j.w.request_redraw();
        }
    }

    /// Só a principal que não abre vira erro do `run`; outra é avisada e as ordens dela caem.
    fn nao_abriu(&mut self, id: WindowId, e: Error) {
        if id == WindowId::MAIN {
            self.erro = Some(e);
            return;
        }
        eprintln!("[pelicano] a janela {id:?} não abriu: {e:?}");
        self.pendentes.retain(|(i, _)| *i != id);
        self.alcas.retain(|(i, _)| *i != id);
    }

    /// Abre as janelas pedidas (a principal no `resumed`).
    fn abrir_janelas(&mut self, el: &ActiveEventLoop) {
        for (id, s) in std::mem::take(&mut self.abrir) {
            // Invisível até o AccessKit se ligar: ele precisa chegar antes da janela aparecer.
            let w = match el.create_window(atributos(&s).with_visible(false)) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    self.nao_abriu(id, Error::Window(e.to_string()));
                    if id == WindowId::MAIN {
                        return el.exit();
                    }
                    continue;
                }
            };
            let tela = match &self.gpu {
                Some(g) => g.tela(w.clone(), s.transparent).map(Saida::Gpu),
                None if self.sem_gpu => {
                    self.renderer.gpu.max_textura = gpu::SEM_LIMITE_TEXTURA;
                    Saida::cpu(&w)
                }
                None => match Gpu::new(w.clone(), s.transparent, s.gpu) {
                    Ok((g, t)) => {
                        self.renderer.gpu.max_textura = g.device.limits().max_texture_dimension_2d;
                        self.gpu = Some(g);
                        Ok(Saida::Gpu(t))
                    }
                    // Sem placa nem rasterizador em software (VM, Linux sem Mesa): a imagem da CPU
                    // vai direto para a janela, sem wgpu.
                    Err(e) => {
                        eprintln!("[pelicano] sem aparelho gráfico ({e}): mostrando a janela pela CPU");
                        self.sem_gpu = true;
                        self.renderer.gpu.max_textura = gpu::SEM_LIMITE_TEXTURA;
                        Saida::cpu(&w)
                    }
                },
            };
            let tela = match tela {
                Ok(t) => t,
                Err(e) => {
                    self.nao_abriu(id, Error::Gpu(e));
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
            let acesso = accesskit_winit::Adapter::with_event_loop_proxy(el, &w, self.proxy.clone());
            // O X11 do GNOME ignora a posição da criação (o winit não marca como escolhida pelo
            // programa); mover depois de aberta ele obedece.
            if let Some(p) = s.position {
                w.set_outer_position(winit::dpi::LogicalPosition::new(p.x, p.y));
            }
            w.set_visible(true);
            w.request_redraw();
            self.janelas.push(Janela { id, w, tela, quadro: Quadro::default(), states: States::default(), cursor: None, icon: CursorIcon::Default, transparente: s.transparent, fechar_ao_pedir: s.close_on_request, tudo: true, na_gpu: false, mover: false, ultimo_mover: std::time::Instant::now(), proximo: None, roda: None, roda_suave: None, acesso, leitor: false, arvore: None, dedo: None });
            let escala = self.janelas.last().map_or(1.0, |j| j.w.scale_factor() as f32);
            let m: Vec<_> = self.app.on_window_event(id, &Event::ScaleChanged(escala)).into_iter().collect();
            self.update(m);
            let (dela, outras) = std::mem::take(&mut self.pendentes).into_iter().partition(|(i, _)| *i == id);
            self.pendentes = outras;
            for (i, c) in dela {
                self.window_cmd(i, c);
            }
            let (dela, outras) = std::mem::take(&mut self.alcas).into_iter().partition(|(i, _)| *i == id);
            self.alcas = outras;
            for (i, f) in dela {
                self.alca(i, f);
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
        let (redraw, pegou) = {
            let el = self.app.view_window(id);
            let node = widget::layout(&el, size, &mut self.renderer, &j.states, &theme);
            let mut cx = EventCx { renderer: &mut self.renderer, states: &mut j.states, cursor: j.cursor, messages: &mut msgs, window: &mut cmds, redraw: false, clipboard: &mut *self.clipboard };
            let pegou = widget::dispatch(&el, &node, &ev, &mut cx);
            (cx.redraw, pegou)
        };
        if pegou == crate::Captured::No {
            msgs.extend(self.app.on_unhandled_event(id, &ev));
        }
        // O mouse andou: o quadro é gravado e comparado; se nada mudou, nem pinta.
        let mexeu = matches!(ev, Event::CursorMoved(_) | Event::CursorLeft);
        msgs.extend(self.app.on_window_event(id, &ev));
        for c in cmds {
            self.window_cmd(id, c);
        }
        if !msgs.is_empty() {
            self.update(msgs);
        } else if redraw || mexeu || matches!(ev, Event::Resized(_)) {
            // Sem mensagem o estado do app não mudou: só esta janela pinta de novo.
            if let Some(i) = self.indice(id) {
                self.janelas[i].w.request_redraw();
            }
        }
        let Some(j) = self.indice(id).map(|i| &mut self.janelas[i]) else { return };
        for e in mouse_do_dedo(&mut j.dedo, &ev, pegou == crate::Captured::Yes) {
            // O evento anterior pode ter fechado janelas: o índice muda.
            let Some(i) = self.indice(id) else { return };
            let j = &mut self.janelas[i];
            match e {
                Event::CursorMoved(p) => j.cursor = Some(p),
                Event::CursorLeft => j.cursor = None,
                _ => {}
            }
            self.event(i, e);
        }
    }

    /// Os pedidos do leitor de tela. Clicar vira um clique no meio do widget (cada um já sabe o que
    /// fazer com ele); focar só põe o foco (os nós têm a chave do widget).
    fn acessibilidade(&mut self, e: accesskit_winit::Event) {
        let Some(i) = self.janelas.iter().position(|j| j.w.id() == e.window_id) else { return };
        let j = &mut self.janelas[i];
        match e.window_event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                j.leitor = true;
                j.arvore = None;
                j.w.request_redraw();
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                j.leitor = false;
                j.arvore = None;
            }
            accesskit_winit::WindowEvent::ActionRequested(pedido) => {
                match pedido_do_leitor(pedido.action) {
                    Some(Pedido::Focar) => {
                        j.states.focus = Some(pedido.target_node.0);
                        return j.w.request_redraw();
                    }
                    Some(Pedido::Clicar) => {}
                    None => return,
                }
                let alvo = j.arvore.as_ref().and_then(|t| t.nodes.iter().find(|(id, _)| *id == pedido.target_node)).and_then(|(_, n)| n.bounds());
                let Some(b) = alvo else { return };
                let meio = Point::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
                let (id, antes) = (j.id, j.cursor.replace(meio));
                self.event(i, Event::MousePressed(crate::MouseButton::Left));
                if let Some(i) = self.indice(id) {
                    self.event(i, Event::MouseReleased(crate::MouseButton::Left));
                }
                if let Some(j) = self.indice(id).map(|i| &mut self.janelas[i]) {
                    j.cursor = antes;
                    j.w.request_redraw();
                }
            }
        }
    }

    /// Sai assim que as tarefas do app terminarem (ou o tempo acabar). Enquanto isso as janelas
    /// somem e as mensagens continuam chegando ao `update`.
    fn sair_quando_der(&mut self, el: &ActiveEventLoop) {
        let desde = *self.saindo.get_or_insert_with(std::time::Instant::now);
        if self.trabalhos == 0 || desde.elapsed() >= ESPERA_AO_SAIR {
            return el.exit();
        }
        for j in &self.janelas {
            j.w.set_visible(false);
        }
        el.set_control_flow(ControlFlow::WaitUntil(desde + ESPERA_AO_SAIR));
    }

    /// Um passo do roteiro na janela principal; pede o próximo quadro enquanto ele não acaba.
    fn seguir_roteiro(&mut self, agora: std::time::Instant, prox: &mut Option<std::time::Instant>) {
        let alvo = self.roteiro_janela;
        let (Some(rt), Some(j)) = (self.roteiro.as_mut(), self.janelas.iter().find(|j| j.id == alvo)) else { return };
        // Os rótulos vêm da árvore de acessibilidade do último quadro (áreas lógicas).
        // Só o que está à vista: a árvore também tem o que a rolagem deixou fora da janela.
        let tela = Self::logical_size(&j.w);
        let achar = |rotulo: &str| {
            let meio = |n: &accesskit::Node| n.bounds().map(|b| Point::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32));
            let visivel = |n: &accesskit::Node| meio(n).is_some_and(|p| (0.0..tela.width).contains(&p.x) && (0.0..tela.height).contains(&p.y));
            let tem = |n: &accesskit::Node, exato: bool| visivel(n) && [n.label(), n.value()].into_iter().flatten().any(|s| if exato { s == rotulo } else { s.contains(rotulo) });
            let nos = &j.arvore.as_ref()?.nodes;
            let (_, n) = nos.iter().find(|(_, n)| tem(n, true)).or_else(|| nos.iter().find(|(_, n)| tem(n, false)))?;
            meio(n)
        };
        let s = rt.tick(agora, achar);
        let (id, w) = (j.id, j.w.clone());
        for ev in s.eventos {
            let Some(i) = self.indice(id) else { return };
            if let Event::CursorMoved(p) = ev {
                self.janelas[i].cursor = Some(p);
            }
            self.event(i, ev);
        }
        w.request_redraw();
        if s.acabou || s.sair {
            if let Some(rt) = self.roteiro.take() {
                self.roteiro_cursor = rt.cursor;
            }
            // Sem o roteiro o quadro volta a comparar: o seguinte precisa ser inteiro para apagar a seta.
            if let Some(i) = self.indice(id) {
                self.janelas[i].tudo = true;
            }
            if let Some(g) = self.gravador.take() {
                g.fim();
                self.gravar = None;
            }
        } else {
            let q = agora + self.intervalo.unwrap_or(std::time::Duration::from_millis(16));
            *prox = Some(prox.map_or(q, |p| p.min(q)));
        }
        // Fechar como o X da janela: o app fica sabendo antes (e salva o que precisa).
        let Some(i) = self.indice(id).filter(|_| s.sair) else { return };
        if !self.janelas[i].fechar_ao_pedir {
            return self.event(i, Event::CloseRequested);
        }
        let m = self.app.on_window_event(id, &Event::CloseRequested);
        self.fechar(i);
        self.update(m.into_iter().collect());
    }

    /// Onde está a janela `id` agora: um `update` no meio pode ter fechado outras.
    fn indice(&self, id: WindowId) -> Option<usize> {
        self.janelas.iter().position(|j| j.id == id)
    }

    /// A roda que espera (com o mouse antes dela) chega à janela `id`.
    fn entregar_roda(&mut self, id: WindowId) {
        let Some(i) = self.indice(id) else { return };
        let j = &mut self.janelas[i];
        if let Some(ev) = j.roda.take() {
            j.ultimo_mover = std::time::Instant::now();
            self.entregar_mover(id);
            if let Some(i) = self.indice(id) {
                self.event(i, ev);
            }
        }
    }

    fn entregar_mover(&mut self, id: WindowId) {
        let Some(i) = self.indice(id) else { return };
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
        // A principal no roteiro: o cursor desenhado não entra na comparação, então o quadro é sempre inteiro.
        let principal = j.id == self.roteiro_janela;
        let roteiro = self.roteiro.as_ref().filter(|_| principal);
        let tudo = std::mem::replace(&mut j.tudo, false) || roteiro.is_some();
        self.renderer.trocar(&mut j.quadro);
        let t0 = std::time::Instant::now();
        j.states.tick(t0);
        let el = self.app.view_window(j.id);
        let t1 = t0.elapsed();
        let node = widget::layout(&el, size, &mut self.renderer, &j.states, &theme);
        let t2 = t0.elapsed();
        let (states, cursor, inspecionar) = (&j.states, j.cursor, self.inspecionar);
        let pintar = |r: &mut Renderer| {
            widget::draw(&el, &node, r, states, &theme, cursor, inspecionar);
            if let Some(rt) = roteiro {
                rt.desenhar(r, t0);
            }
        };
        // Primeiro grava (barato: só áreas e hashes) e compara com o quadro na tela. Com leitor de
        // tela, a gravação também monta a árvore de acessibilidade.
        self.renderer.record();
        let mut acesso = (j.leitor || roteiro.is_some()).then(Access::default);
        let icon = widget::draw_access(&el, &node, &mut self.renderer, states, &theme, cursor, inspecionar, acesso.as_mut());
        let mudou = self.renderer.changes();
        let t3 = t0.elapsed();
        let dano = std::env::var_os("PELICANO_DANO").is_some();
        let tipo = dano.then(|| match &mudou { Changes::All => "tudo".to_string(), Changes::None => "nada".to_string(), Changes::Regions(r) => format!("{} regiões", r.len()) });
        let regioes = match mudou {
            _ if tudo => None,
            Changes::None => Some(Vec::new()),
            Changes::All => None,
            Changes::Regions(r) => Some(r),
        };
        let nada = regioes.as_ref().is_some_and(|r| r.is_empty());
        // Na GPU o quadro sai inteiro; o que ela ainda não faz cai na CPU (inteiro logo depois de
        // um quadro da GPU, que deixou o `pixmap` para trás).
        // Gravando, o quadro sai pela CPU: é o `pixmap` que vai para o vídeo.
        let gpu = self.gpu.as_ref().is_some_and(|g| g.pintor.is_some()) && self.gravar.is_none();
        let mut na_gpu = false;
        if gpu && !nada {
            if self.renderer.falta_gpu.is_none() {
                self.renderer.begin_gpu(size, escala, fundo);
                pintar(&mut self.renderer);
                self.renderer.end();
            }
            na_gpu = self.renderer.falta_gpu.is_none();
            if let Some(m) = self.renderer.falta_gpu.filter(|_| dano) {
                if self.avisados.insert(m) {
                    eprintln!("[gpu] quadro na CPU: {m}");
                }
            }
        }
        let regioes = if j.na_gpu { regioes.filter(|r| r.is_empty()) } else { regioes };
        if !nada {
            j.na_gpu = na_gpu;
        }
        // Na CPU, mas a lista da GPU representa o quadro: ela é pintada em faixas, uma thread
        // cada, em vez do tiny-skia numa thread só.
        let mut em_faixas = false;
        if !na_gpu && !nada && self.renderer.falta_gpu.is_none() {
            self.renderer.begin_gpu(size, escala, fundo);
            pintar(&mut self.renderer);
            self.renderer.end();
            if self.renderer.falta_gpu.is_none() {
                self.renderer.pintar_lista_cpu(regioes.as_deref());
                em_faixas = true;
            }
        }
        // shortcut: o tiny-skia (quadro com camada/vidro, regiões parciais, `capture`) mistura em sRGB,
        // não em linear: bordas suavizadas e translúcidos saem um pouco mais escuros que na GPU e nas
        // faixas. Pintar em linear também ali (pixmap de 16 bits) se o salto incomodar.
        let parcial = na_gpu || em_faixas || regioes.as_ref().is_some_and(|rs| rs.iter().all(|r| self.renderer.begin_parcial(size, escala, fundo, *r) && {
            pintar(&mut self.renderer);
            self.renderer.end();
            true
        }));
        if !parcial {
            self.renderer.begin(size, escala, fundo);
            pintar(&mut self.renderer);
        }
        drop(el);
        if let Some(a) = acesso {
            let arvore = a.tree(&w.title(), escala as f64, j.states.focus);
            if j.arvore.as_ref() != Some(&arvore) {
                j.arvore = Some(arvore.clone());
                j.acesso.update_if_active(|| arvore);
            }
        }
        // Transição no meio do caminho: o próximo quadro vem no ritmo do monitor.
        if j.states.animating() {
            j.proximo = Some(t0 + self.intervalo.unwrap_or(std::time::Duration::from_millis(16)));
        }
        if icon != j.icon {
            j.icon = icon;
            w.set_cursor(icon);
        }
        // Quadro perdido (superfície velha no meio do resize): pede outro, senão a janela
        // fica com o conteúdo antigo até o próximo evento.
        let t4 = t0.elapsed();
        let foi = match (&mut j.tela, self.gpu.as_mut()) {
            _ if nada => true,
            (Saida::Gpu(t), Some(g)) if na_gpu => t.present_gpu(g, &mut self.renderer.gpu),
            (Saida::Gpu(t), Some(g)) => t.present(g, &self.renderer.pixmap, self.renderer.changed_rows()),
            (Saida::Cpu(t), _) => mostrar_cpu(t, &self.renderer.pixmap),
            (Saida::Gpu(_), None) => true,
        };
        if !foi {
            j.tudo = true;
            w.request_redraw();
        }
        // Os tempos acumulados de cada etapa do quadro (para achar o gargalo).
        if let Some(tipo) = tipo {
            let onde = if na_gpu { " gpu" } else if em_faixas { " faixas" } else { "" };
            eprintln!("[quadro] {size:?} view {t1:.1?} layout {t2:.1?} gravar {t3:.1?} pintar {t4:.1?} tela {:.1?} {tipo}{onde}", t0.elapsed());
        }
        if std::env::var_os("PELICANO_MEMORIA").is_some() {
            eprintln!("[memória] {}", self.renderer.memoria());
        }
        if let Some(destino) = self.gravar.as_deref().filter(|_| principal) {
            let pm = &self.renderer.pixmap;
            if self.gravador.is_none() {
                match crate::roteiro::Gravador::new(destino, pm.width(), pm.height()) {
                    Ok(g) => self.gravador = Some(g),
                    Err(e) => {
                        eprintln!("[gravar] {e}");
                        self.gravar = None;
                    }
                }
            }
            if let Some(g) = &mut self.gravador {
                g.quadro(pm.width(), pm.height(), pm.data());
            }
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

impl<A: App> ApplicationHandler<Evento<A::Msg>> for Runtime<A> {
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
            return self.sair_quando_der(el);
        }
        let agora = std::time::Instant::now();
        let janela = self.janelas.first().map(|j| j.w.clone());
        let intervalo = *self.intervalo.get_or_insert_with(|| {
            let hz = janela.and_then(|w| w.current_monitor()).and_then(|m| m.refresh_rate_millihertz()).unwrap_or(60_000);
            std::time::Duration::from_micros(1_000_000_000 / hz.max(1000) as u64)
        });
        let mut prox: Option<std::time::Instant> = None;
        self.seguir_roteiro(agora, &mut prox);
        for j in &mut self.janelas {
            match j.proximo {
                Some(q) if agora >= q => {
                    j.proximo = None;
                    j.w.request_redraw();
                }
                Some(q) => prox = Some(prox.map_or(q, |p| p.min(q))),
                None => {}
            }
        }
        if !self.janelas.iter().any(|j| j.mover || j.roda.is_some() || j.roda_suave.is_some()) {
            return el.set_control_flow(prox.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        }
        // Pelos ids: entregar pode fechar janelas.
        let ids: Vec<_> = self.janelas.iter().map(|j| j.id).collect();
        for id in ids {
            let Some(j) = self.indice(id).map(|i| &mut self.janelas[i]) else { continue };
            // Mouse e roda: no máximo uma entrega por quadro; o resto soma até lá.
            if j.mover || j.roda.is_some() || j.roda_suave.is_some() {
                match esperar_entrega(j.ultimo_mover, intervalo, agora) {
                    None => {
                        proxima_fatia(&mut j.roda, &mut j.roda_suave);
                        self.entregar_mover(id);
                        self.entregar_roda(id);
                    }
                    Some(quando) => prox = Some(prox.map_or(quando, |p| p.min(quando))),
                }
            }
        }
        self.abrir_janelas(el);
        el.set_control_flow(prox.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        if self.sair {
            self.sair_quando_der(el);
        }
    }

    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(g) = self.gravador.take() {
            g.fim();
        }
        self.clipboard = Box::new(widget::MemoryClipboard::default());
    }

    fn user_event(&mut self, el: &ActiveEventLoop, e: Evento<A::Msg>) {
        match e {
            Evento::App(m) => self.update(vec![m]),
            Evento::Feito(m) => {
                self.trabalhos -= 1;
                self.update(vec![m]);
            }
            Evento::Janela(id, ev) => {
                if let Some(i) = self.janelas.iter().position(|j| j.id == id) {
                    if let Event::CursorMoved(p) = ev {
                        self.janelas[i].cursor = Some(p);
                    }
                    self.event(i, ev);
                }
            }
            Evento::Acesso(a) => self.acessibilidade(a),
        }
        // Sair fica para o `about_to_wait`, que vem depois de todo evento e espera as tarefas.
        self.abrir_janelas(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, wid: winit::window::WindowId, we: WindowEvent) {
        let Some(i) = self.janelas.iter().position(|j| j.w.id() == wid) else { return };
        // No roteiro, o mouse de verdade não briga com o desenhado.
        if self.roteiro.is_some() && self.janelas[i].id == self.roteiro_janela && matches!(we, WindowEvent::CursorMoved { .. } | WindowEvent::CursorLeft { .. } | WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::Touch(_)) {
            return;
        }
        let j = &mut self.janelas[i];
        j.acesso.process_event(&j.w, &we);
        let (id, escala) = (j.id, j.w.scale_factor());
        let ev = match we {
            WindowEvent::RedrawRequested => return self.redraw(i),
            WindowEvent::CloseRequested => {
                if j.fechar_ao_pedir {
                    // Avisa o app antes de a janela sumir.
                    let m = self.app.on_window_event(id, &Event::CloseRequested);
                    self.fechar(i);
                    self.update(m.into_iter().collect());
                    self.abrir_janelas(el);
                    return;
                }
                Some(Event::CloseRequested)
            }
            WindowEvent::Resized(s) => {
                // A superfície muda de tamanho no `present`, uma vez por quadro.
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
            WindowEvent::MouseWheel { delta, .. } => {
                let ev = match delta {
                    // Clique de mouse sem Ctrl/Alt (que costumam ser zoom, um passo por clique): desliza.
                    MouseScrollDelta::LineDelta(x, y) if (self.modifiers - crate::Modifiers::SHIFT).is_empty() => {
                        let ev = Event::Wheel { dx: x * 48.0, dy: y * 48.0, modifiers: self.modifiers };
                        // Shift mudou no meio: o resto da anterior vai de uma vez, depois da que já esperava.
                        if let Some(resto) = juntar_roda(&mut j.roda_suave, ev) {
                            if let Some(antes) = juntar_roda(&mut j.roda, resto) {
                                let resto = j.roda.replace(antes);
                                self.entregar_roda(id);
                                if let Some(i) = self.indice(id) {
                                    self.janelas[i].roda = resto;
                                }
                            }
                        }
                        return;
                    }
                    MouseScrollDelta::LineDelta(x, y) => Event::Wheel { dx: x * 48.0, dy: y * 48.0, modifiers: self.modifiers },
                    MouseScrollDelta::PixelDelta(p) => Event::Wheel { dx: (p.x / escala) as f32, dy: (p.y / escala) as f32, modifiers: self.modifiers },
                };
                // Soma com a que espera; vai uma por quadro no `about_to_wait`, como o mouse.
                // Modificador mudou no meio: a de antes vai já, a nova fica esperando.
                if let Some(antes) = juntar_roda(&mut j.roda, ev) {
                    let nova = j.roda.replace(antes);
                    self.entregar_roda(id);
                    if let Some(i) = self.indice(id) {
                        self.janelas[i].roda = nova;
                    }
                }
                None
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                Some(Event::ModifiersChanged(self.modifiers))
            }
            WindowEvent::DroppedFile(p) => Some(Event::FileDropped(p)),
            WindowEvent::HoveredFile(p) => Some(Event::FileHovered(p)),
            WindowEvent::HoveredFileCancelled => Some(Event::FileHoverCancelled),
            WindowEvent::KeyboardInput { event, .. } => Some(Event::Key { key: event.logical_key.clone(), text: event.text.map(|t| t.to_string()), modifiers: self.modifiers, pressed: event.state.is_pressed() }),
            WindowEvent::Focused(f) => Some(Event::Focused(f)),
            WindowEvent::Touch(t) => Some(toque(t.phase, t.id, t.location, escala)),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                j.tudo = true;
                self.intervalo = None;
                j.w.request_redraw();
                Some(Event::ScaleChanged(scale_factor as f32))
            }
            _ => None,
        };
        if let Some(ev) = ev {
            // Clique e tecla veem o mouse onde ele está, e a rolagem que veio antes deles.
            self.entregar_mover(id);
            self.entregar_roda(id);
            let Some(i) = self.indice(id) else { return };
            // Com o modo debug, o que chega da janela vai para o stderr (para achar tecla ou clique perdido).
            if self.settings.debug && !matches!(ev, Event::CursorMoved(_)) {
                eprintln!("[pelicano] {ev:?} cursor={:?}", self.janelas[i].cursor);
            }
            self.event(i, ev);
        }
        self.abrir_janelas(el);
    }
}

/// O que um pedido do leitor de tela vira.
#[derive(Debug, PartialEq)]
enum Pedido {
    Clicar,
    Focar,
}

fn pedido_do_leitor(a: accesskit::Action) -> Option<Pedido> {
    match a {
        accesskit::Action::Click => Some(Pedido::Clicar),
        accesskit::Action::Focus => Some(Pedido::Focar),
        _ => None,
    }
}

/// Mouse e roda vão no máximo uma vez por quadro: `None` entrega já, senão a hora de entregar.
fn esperar_entrega(ultimo: std::time::Instant, intervalo: std::time::Duration, agora: std::time::Instant) -> Option<std::time::Instant> {
    let quando = ultimo + intervalo;
    (agora < quando).then_some(quando)
}

/// O pedaço da roda que anda neste quadro: 40% do que falta (uns 100 ms até parar a 60 Hz),
/// ou tudo quando falta menos de meio pixel.
fn fatia_roda(resto: &mut Option<Event>) -> Option<Event> {
    let Some(Event::Wheel { dx, dy, modifiers }) = resto.as_mut() else { return resto.take() };
    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        return resto.take();
    }
    let (fx, fy) = (*dx * 0.4, *dy * 0.4);
    *dx -= fx;
    *dy -= fy;
    Some(Event::Wheel { dx: fx, dy: fy, modifiers: *modifiers })
}

/// Um pedaço da rolagem suave passa para a entrega. Com outros modificadores na entrega, ela vai
/// sozinha e o pedaço volta para o resto (senão o resto sumia).
fn proxima_fatia(roda: &mut Option<Event>, suave: &mut Option<Event>) {
    if let Some(f) = fatia_roda(suave) {
        if let Some(antes) = juntar_roda(roda, f) {
            if let Some(f) = roda.replace(antes) {
                juntar_roda(suave, f);
            }
        }
    }
}

/// Soma a roda `nova` à que espera entrega. Com outros modificadores não dá para somar: devolve a
/// que esperava (a entregar já) e guarda a nova.
fn juntar_roda(pendente: &mut Option<Event>, nova: Event) -> Option<Event> {
    match (pendente.as_mut(), &nova) {
        (Some(Event::Wheel { dx, dy, modifiers }), Event::Wheel { dx: x, dy: y, modifiers: m }) if modifiers == m => {
            *dx += x;
            *dy += y;
            None
        }
        _ => pendente.replace(nova),
    }
}

/// O toque do winit em pixels lógicos.
fn toque(fase: TouchPhase, id: u64, p: winit::dpi::PhysicalPosition<f64>, escala: f64) -> Event {
    let p = p.to_logical::<f32>(escala);
    let position = Point::new(p.x, p.y);
    match fase {
        TouchPhase::Started => Event::FingerPressed { id, position },
        TouchPhase::Moved => Event::FingerMoved { id, position },
        TouchPhase::Ended => Event::FingerLifted { id, position },
        TouchPhase::Cancelled => Event::FingerLost { id, position },
    }
}

/// O mouse que um toque vira: o winit quase nunca o simula, e sem isso botão, lista e campo
/// não sentem o dedo. Só o primeiro dedo, e só se ninguém `pegou` o toque ao encostar (a tela de
/// desenho que trata o dedo não recebe o mesmo traço outra vez como mouse). Depois de encostar,
/// o dedo segue como mouse até sair, para o botão não ficar apertado.
fn mouse_do_dedo(dedo: &mut Option<u64>, ev: &Event, pegou: bool) -> Vec<Event> {
    use crate::MouseButton::Left;
    match *ev {
        Event::FingerPressed { id, position } if !pegou && dedo.is_none() => {
            *dedo = Some(id);
            vec![Event::CursorMoved(position), Event::MousePressed(Left)]
        }
        Event::FingerMoved { id, position } if *dedo == Some(id) => vec![Event::CursorMoved(position)],
        // Sai da janela depois de soltar: no toque não há "mouse em cima" que fique.
        Event::FingerLifted { id, position } if *dedo == Some(id) => {
            *dedo = None;
            vec![Event::CursorMoved(position), Event::MouseReleased(Left), Event::CursorLeft]
        }
        // Cancelado: solta fora, para não virar clique.
        Event::FingerLost { id, .. } if *dedo == Some(id) => {
            *dedo = None;
            vec![Event::CursorLeft, Event::MouseReleased(Left)]
        }
        _ => Vec::new(),
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
    /// O desenho na GPU; `None` = na CPU ([`Settings::gpu`]).
    pintor: Option<gpu::Pintor>,
}

/// Por onde o quadro chega à janela.
enum Saida {
    Gpu(Tela),
    /// Sem wgpu: o sistema copia a imagem (softbuffer). Sem transparência.
    Cpu(softbuffer::Surface<Arc<Window>, Arc<Window>>),
}

impl Saida {
    fn cpu(w: &Arc<Window>) -> Result<Saida, String> {
        let ctx = softbuffer::Context::new(w.clone()).map_err(|e| e.to_string())?;
        softbuffer::Surface::new(&ctx, w.clone()).map(Saida::Cpu).map_err(|e| e.to_string())
    }
}

/// O quadro da CPU na janela sem GPU: o softbuffer quer `0x00RRGGBB` por pixel; o fundo da
/// janela é opaco, então o premultiplicado já é a cor.
fn mostrar_cpu(t: &mut softbuffer::Surface<Arc<Window>, Arc<Window>>, pm: &tiny_skia::Pixmap) -> bool {
    let (Some(w), Some(h)) = (std::num::NonZeroU32::new(pm.width()), std::num::NonZeroU32::new(pm.height())) else { return true };
    if t.resize(w, h).is_err() {
        return false;
    }
    let Ok(mut buf) = t.buffer_mut() else { return false };
    for (d, p) in buf.iter_mut().zip(pm.data().chunks_exact(4)) {
        *d = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
    }
    buf.present().is_ok()
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
    fn new(w: Arc<Window>, transparente: bool, pintar: bool) -> Result<(Self, Tela), String> {
        // Vulkan/DX12/Metal primeiro: só sondar o OpenGL já carrega o driver dele (no Mesa, ~15 MB a
        // mais) sem nunca usá-lo. O OpenGL fica para a placa que não tem os outros.
        let mut achado = None;
        for backends in [wgpu::Backends::PRIMARY, wgpu::Backends::GL] {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..Default::default() });
            let Ok(surface) = instance.create_surface(w.clone()) else { continue };
            // A placa dedicada, se houver; o rasterizador em software (llvmpipe) só se não tiver outra.
            let pedido = wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false };
            let Some(mut adapter) = futures::executor::block_on(instance.request_adapter(&pedido)) else { continue };
            if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
                if let Some(a) = instance.enumerate_adapters(backends).into_iter().find(|a| a.get_info().device_type != wgpu::DeviceType::Cpu && a.is_surface_supported(&surface)) {
                    adapter = a;
                }
            }
            // Só o Vulkan em software (lavapipe): fica de reserva, o OpenGL ainda pode ter a placa.
            let cpu = adapter.get_info().device_type == wgpu::DeviceType::Cpu;
            if achado.is_none() || !cpu {
                achado = Some((instance, surface, adapter));
            }
            if !cpu {
                break;
            }
        }
        let (instance, surface, adapter) = achado.ok_or("nenhum adaptador gráfico")?;
        let info = adapter.get_info();
        let limites = gpu::limites(&adapter);
        let env = std::env::var("PELICANO_GPU").ok();
        let pintar = match env.as_deref() {
            Some("0") => false,
            Some("1") => limites.is_some(),
            _ => pintar && limites.is_some() && info.device_type != wgpu::DeviceType::Cpu,
        };
        if std::env::var_os("PELICANO_DANO").is_some() {
            eprintln!("[gpu] {} ({:?}, {:?}): desenho na {}", info.name, info.device_type, info.backend, if pintar { "GPU" } else { "CPU" });
        }
        let limites = limites.filter(|_| pintar).unwrap_or_else(|| wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()));
        let (device, queue) = futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: limites }, None)).map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        // A GPU mistura em espaço linear (como o Iced) e precisa de um formato "Srgb" que codifique
        // na escrita; sem ele, pinta a CPU. O quadro da CPU já vem em sRGB e passa por uma textura
        // "Srgb" também (decodifica na leitura, codifica na escrita: sai igual).
        let srgb = caps.formats.iter().copied().find(|f| f.is_srgb());
        let pintar = pintar && srgb.is_some();
        let format = srgb.filter(|_| pintar).or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb())).or(caps.formats.first().copied()).ok_or("superfície sem formato")?;
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
        let pintor = pintar.then(|| gpu::Pintor::new(&device, format));
        let g = Gpu { instance, adapter, device, queue, format, pipeline, layout, pintor };
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
        if w > 0 && h > 0 && (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&g.device, &self.config);
        }
    }

    /// O quadro desenhado na GPU direto na tela. `false` como no [`present`](Self::present).
    fn present_gpu(&mut self, g: &mut Gpu, lista: &mut gpu::Lista) -> bool {
        let (w, h) = lista.tamanho();
        self.resize(g, w, h);
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.surface.configure(&g.device, &self.config);
                return std::mem::replace(&mut self.falhou, true);
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let Some(p) = g.pintor.as_mut() else { return true };
        p.desenhar(&g.device, &g.queue, &view, lista);
        frame.present();
        g.device.poll(wgpu::Maintain::Poll);
        self.falhou = false;
        true
    }

    /// `false` se o quadro não chegou à tela e vale tentar de novo (só na primeira falha seguida).
    fn present(&mut self, g: &Gpu, pm: &tiny_skia::Pixmap, linhas: Option<(u32, u32)>) -> bool {
        let (w, h) = (pm.width(), pm.height());
        self.resize(g, w, h);
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        // A textura cresce em degraus de 256 px: arrastar a borda não cria uma por quadro (o shader
        // lê pixel a pixel, então sobra não aparece).
        let degrau = |n: u32| n.div_ceil(256) * 256;
        let capacidade = wgpu::Extent3d { width: degrau(w), height: degrau(h), depth_or_array_layers: 1 };
        // Textura nova (ou quadro anterior perdido) precisa do quadro todo; senão só as linhas que mudaram.
        let mut linhas = linhas.filter(|_| !self.falhou).unwrap_or((0, h));
        if self.textura.as_ref().map_or(true, |(t, _)| t.size() != capacidade) {
            linhas = (0, h);
            let t = g.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: capacidade,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: if g.format.is_srgb() { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm },
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
        // Sem isso o wgpu só solta as texturas velhas (de cada tamanho da janela) quando quer: a
        // memória de vídeo crescia a cada redimensionar.
        g.device.poll(wgpu::Maintain::Poll);
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

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn leitor_focar_nao_clica() {
        assert_eq!(pedido_do_leitor(accesskit::Action::Focus), Some(Pedido::Focar));
        assert_eq!(pedido_do_leitor(accesskit::Action::Click), Some(Pedido::Clicar));
        assert_eq!(pedido_do_leitor(accesskit::Action::ScrollDown), None);
    }

    #[test]
    fn mouse_e_roda_uma_vez_por_quadro() {
        let (t0, q) = (std::time::Instant::now(), Duration::from_millis(16));
        assert_eq!(esperar_entrega(t0, q, t0), Some(t0 + q));
        assert_eq!(esperar_entrega(t0, q, t0 + q), None);
    }

    #[test]
    #[should_panic(expected = "não pode capturar")]
    fn map_de_assinatura_recusa_closure_que_captura() {
        let x = 3u8;
        let _ = Subscription::every(Duration::from_millis(5)).map(move |_| x);
    }

    #[test]
    fn assinatura_tem_identidade_e_o_relogio_bate() {
        let id = |s: Subscription<Instant>| s.0[0].0;
        let ms = Duration::from_millis(5);
        assert_eq!(id(Subscription::every(ms)), id(Subscription::every(ms)), "mesmo pedido, mesma assinatura");
        assert_ne!(id(Subscription::every(ms)), id(Subscription::every(ms * 2)));
        let a = Subscription::every(ms).map(|_| 1u8).0[0].0;
        let b = Subscription::every(ms).map(|_| 2u8).0[0].0;
        assert_ne!(a, b, "mapeado para mensagens diferentes, são duas");
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        let t0 = Instant::now();
        let n = rt.block_on(Subscription::every(ms).0.remove(0).1.take(3).count());
        assert_eq!(n, 3);
        assert!(t0.elapsed() >= ms * 3);
    }

    #[test]
    fn alca_da_janela_passa_pelo_map() {
        let t = Task::window_handle(WindowId(3), |h| matches!(h, RawWindowHandle::Xcb(_))).map(|x11| if x11 { 7u8 } else { 0 });
        let Some(Action::Alca(id, f)) = t.0.into_iter().next() else { panic!("alça") };
        let h = winit::raw_window_handle::XcbWindowHandle::new(std::num::NonZeroU32::new(5).unwrap());
        assert_eq!((id, f(RawWindowHandle::Xcb(h))), (WindowId(3), 7));
    }

    #[test]
    fn clique_da_roda_desliza_em_pedacos_que_somam_o_clique() {
        let mut resto = Some(Event::Wheel { dx: 0.0, dy: -48.0, modifiers: crate::Modifiers::empty() });
        let (mut andou, mut quadros) = (0.0, 0);
        while let Some(Event::Wheel { dy, .. }) = super::fatia_roda(&mut resto) {
            assert!(dy.abs() <= 48.0 * 0.4 + 0.01);
            andou += dy;
            quadros += 1;
        }
        assert!((andou + 48.0f32).abs() < 1e-3, "{andou}");
        assert!((5..=15).contains(&quadros), "{quadros}");
    }

    #[test]
    fn rodas_da_mesma_fila_viram_uma_entrega() {
        let roda = |dy: f32, modifiers: crate::Modifiers| Event::Wheel { dx: 0.0, dy, modifiers };
        let mut pendente = None;
        // 30 eventos do touchpad entre dois quadros: uma entrega só, com a soma.
        for _ in 0..30 {
            assert!(super::juntar_roda(&mut pendente, roda(-4.0, crate::Modifiers::empty())).is_none());
        }
        assert_eq!(pendente, Some(roda(-120.0, crate::Modifiers::empty())));
        // Shift no meio (vira horizontal): a de antes sai inteira, a nova espera.
        let antes = super::juntar_roda(&mut pendente, roda(-4.0, crate::Modifiers::SHIFT));
        assert_eq!(antes, Some(roda(-120.0, crate::Modifiers::empty())));
        assert_eq!(pendente, Some(roda(-4.0, crate::Modifiers::SHIFT)));
    }

    #[test]
    fn fatia_da_roda_suave_nao_se_perde_com_ctrl_esperando() {
        let roda = |dy: f32, modifiers: crate::Modifiers| Event::Wheel { dx: 0.0, dy, modifiers };
        // Um clique da roda deslizando e um Ctrl+roda (zoom) esperando entrega.
        let (mut entrega, mut suave) = (Some(roda(10.0, crate::Modifiers::CONTROL)), Some(roda(48.0, crate::Modifiers::empty())));
        super::proxima_fatia(&mut entrega, &mut suave);
        assert_eq!(entrega, Some(roda(10.0, crate::Modifiers::CONTROL)));
        assert_eq!(suave, Some(roda(48.0, crate::Modifiers::empty())));
        // Sem nada esperando, o pedaço vai e o resto fica.
        let mut entrega = None;
        super::proxima_fatia(&mut entrega, &mut suave);
        let (Some(Event::Wheel { dy: a, .. }), Some(Event::Wheel { dy: b, .. })) = (entrega, suave) else { panic!() };
        assert!((a + b - 48.0).abs() < 1e-4 && a > 0.0);
    }

    #[test]
    fn toque_em_pixels_logicos_e_vira_mouse_so_se_ninguem_pegou() {
        let p = Point::new(10.0, 20.0);
        let ev = toque(TouchPhase::Started, 7, winit::dpi::PhysicalPosition::new(20.0, 40.0), 2.0);
        assert_eq!(ev, Event::FingerPressed { id: 7, position: p });
        assert_eq!(toque(TouchPhase::Cancelled, 7, winit::dpi::PhysicalPosition::new(20.0, 40.0), 2.0), Event::FingerLost { id: 7, position: p });
        use crate::MouseButton::Left;
        let mut dedo = None;
        assert!(mouse_do_dedo(&mut dedo, &ev, true).is_empty(), "pegou: não vira mouse");
        assert!(mouse_do_dedo(&mut dedo, &Event::FingerMoved { id: 7, position: p }, false).is_empty());
        assert_eq!(mouse_do_dedo(&mut dedo, &ev, false), vec![Event::CursorMoved(p), Event::MousePressed(Left)]);
        assert!(mouse_do_dedo(&mut dedo, &Event::FingerPressed { id: 8, position: p }, false).is_empty(), "segundo dedo não");
        assert_eq!(mouse_do_dedo(&mut dedo, &Event::FingerMoved { id: 7, position: p }, true), vec![Event::CursorMoved(p)]);
        assert_eq!(mouse_do_dedo(&mut dedo, &Event::FingerLifted { id: 7, position: p }, false), vec![Event::CursorMoved(p), Event::MouseReleased(Left), Event::CursorLeft]);
        assert_eq!(dedo, None);
        mouse_do_dedo(&mut dedo, &ev, false);
        assert_eq!(mouse_do_dedo(&mut dedo, &Event::FingerLost { id: 7, position: p }, false), vec![Event::CursorLeft, Event::MouseReleased(Left)]);
    }
}
