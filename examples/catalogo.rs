//! Todos os widgets no estilo padrão, numa janela padrão. O dropdown ao lado de
//! cada um aplica uma personalização, para comparar com o padrão.
//! `cargo run --example catalogo`; `-- --capturar c.png` só gera o PNG.
use pelicano::column;
use pelicano::*;

const BOTAO: [&str; 7] = ["Padrão", "Pílula", "Contorno", "Texto", "Gradiente", "Sombra", "Vidro"];
const CAMPO: [&str; 5] = ["Padrão", "Sublinhado", "Pílula", "Sem borda", "Vidro"];
const MARCA: [&str; 3] = ["Padrão", "Verde", "Laranja"];
const SLIDER: [&str; 3] = ["Padrão", "Verde", "Rosa"];
const BARRA: [&str; 4] = ["Padrão", "Grossa", "Fina", "Verde"];
const CAIXA: [&str; 7] = ["Nenhum", "Cartão", "Contorno", "Gradiente", "Sombra forte", "Cantos diferentes", "Vidro"];
const TEXTO: [&str; 5] = ["Padrão", "Negrito", "Grande", "Mudo", "Mono"];
const REGUA: [&str; 3] = ["Padrão", "Grossa", "Colorida"];
const OPCOES: [&str; 3] = ["Opção A", "Opção B", "Opção C"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum W {
    Botao,
    Campo,
    Checkbox,
    Toggler,
    Slider,
    Barra,
    Caixa,
    Texto,
    Regua,
    Radio,
}

#[derive(Debug, Clone)]
enum Msg {
    Estilo(W, &'static str),
    Clique,
    Texto(String),
    Marcar(bool),
    Ligar(bool),
    Opcao(&'static str),
    Valor(f32),
    Data(Date),
    Periodo(Date, Date),
    Cor(Color),
    Radio(u8),
    Aba(usize),
    Numero(f64),
}

struct Catalogo {
    estilos: Vec<(W, &'static str)>,
    cliques: u32,
    texto: String,
    marcado: bool,
    ligado: bool,
    opcao: Option<&'static str>,
    valor: f32,
    data: Option<Date>,
    periodo: Option<(Date, Date)>,
    cor: Color,
    radio: Option<u8>,
    aba: usize,
    numero: f64,
}

impl Catalogo {
    fn estilo(&self, w: W) -> &'static str {
        self.estilos.iter().find(|e| e.0 == w).map_or("Padrão", |e| e.1)
    }

    /// Uma linha do catálogo: nome, o widget e o dropdown da personalização.
    fn linha<'a>(&self, nome: &'a str, w: W, opcoes: &'static [&'static str], widget: Element<'a, Msg>) -> Element<'a, Msg> {
        row![
            text(nome).size(13).width(130.0),
            container(widget).width(Length::FILL),
            pick_list(opcoes, Some(self.estilo(w)), move |v| Msg::Estilo(w, v)).width(170.0),
        ]
        .spacing(16)
        .align(Align::Center)
        .padding([10u16, 0])
        .into()
    }
}

fn botao(e: &str) -> impl Fn(&Theme, Status) -> Style + '_ {
    move |t, s| {
        let hover = s == Status::Hovered;
        let base = button_primary(t, s);
        match e {
            "Pílula" => base.border(Border::rounded(999.0)),
            "Contorno" => Style::default().border(Border::rounded(6.0).color(t.primary, 1.5)).text_color(t.primary).background(if hover { t.primary.alpha(0.1) } else { Color::TRANSPARENT }),
            "Texto" => button_text(t, s),
            "Gradiente" => base.background(Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(0x8b, 0x5c, 0xf6)), (1.0, Color::rgb8(0xec, 0x48, 0x99))] }).border(Border::rounded(8.0)),
            "Sombra" => base.shadow(Shadow { color: t.primary.alpha(if hover { 0.6 } else { 0.4 }), offset: Point::new(0.0, 4.0), blur: if hover { 16.0 } else { 10.0 } }),
            "Vidro" => Style::glass(8.0).text_color(t.text),
            _ => base,
        }
    }
}

fn campo(e: &str) -> impl Fn(&Theme, Status) -> Style + '_ {
    move |t, s| {
        let base = input_default(t, s);
        let foco = s == Status::Focused;
        match e {
            "Sublinhado" => Style::default().text_color(t.text).border(Border::default().color(Color::TRANSPARENT, 0.0)).background(Background::Linear {
                angle: std::f32::consts::FRAC_PI_2,
                stops: vec![(0.0, Color::TRANSPARENT), (0.94, Color::TRANSPARENT), (0.95, if foco { t.primary } else { t.border }), (1.0, if foco { t.primary } else { t.border })],
            }),
            "Pílula" => base.border(Border::rounded(999.0).color(if foco { t.primary } else { t.border }, 1.0)),
            "Sem borda" => Style::default().background(t.text.alpha(if foco { 0.1 } else { 0.06 })).border(Border::rounded(6.0)).text_color(t.text),
            "Vidro" => Style::glass(8.0).text_color(t.text),
            _ => base,
        }
    }
}

fn cor(e: &str) -> Option<Color> {
    match e {
        "Verde" => Color::hex("#22c55e"),
        "Laranja" => Color::hex("#f97316"),
        "Rosa" => Color::hex("#ec4899"),
        _ => None,
    }
}

impl App for Catalogo {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Estilo(w, e) => {
                self.estilos.retain(|x| x.0 != w);
                self.estilos.push((w, e));
            }
            Msg::Clique => self.cliques += 1,
            Msg::Texto(t) => self.texto = t,
            Msg::Marcar(b) => self.marcado = b,
            Msg::Ligar(b) => self.ligado = b,
            Msg::Opcao(o) => self.opcao = Some(o),
            Msg::Valor(v) => self.valor = v,
            Msg::Data(d) => self.data = Some(d),
            Msg::Periodo(a, b) => self.periodo = Some((a, b)),
            Msg::Cor(c) => self.cor = c,
            Msg::Radio(r) => self.radio = Some(r),
            Msg::Aba(a) => self.aba = a,
            Msg::Numero(n) => self.numero = n,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let e = |w| self.estilo(w);
        let mut checkbox = checkbox("Aceito os termos", self.marcado).on_toggle(Msg::Marcar);
        let mut toggler = toggler("Notificações", self.ligado).on_toggle(Msg::Ligar);
        let mut slider = slider(0.0..=100.0, self.valor, Msg::Valor).width(260.0);
        if let Some(c) = cor(e(W::Checkbox)) {
            checkbox = checkbox.color(c);
        }
        if let Some(c) = cor(e(W::Toggler)) {
            toggler = toggler.color(c);
        }
        if let Some(c) = cor(e(W::Slider)) {
            slider = slider.color(c);
        }
        let mut barra = progress_bar(0.0..=100.0, self.valor).width(260.0);
        barra = match e(W::Barra) {
            "Grossa" => barra.height(14.0),
            "Fina" => barra.height(2.0),
            "Verde" => barra.color(Color::rgb8(0x22, 0xc5, 0x5e)),
            _ => barra,
        };
        let caixa_estilo = e(W::Caixa);
        let caixa = container(text("Conteúdo da caixa").size(13)).padding(16.0).width(260.0).style(move |t| match caixa_estilo {
            "Cartão" => Style::default().background(t.surface).border(Border::rounded(10.0)),
            "Contorno" => Style::default().border(Border::rounded(10.0).color(t.border, 1.0)),
            "Gradiente" => Style::default().background(Background::Linear { angle: 0.4, stops: vec![(0.0, Color::rgb8(0x0e, 0xa5, 0xe9)), (1.0, Color::rgb8(0x63, 0x66, 0xf1))] }).border(Border::rounded(10.0)).text_color(Color::WHITE),
            "Sombra forte" => Style::default().background(t.surface).border(Border::rounded(10.0)).shadow(Shadow { color: Color::BLACK.alpha(0.5), offset: Point::new(0.0, 8.0), blur: 24.0 }),
            "Cantos diferentes" => Style::default().background(t.surface).border(Border { radius: [0.0, 24.0, 0.0, 24.0], ..Border::default().color(t.primary, 2.0) }),
            "Vidro" => Style::glass(10.0),
            _ => Style::default(),
        });
        let texto = text("O rato roeu a roupa do rei de Roma");
        let texto = match e(W::Texto) {
            "Negrito" => texto.font(Font::BOLD),
            "Grande" => texto.size(22),
            "Mudo" => texto.color(self.theme().muted),
            "Mono" => texto.font(Font::MONO),
            _ => texto,
        };
        let cor_radio = cor(e(W::Radio));
        let radio = |rotulo: &'static str, v: u8| {
            let r = radio(rotulo, v, self.radio, Msg::Radio);
            match cor_radio {
                Some(c) => r.color(c),
                None => r,
            }
        };
        let regua = match e(W::Regua) {
            "Grossa" => horizontal_rule(4.0),
            "Colorida" => horizontal_rule(2.0).color(Color::rgb8(0xec, 0x48, 0x99)),
            _ => horizontal_rule(1.0),
        };
        let itens = column![
            text("Catálogo").size(24).font(Font::BOLD),
            text("Tudo no estilo padrão. Escolha uma personalização ao lado de cada widget.").size(13).color(self.theme().muted),
            self.linha("Botão", W::Botao, &BOTAO, row![button(text("Clique")).on_press(Msg::Clique).style(botao(e(W::Botao))), text(format!("{} cliques", self.cliques)).size(13)].spacing(12).align(Align::Center).into()),
            self.linha("Campo de texto", W::Campo, &CAMPO, text_input("Digite algo…", &self.texto).on_input(Msg::Texto).width(260.0).style(campo(e(W::Campo))).into()),
            row![text("Data").size(13).width(130.0), date_picker(self.data, Msg::Data), text(self.data.map_or("nenhuma".into(), |d| format!("escolhida: {d}"))).size(12).color(self.theme().muted)].spacing(16).align(Align::Center).padding([10u16, 0]),
            row![text("Período").size(13).width(130.0), date_range_picker(self.periodo.map(|p| p.0), self.periodo.map(|p| p.1), Msg::Periodo), text(self.periodo.map_or("nenhum".into(), |(a, b)| format!("{} dias", dias_entre(a, b) + 1))).size(12).color(self.theme().muted)].spacing(16).align(Align::Center).padding([10u16, 0]),
            row![text("Cor").size(13).width(130.0), color_picker(self.cor, Msg::Cor), container(Space::new(28.0, 28.0)).style({ let c = self.cor; move |_| Style::default().background(c).border(Border::rounded(6.0)) })].spacing(16).align(Align::Center).padding([10u16, 0]),
            self.linha("Checkbox", W::Checkbox, &MARCA, checkbox.into()),
            self.linha("Toggler", W::Toggler, &MARCA, toggler.into()),
            self.linha("Radio", W::Radio, &MARCA, row![radio("Pequeno", 1), radio("Médio", 2), radio("Grande", 3)].spacing(18).into()),
            row![text("Abas").size(13).width(130.0), container(tabs(&["Geral", "Avançado", "Sobre"], self.aba, Msg::Aba)).width(320.0)].spacing(16).align(Align::Center).padding([10u16, 0]),
            row![text("Número").size(13).width(130.0), number_input(self.numero, 0.0..=10.0, 1.0, Msg::Numero)].spacing(16).align(Align::Center).padding([10u16, 0]),
            self.linha("Slider", W::Slider, &SLIDER, row![slider, text(format!("{:.0}", self.valor)).size(13)].spacing(12).align(Align::Center).into()),
            self.linha("Barra de progresso", W::Barra, &BARRA, barra.into()),
            self.linha("Caixa (container)", W::Caixa, &CAIXA, caixa.into()),
            self.linha("Texto", W::Texto, &TEXTO, texto.into()),
            self.linha("Régua", W::Regua, &REGUA, container(regua).width(260.0).into()),
            row![text("Dropdown").size(13).width(130.0), pick_list(&OPCOES[..], self.opcao, Msg::Opcao).placeholder("Escolha…").width(200.0), text("ainda sem estilo próprio").size(12).color(self.theme().muted)].spacing(16).align(Align::Center).padding([10u16, 0]),
            row![text("Tooltip").size(13).width(130.0), tooltip(button(text("Passe o mouse")).style(button_text).on_press(Msg::Clique), "Eu sou uma tooltip", TooltipPosition::Bottom)].spacing(16).align(Align::Center).padding([10u16, 0]),
        ]
        .spacing(4)
        .padding([24u16, 32]);
        scrollable(itens).into()
    }
}

/// Dias de `a` até `b`, contando pela diferença no calendário.
fn dias_entre(a: Date, b: Date) -> i64 {
    let dias = |d: Date| -> i64 { (1..d.month).map(|m| days_in_month(d.year, m) as i64).sum::<i64>() + d.day as i64 + (1970..d.year).map(|y| if days_in_month(y, 2) == 29 { 366 } else { 365 }).sum::<i64>() };
    dias(b) - dias(a)
}

fn main() -> Result<(), Error> {
    let app = || Catalogo { estilos: Vec::new(), cliques: 0, texto: String::new(), marcado: false, ligado: true, opcao: None, valor: 40.0, data: None, periodo: None, cor: Color::rgb8(0x3b, 0x82, 0xf6), radio: Some(2), aba: 0, numero: 3.0 };
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let mut a = app();
        // `--estilos`: a captura sai com uma personalização em cada widget.
        if args.iter().any(|x| x == "--estilos") {
            a.estilos = vec![(W::Botao, "Gradiente"), (W::Campo, "Pílula"), (W::Checkbox, "Verde"), (W::Toggler, "Laranja"), (W::Slider, "Rosa"), (W::Barra, "Grossa"), (W::Caixa, "Cantos diferentes"), (W::Texto, "Negrito"), (W::Regua, "Colorida")];
        }
        let png = capture(&a.view(), Size::new(900.0, 1100.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("catalogo.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Catálogo — Pelicano".into(), size: Size::new(900.0, 760.0), ..Default::default() }, || (app(), Task::none()))
}
