//! A janela, o laço de eventos, as tarefas assíncronas e a apresentação do quadro.
use crate::widget::{self, CursorIcon, Element, EventCx};
use crate::{Color, Event, Modifiers, Point, Renderer, ResizeDirection, Size, States, Theme};
use futures::future::{AbortHandle, Abortable, BoxFuture};
use futures::stream::BoxStream;
use futures::{FutureExt, Stream, StreamExt};
use std::future::Future;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

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
}

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
    /// Libera o inspetor de layout no F12. Desligado por padrão; `PELICANO_DEBUG=1` também libera (e já abre ligado).
    pub debug: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { title: "Pelicano".into(), size: Size::new(1024.0, 720.0), min_size: None, decorations: true, transparent: false, blur: false, resizable: true, close_on_request: true, app_id: None, debug: false }
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
    Close,
    Resize(Size),
    Title(String),
    Blur(bool),
}

enum Action<M> {
    Future(BoxFuture<'static, M>),
    Stream(BoxStream<'static, M>),
    Window(WindowCmd),
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
    pub fn window(cmd: WindowCmd) -> Self {
        Task(vec![Action::Window(cmd)])
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
                    Action::Window(c) => Action::Window(c),
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
    let event_loop = EventLoop::<A::Msg>::with_user_event().build().map_err(Error::EventLoop)?;
    let pool = futures::executor::ThreadPool::new().map_err(|e| Error::Window(e.to_string()))?;
    let (app, task) = init();
    let mut rt = Runtime { app, settings, proxy: event_loop.create_proxy(), pool, window: None, gpu: None, renderer: Renderer::new(), states: States::default(), cursor: None, modifiers: Modifiers::empty(), icon: CursorIcon::Default, pendentes: Vec::new(), erro: None, sair: false, inspecionar: false };
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
    settings: Settings,
    proxy: EventLoopProxy<A::Msg>,
    pool: futures::executor::ThreadPool,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    renderer: Renderer,
    states: States,
    cursor: Option<Point>,
    modifiers: Modifiers,
    icon: CursorIcon,
    /// Ordens de janela pedidas antes de a janela existir.
    pendentes: Vec<WindowCmd>,
    erro: Option<Error>,
    sair: bool,
    /// O inspetor está à mostra (F12, só com `Settings::debug`).
    inspecionar: bool,
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
                Action::Window(c) => self.window_cmd(c),
            }
        }
    }

    fn window_cmd(&mut self, c: WindowCmd) {
        let Some(w) = &self.window else {
            self.pendentes.push(c);
            return;
        };
        match c {
            WindowCmd::Drag => drop(w.drag_window()),
            WindowCmd::DragResize(d) => drop(w.drag_resize_window(d)),
            WindowCmd::Minimize => w.set_minimized(true),
            WindowCmd::ToggleMaximize => w.set_maximized(!w.is_maximized()),
            WindowCmd::Close => self.sair = true,
            WindowCmd::Resize(s) => drop(w.request_inner_size(LogicalSize::new(s.width, s.height))),
            WindowCmd::Title(t) => w.set_title(&t),
            WindowCmd::Blur(b) => w.set_blur(b),
        }
    }

    fn update(&mut self, msgs: Vec<A::Msg>) {
        for m in msgs {
            let t = self.app.update(m);
            self.run_task(t);
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn logical_size(&self) -> Size {
        let w = self.window.as_ref().expect("só chamado com janela");
        let s = w.inner_size().to_logical::<f32>(w.scale_factor());
        Size::new(s.width, s.height)
    }

    /// Passa o evento pela árvore atual e depois pelo `App::on_event`.
    fn event(&mut self, ev: Event) {
        if self.settings.debug && matches!(&ev, Event::Key { key: crate::Key::Named(crate::NamedKey::F12), pressed: true, .. }) {
            self.inspecionar = !self.inspecionar;
            return self.update(Vec::new());
        }
        let size = self.logical_size();
        let theme = self.app.theme();
        let mut msgs = Vec::new();
        let mut cmds = Vec::new();
        let redraw = {
            let el = self.app.view();
            let node = widget::layout(&el, size, &mut self.renderer, &self.states, &theme);
            let mut cx = EventCx { renderer: &mut self.renderer, states: &mut self.states, cursor: self.cursor, messages: &mut msgs, window: &mut cmds, redraw: false };
            widget::dispatch(&el, &node, &ev, &mut cx);
            cx.redraw
        };
        msgs.extend(self.app.on_event(&ev));
        for c in cmds {
            self.window_cmd(c);
        }
        // shortcut: todo movimento do mouse redesenha o quadro inteiro; comparar o hover antes/depois se pesar em telas 4K.
        if !msgs.is_empty() || redraw || matches!(ev, Event::CursorMoved(_) | Event::CursorLeft | Event::Resized(_)) {
            self.update(msgs);
        }
    }

    fn redraw(&mut self) {
        let Some(w) = self.window.clone() else { return };
        let size = self.logical_size();
        let theme = self.app.theme();
        let fundo = if self.settings.transparent { Color::TRANSPARENT } else { theme.background };
        self.renderer.begin(size, w.scale_factor() as f32, fundo);
        let icon = {
            let el = self.app.view();
            let node = widget::layout(&el, size, &mut self.renderer, &self.states, &theme);
            widget::draw(&el, &node, &mut self.renderer, &self.states, &theme, self.cursor, self.inspecionar)
        };
        if icon != self.icon {
            self.icon = icon;
            w.set_cursor(icon);
        }
        if let Some(gpu) = &mut self.gpu {
            gpu.present(&self.renderer.pixmap);
        }
    }
}

impl<A: App> ApplicationHandler<A::Msg> for Runtime<A> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let s = &self.settings;
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title(&s.title)
            .with_inner_size(LogicalSize::new(s.size.width, s.size.height))
            .with_decorations(s.decorations)
            .with_transparent(s.transparent)
            .with_blur(s.blur)
            .with_resizable(s.resizable);
        if let Some(m) = s.min_size {
            attrs = attrs.with_min_inner_size(LogicalSize::new(m.width, m.height));
        }
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
        if let Some(id) = &s.app_id {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs = attrs.with_name(id, id);
        }
        let w = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.erro = Some(Error::Window(e.to_string()));
                return el.exit();
            }
        };
        match Gpu::new(w.clone(), self.settings.transparent) {
            Ok(g) => self.gpu = Some(g),
            Err(e) => {
                self.erro = Some(Error::Gpu(e));
                return el.exit();
            }
        }
        self.window = Some(w);
        for c in std::mem::take(&mut self.pendentes) {
            self.window_cmd(c);
        }
        self.window.as_ref().expect("acabou de abrir").request_redraw();
    }

    fn user_event(&mut self, el: &ActiveEventLoop, m: A::Msg) {
        self.update(vec![m]);
        if self.sair {
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, we: WindowEvent) {
        let escala = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
        let ev = match we {
            WindowEvent::RedrawRequested => return self.redraw(),
            WindowEvent::CloseRequested => {
                if self.settings.close_on_request {
                    self.sair = true;
                }
                Some(Event::CloseRequested)
            }
            WindowEvent::Resized(s) => {
                if let Some(g) = &mut self.gpu {
                    g.resize(s.width, s.height);
                }
                let l = s.to_logical::<f32>(escala);
                Some(Event::Resized(Size::new(l.width, l.height)))
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = position.to_logical::<f32>(escala);
                self.cursor = Some(Point::new(p.x, p.y));
                Some(Event::CursorMoved(Point::new(p.x, p.y)))
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                Some(Event::CursorLeft)
            }
            WindowEvent::MouseInput { state, button, .. } => Some(match state {
                ElementState::Pressed => Event::MousePressed(button),
                ElementState::Released => Event::MouseReleased(button),
            }),
            WindowEvent::MouseWheel { delta, .. } => Some(match delta {
                MouseScrollDelta::LineDelta(x, y) => Event::Wheel { dx: x * 48.0, dy: y * 48.0 },
                MouseScrollDelta::PixelDelta(p) => Event::Wheel { dx: (p.x / escala) as f32, dy: (p.y / escala) as f32 },
            }),
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                None
            }
            WindowEvent::KeyboardInput { event, .. } => Some(Event::Key { key: event.logical_key.clone(), text: event.text.map(|t| t.to_string()), modifiers: self.modifiers, pressed: event.state.is_pressed() }),
            WindowEvent::Focused(f) => Some(Event::Focused(f)),
            WindowEvent::ScaleFactorChanged { .. } => {
                self.window.as_ref().map(|w| w.request_redraw());
                None
            }
            _ => None,
        };
        if let Some(ev) = ev {
            self.event(ev);
        }
        if self.sair {
            el.exit();
        }
    }
}

/// Só leva o quadro pronto para a tela: uma textura e um triângulo que a cobre.
struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    textura: Option<(wgpu::Texture, wgpu::BindGroup)>,
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
    fn new(w: Arc<Window>, transparente: bool) -> Result<Self, String> {
        let tamanho = w.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(w).map_err(|e| e.to_string())?;
        let adapter = futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).ok_or("nenhum adaptador gráfico")?;
        let (device, queue) = futures::executor::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()) },
            None,
        ))
        .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        // O quadro já vem em sRGB: um formato "Srgb" converteria de novo.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or(caps.formats.first().copied()).ok_or("superfície sem formato")?;
        let alpha_mode = if transparente && caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) { wgpu::CompositeAlphaMode::PreMultiplied } else { caps.alpha_modes[0] };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: tamanho.width.max(1),
            height: tamanho.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
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
        Ok(Gpu { surface, device, queue, config, pipeline, layout, textura: None })
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn present(&mut self, pm: &tiny_skia::Pixmap) {
        let (w, h) = (pm.width(), pm.height());
        if (w, h) != (self.config.width, self.config.height) {
            self.resize(w, h);
        }
        let extent = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        if self.textura.as_ref().map_or(true, |(t, _)| t.size() != extent) {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
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
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }] });
            self.textura = Some((t, bg));
        }
        let (t, bg) = self.textura.as_ref().expect("criada acima");
        self.queue.write_texture(
            wgpu::ImageCopyTexture { texture: t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            pm.data(),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
            extent,
        );
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            // Superfície velha (janela mudou de tamanho no meio): reconfigura e espera o próximo quadro.
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
        frame.present();
    }
}
