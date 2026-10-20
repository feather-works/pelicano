//! Um shell de verdade numa janela. `cargo run --release --example terminal --features terminal`
use pelicano::*;

#[derive(Debug, Clone)]
enum Msg {
    Term(TerminalEvent),
}

struct App1 {
    term: Terminal,
    titulo: String,
}

impl App for App1 {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Term(TerminalEvent::Title(t)) => {
                self.titulo = t.clone();
                return Task::window(WindowCmd::Title(if t.is_empty() { "Terminal".into() } else { t }));
            }
            Msg::Term(TerminalEvent::Exit(_)) => return Task::window(WindowCmd::Exit),
            Msg::Term(_) => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        terminal(&self.term).size(14.0).background(Color::rgb8(24, 24, 28)).into()
    }

    fn subscription(&self) -> Subscription<Msg> {
        self.term.subscription().map(Msg::Term)
    }
}

fn main() -> Result<(), Error> {
    run(Settings { title: "Terminal".into(), size: Size::new(900.0, 560.0), ..Default::default() }, || {
        let term = Terminal::new(TerminalSettings::default()).expect("não abriu o shell");
        (App1 { term, titulo: String::new() }, Task::none())
    })
}
