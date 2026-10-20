//! O editor de texto do pelicano, cru: números de linha e a posição do cursor.
//! `cargo run --example notas [arquivo]`
use pelicano::column;
use pelicano::*;

#[derive(Debug, Clone)]
enum Msg {
    Mudou,
}

struct Notas {
    conteudo: Content,
    editado: bool,
}

impl App for Notas {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Mudou => self.editado = true,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let (ln, col) = self.conteudo.cursor();
        let status = format!("Ln {ln}, Col {col}    {} linhas    UTF-8{}", self.conteudo.line_count(), if self.editado { "    • editado" } else { "" });
        column![text_editor(&self.conteudo).line_numbers(true).on_change(Msg::Mudou), container(text(status).size(12.0)).padding(6.0)].into()
    }

    fn theme(&self) -> Theme {
        Theme::light()
    }
}

fn main() -> Result<(), Error> {
    let texto = std::env::args().nth(1).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(|| "Olá, FeatherNotes!\n\nDigite, selecione com o mouse ou Shift+setas, Ctrl+Z / Ctrl+Y, Ctrl+C / Ctrl+V.\n".into());
    run(Settings { title: "Notas (pelicano)".into(), size: Size::new(900.0, 640.0), ..Default::default() }, || (Notas { conteudo: Content::with_text(&texto), editado: false }, Task::none()))
}
