//! Diagnóstico da issue #135 do FeatherSheet (desenho deslocado no Windows 10): winit + softbuffer
//! puros, sem o pelicano. Abre a janela sem moldura (como o FeatherSheet) e desenha a borda da área
//! cliente em vermelho, uma grade a cada 50 px e uma cruz verde onde o mouse está. Se a cruz não
//! cair sob a ponta do mouse, o deslocamento é do winit/Windows; se cair, é do pelicano.
//! Teclas: D liga/desliga a moldura do sistema, M maximiza. O console mostra as medidas.
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct Diag {
    janela: Option<Rc<Window>>,
    sup: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    mouse: (f64, f64),
}

impl Diag {
    fn medidas(&self, motivo: &str) {
        let Some(w) = &self.janela else { return };
        let m = w.current_monitor().map(|m| format!("{:?} escala {}", m.size(), m.scale_factor())).unwrap_or_default();
        let t = format!(
            "[{motivo}] interna {:?} externa {:?} pos.interna {:?} pos.externa {:?} escala {} moldura {} maximizada {} monitor {m}",
            w.inner_size(), w.outer_size(), w.inner_position(), w.outer_position(), w.scale_factor(), w.is_decorated(), w.is_maximized()
        );
        println!("{t}");
    }
}

impl ApplicationHandler for Diag {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.janela.is_some() { return; }
        let attrs = Window::default_attributes().with_title("diagnóstico #135").with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(760.0, 520.0)).with_decorations(false).with_resizable(true);
        // `janela_diag pelicano`: cria como o pelicano (invisível, canto arredondado pedido ao DWM) e só mostra depois.
        let como_pelicano = std::env::args().any(|a| a == "pelicano");
        #[cfg(windows)]
        let attrs = if como_pelicano {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            attrs.with_visible(false).with_corner_preference(CornerPreference::Round)
        } else { attrs };
        let w = Rc::new(el.create_window(attrs).expect("janela"));
        if como_pelicano { self.medidas("criou invisível"); w.set_visible(true); }
        let ctx = softbuffer::Context::new(w.clone()).expect("contexto");
        self.sup = Some(softbuffer::Surface::new(&ctx, w.clone()).expect("superfície"));
        self.janela = Some(w);
        self.medidas("abriu");
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        let Some(w) = self.janela.clone() else { return };
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => { self.medidas("redimensionou"); w.request_redraw(); }
            WindowEvent::Moved(_) => self.medidas("moveu"),
            WindowEvent::ScaleFactorChanged { .. } => self.medidas("escala"),
            WindowEvent::CursorMoved { position, .. } => { self.mouse = (position.x, position.y); w.request_redraw(); }
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => {
                println!("[clique] em {:.0},{:.0} (px físicos da área cliente)", self.mouse.0, self.mouse.1);
                // Arrastar pela faixa de cima, para dar para mover a janela sem moldura.
                if self.mouse.1 < 40.0 { let _ = w.drag_window(); }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => match event.logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("d") => { w.set_decorations(!w.is_decorated()); self.medidas("moldura"); }
                // Conserto candidato: liga e desliga a moldura, o que obriga o Windows a recalcular a área da janela.
                Key::Character(c) if c.eq_ignore_ascii_case("f") => { w.set_decorations(true); w.set_decorations(false); self.medidas("conserto F"); }
                Key::Character(c) if c.eq_ignore_ascii_case("m") => w.set_maximized(!w.is_maximized()),
                Key::Named(NamedKey::Escape) => el.exit(),
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                let t = w.inner_size();
                let (Some(l), Some(a)) = (NonZeroU32::new(t.width), NonZeroU32::new(t.height)) else { return };
                let sup = self.sup.as_mut().expect("superfície");
                sup.resize(l, a).expect("resize");
                let mut buf = sup.buffer_mut().expect("buffer");
                let (lw, la) = (t.width as usize, t.height as usize);
                let (mx, my) = (self.mouse.0 as i64, self.mouse.1 as i64);
                for y in 0..la {
                    for x in 0..lw {
                        let borda = x < 4 || y < 4 || x + 4 >= lw || y + 4 >= la;
                        let cruz = (x as i64 == mx && (y as i64 - my).abs() < 30) || (y as i64 == my && (x as i64 - mx).abs() < 30);
                        let topo = y < 40;
                        buf[y * lw + x] = if cruz { 0x00_00C0_00 } else if borda { 0x00_E0_20_20 }
                            else if x % 50 == 0 || y % 50 == 0 { 0x00_B0_B0_B0 } else if topo { 0x00_30_60_C0 } else { 0x00_FF_FF_FF };
                    }
                }
                buf.present().expect("present");
            }
            _ => {}
        }
    }
}

fn main() {
    println!("Diagnóstico #135: a cruz verde deve ficar sob a ponta do mouse; a borda vermelha, rente à janela.");
    println!("Rode com o argumento pelicano para abrir como o FeatherSheet. F = conserto candidato.");
    println!("D = liga/desliga a moldura do Windows, M = maximiza, Esc = sai. Arraste pela faixa azul para mover.");
    let el = EventLoop::new().expect("laço");
    el.run_app(&mut Diag::default()).expect("rodar");
}
