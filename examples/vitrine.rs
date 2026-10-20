//! Todos os widgets numa janela de vidro sem moldura.
//! `cargo run --example vitrine` abre a janela; `-- --capturar saida.png` só gera o PNG.
use pelicano::*;
// O `column!` do pelicano, não o da biblioteca padrão.
use pelicano::column;

#[derive(Debug, Clone)]
enum Msg {
    Nome(String),
    Enviar,
    Notificar(bool),
    Escuro(bool),
    Formato(&'static str),
    Volume(f32),
    Arrastar,
    Redimensionar,
    Maximizar,
    Minimizar,
    Fechar,
}

struct Vitrine {
    nome: String,
    enviado: Option<String>,
    notificar: bool,
    escuro: bool,
    formato: Option<&'static str>,
    volume: f32,
}

const FORMATOS: [&str; 4] = [".xlsx", ".ods", ".csv", ".fsdz"];

impl App for Vitrine {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Nome(n) => self.nome = n,
            Msg::Enviar => self.enviado = Some(self.nome.clone()),
            Msg::Notificar(b) => self.notificar = b,
            Msg::Escuro(b) => self.escuro = b,
            Msg::Formato(f) => self.formato = Some(f),
            Msg::Volume(v) => self.volume = v,
            Msg::Arrastar => return Task::window(WindowCmd::Drag),
            Msg::Redimensionar => return Task::window(WindowCmd::DragResize(ResizeDirection::SouthEast)),
            Msg::Maximizar => return Task::window(WindowCmd::ToggleMaximize),
            Msg::Minimizar => return Task::window(WindowCmd::Minimize),
            Msg::Fechar => return Task::window(WindowCmd::Close),
        }
        Task::none()
    }

    fn theme(&self) -> Theme {
        if self.escuro {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        let t = self.theme();
        let escuro = self.escuro;
        let titulo = mouse_area(
            row![text("Pelicano").size(15).font(Font::SEMIBOLD), text("vitrine").size(13).color(t.muted), horizontal_space()]
                .spacing(8)
                .align(Align::Center)
                .padding([0u16, 16])
                .width(Length::FILL)
                .height(40.0),
        )
        .on_press(Msg::Arrastar)
        .on_double_click(Msg::Maximizar);
        let controle = |rotulo: &'static str, m: Msg| button(text(rotulo).size(14)).on_press(m).padding([4u16, 12]).style(button_text);
        let barra = row![titulo, controle("–", Msg::Minimizar), controle("□", Msg::Maximizar), controle("✕", Msg::Fechar)].align(Align::Center).padding([0u16, 6]);

        let formulario = column![
            text("Cartão de vidro").size(20).font(Font::BOLD),
            text("Fundo desfocado, borda de luz e sombra: Style::glass(16.0).").size(13).color(t.muted),
            row![
                text_input("Seu nome", &self.nome).on_input(Msg::Nome).on_submit(Msg::Enviar),
                tooltip(button(text("Enviar").font(Font::SEMIBOLD)).on_press(Msg::Enviar).padding([7u16, 16]), "Também vai com Enter", TooltipPosition::Bottom),
            ]
            .spacing(8)
            .align(Align::Center),
            text(match &self.enviado {
                Some(n) => format!("Olá, {n}!"),
                None => "Nada enviado ainda.".into(),
            })
            .size(13),
            horizontal_rule(1.0),
            row![checkbox("Notificar atualizações", self.notificar).on_toggle(Msg::Notificar), horizontal_space(), toggler("Tema escuro", self.escuro).on_toggle(Msg::Escuro)].align(Align::Center),
            row![text("Formato").size(13), pick_list(&FORMATOS[..], self.formato, Msg::Formato).placeholder("Escolha…").width(140.0)].spacing(12).align(Align::Center),
            row![text("Volume").size(13), slider(0.0..=100.0, self.volume, Msg::Volume).step(1.0), text(format!("{:.0}", self.volume)).size(13).width(28.0)].spacing(12).align(Align::Center),
            progress_bar(0.0..=100.0, self.volume),
        ]
        .spacing(14);
        let cartao = container(formulario).padding(24.0).width(440.0).style(|_| Style::glass(16.0));

        let lista = scrollable(column((1..=30).map(|i| {
            container(row![text(format!("Item {i}")).size(13), horizontal_space(), text(if i % 3 == 0 { "pronto" } else { "na fila" }).size(12).color(t.muted)])
                .padding([8u16, 12])
                .width(Length::FILL)
                .style(move |t: &Theme| Style::default().background(t.surface.alpha(0.55)).border(Border::rounded(8.0)))
                .into()
        }))
        .spacing(6)
        .padding([0u16, 12]));
        let lateral = container(lista).width(260.0).height(Length::FILL).padding([12u16, 0]).clip(true).style(|_| Style::glass(16.0));

        let corpo = row![cartao, lateral].spacing(20).padding(20.0).height(Length::FILL);
        // Puxador no canto inferior direito: redimensiona sem moldura do sistema.
        let canto = container(mouse_area(Space::new(18.0, 18.0)).on_press(Msg::Redimensionar).cursor_icon(CursorIcon::SeResize)).center().align_x(Align::End).align_y(Align::End);
        // Bolhas de cor atrás do vidro, para o desfoque ter o que mostrar.
        let bolhas = canvas(Length::FILL, Length::FILL, |r, area, _| {
            for (x, y, raio, cor) in [(0.25, 0.35, 120.0, "#f97316"), (0.42, 0.8, 90.0, "#22c55e"), (0.85, 0.2, 110.0, "#ec4899")] {
                let c = Rect { x: area.x + area.width * x - raio, y: area.y + area.height * y - raio, width: raio * 2.0, height: raio * 2.0 };
                r.fill_quad(c, Some(&Color::hex(cor).unwrap_or(Color::WHITE).alpha(0.85).into()), Border::rounded(raio), None);
            }
        });
        container(stack![bolhas, column![barra, corpo], canto])
            .width(Length::FILL)
            .height(Length::FILL)
            .clip(true)
            .style(move |t: &Theme| {
                let (a, b) = if escuro { (Color::rgb8(0x3b, 0x1d, 0x5a), Color::rgb8(0x0b, 0x4a, 0x6e)) } else { (Color::rgb8(0xfb, 0xc2, 0xeb), Color::rgb8(0xa6, 0xc1, 0xee)) };
                Style::default()
                    .background(Background::Linear { angle: 0.6, stops: vec![(0.0, a.alpha(0.92)), (1.0, b.alpha(0.92))] })
                    .border(Border::rounded(14.0).color(Color::WHITE.alpha(0.15), 1.0))
                    .text_color(t.text)
            })
            .into()
    }
}

fn main() -> Result<(), Error> {
    let inicial = || Vitrine { nome: String::new(), enviado: None, notificar: true, escuro: true, formato: None, volume: 40.0 };
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let destino = args.get(i + 1).map_or("vitrine.png", String::as_str);
        // `--inspecionar x y`: a captura sai com o inspetor e o mouse nesse ponto.
        let inspecionar = args.iter().position(|a| a == "--inspecionar").and_then(|j| Some(Point::new(args.get(j + 1)?.parse().ok()?, args.get(j + 2)?.parse().ok()?)));
        let app = inicial();
        let png = capture(&app.view(), Size::new(900.0, 600.0), 1.0, &Theme { background: Color::TRANSPARENT, ..app.theme() }, &mut Renderer::new(), inspecionar);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    let settings = Settings {
        title: "Pelicano — vitrine".into(),
        size: Size::new(900.0, 600.0),
        min_size: Some(Size::new(720.0, 480.0)),
        decorations: false,
        transparent: true,
        blur: true,
        app_id: Some("pelicano-vitrine".into()),
        debug: true,
        ..Default::default()
    };
    run(settings, || (inicial(), Task::none()))
}
