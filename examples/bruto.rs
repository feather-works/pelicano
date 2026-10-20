//! O pelicano cru: todo widget com os valores padrão, sem estilo, espaçamento nem largura.
//! `cargo run --example bruto`
use pelicano::column;
use pelicano::*;

#[derive(Debug, Clone)]
enum Msg {
    Botao,
    Texto(String),
    Marcar(bool),
    Ligar(bool),
    Radio(u8),
    Opcao(&'static str),
    Valor(f32),
    Data(Date),
    Periodo(Date, Date),
    Cor(Color),
    Aba(usize),
    Numero(f64),
}

#[derive(Default)]
struct Bruto {
    texto: String,
    marcado: bool,
    ligado: bool,
    radio: Option<u8>,
    opcao: Option<&'static str>,
    valor: f32,
    data: Option<Date>,
    periodo: Option<(Date, Date)>,
    cor: Option<Color>,
    aba: usize,
    numero: f64,
}

const OPCOES: [&str; 3] = ["A", "B", "C"];

impl App for Bruto {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Botao => {}
            Msg::Texto(t) => self.texto = t,
            Msg::Marcar(b) => self.marcado = b,
            Msg::Ligar(b) => self.ligado = b,
            Msg::Radio(r) => self.radio = Some(r),
            Msg::Opcao(o) => self.opcao = Some(o),
            Msg::Valor(v) => self.valor = v,
            Msg::Data(d) => self.data = Some(d),
            Msg::Periodo(a, b) => self.periodo = Some((a, b)),
            Msg::Cor(c) => self.cor = Some(c),
            Msg::Aba(a) => self.aba = a,
            Msg::Numero(n) => self.numero = n,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        scrollable(column![
            text("texto"),
            button("botão").on_press(Msg::Botao),
            button("botão desabilitado"),
            text_input("campo de texto", &self.texto).on_input(Msg::Texto),
            checkbox("checkbox", self.marcado).on_toggle(Msg::Marcar),
            toggler("toggler", self.ligado).on_toggle(Msg::Ligar),
            radio("radio 1", 1, self.radio, Msg::Radio),
            radio("radio 2", 2, self.radio, Msg::Radio),
            pick_list(&OPCOES[..], self.opcao, Msg::Opcao),
            slider(0.0..=100.0, self.valor, Msg::Valor),
            progress_bar(0.0..=100.0, self.valor),
            date_picker(self.data, Msg::Data),
            date_range_picker(self.periodo.map(|p| p.0), self.periodo.map(|p| p.1), Msg::Periodo),
            color_picker(self.cor.unwrap_or(Color::BLACK), Msg::Cor),
            tabs(&["aba 1", "aba 2"], self.aba, Msg::Aba),
            number_input(self.numero, 0.0..=10.0, 1.0, Msg::Numero),
            horizontal_rule(1.0),
            tooltip(text("tooltip (passe o mouse)"), "dica", TooltipPosition::Bottom),
            container(text("container")),
        ])
        .into()
    }
}

fn main() -> Result<(), Error> {
    if let Some(i) = std::env::args().position(|a| a == "--capturar") {
        let destino = std::env::args().nth(i + 1).unwrap_or("bruto.png".into());
        let png = capture(&Bruto::default().view(), Settings::default().size, 1.0, &Theme::dark(), &mut Renderer::new(), None);
        std::fs::write(&destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings::default(), || (Bruto::default(), Task::none()))
}
