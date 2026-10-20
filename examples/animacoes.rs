//! Vitrine das animações: cartões que crescem no hover e afundam no clique, carregando que gira,
//! "ao vivo" que pulsa, lista em cascata, aviso que entra pela lateral, painel que surge e some,
//! e os controles prontos (menus, calendário, dicas, caixas, chave, slider) já animados.
//! `cargo run --release --example animacoes`
use pelicano::{column, row};
use pelicano::*;
use std::time::Duration;

#[derive(Debug, Clone)]
enum Msg {
    Escolher(usize),
    Lista,
    Painel,
    Notificar,
    FecharAviso(u32),
    Ligar(bool),
    Marcar(bool),
    Radio(u8),
    Valor(f32),
    Opcao(&'static str),
    Data(Date),
    Aba(usize),
}

struct Animacoes {
    escolhido: usize,
    lista: bool,
    painel: bool,
    aviso: Option<u32>,
    avisos: u32,
    ligado: bool,
    marcado: bool,
    radio: Option<u8>,
    valor: f32,
    opcao: Option<&'static str>,
    data: Option<Date>,
    aba: usize,
}

impl Default for Animacoes {
    fn default() -> Self {
        Animacoes { escolhido: 0, lista: true, painel: false, aviso: None, avisos: 0, ligado: false, marcado: false, radio: Some(1), valor: 40.0, opcao: None, data: None, aba: 0 }
    }
}

const CARTOES: [(&str, &str, (u8, u8, u8)); 3] = [("Música", "Toque o que está tocando", (236, 72, 153)), ("Fotos", "Suas lembranças em ordem", (59, 130, 246)), ("Jogos", "Biblioteca e conquistas", (34, 197, 94))];
const TAREFAS: [&str; 5] = ["Baixar atualização", "Sincronizar notas", "Limpar a lixeira", "Revisar o calendário", "Fazer backup"];
const OPCOES: [&str; 4] = ["Suave", "Rápida", "Elástica", "Sem animação"];

/// O cartão: degradê da cor dele, que acende no hover e ganha borda quando escolhido. A
/// transição do estilo faz a cor andar; o `animate` em volta faz ele crescer e afundar.
fn cartao(cor: Color, escolhido: bool) -> impl Fn(&Theme, Status) -> Style {
    move |_, s| {
        let luz = if s == Status::Hovered { 0.12 } else { 0.0 };
        let (a, b) = (cor.mix(Color::WHITE, luz), cor.mix(Color::BLACK, 0.45 - luz));
        let borda = if escolhido { Color::WHITE.alpha(0.9) } else { Color::WHITE.alpha(0.0) };
        let sombra = Shadow { color: cor.alpha(if s == Status::Hovered { 0.55 } else { 0.25 }), offset: Point::new(0.0, if s == Status::Hovered { 14.0 } else { 6.0 }), blur: if s == Status::Hovered { 28.0 } else { 14.0 } };
        Style::default()
            .background(Background::Linear { angle: 1.0, stops: vec![(0.0, a), (1.0, b)] })
            .border(Border::rounded(16.0).color(borda, 2.0))
            .shadow(sombra)
            .text_color(Color::WHITE)
            .transition(Transition::new(220).easing(Easing::EaseOut))
    }
}

/// Três quartos de círculo: girando vira o "carregando".
fn roda<'a>() -> Element<'a, Msg> {
    canvas(22.0, 22.0, |r, area, t| {
        let (cx, cy, raio) = (area.x + area.width / 2.0, area.y + area.height / 2.0, area.width / 2.0 - 2.0);
        let pontos: Vec<Point> = (0..=36).map(|i| (i as f32 / 36.0) * 1.5 * std::f32::consts::PI).map(|a| Point::new(cx + raio * a.cos(), cy + raio * a.sin())).collect();
        r.stroke(&pontos, 3.0, t.primary);
    })
    .into()
}

fn secao<'a>(titulo: &'a str, conteudo: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(column![text(titulo).size(13).color(Color::WHITE.alpha(0.6)), conteudo.into()].spacing(12)).padding(20.0).width(Length::FILL).style(|_| Style::glass(14.0)).into()
}

impl App for Animacoes {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Escolher(i) => self.escolhido = i,
            Msg::Lista => self.lista = !self.lista,
            Msg::Painel => self.painel = !self.painel,
            Msg::Notificar => {
                self.avisos += 1;
                self.aviso = Some(self.avisos);
                let n = self.avisos;
                // Some sozinho depois de um tempo (só se nenhum aviso mais novo tomou o lugar).
                return Task::perform(async { std::thread::sleep(Duration::from_millis(2500)) }, move |_| Msg::FecharAviso(n));
            }
            Msg::FecharAviso(n) => {
                if self.aviso == Some(n) {
                    self.aviso = None;
                }
            }
            Msg::Ligar(b) => self.ligado = b,
            Msg::Marcar(b) => self.marcado = b,
            Msg::Radio(r) => self.radio = Some(r),
            Msg::Valor(v) => self.valor = v,
            Msg::Opcao(o) => self.opcao = Some(o),
            Msg::Data(d) => self.data = Some(d),
            Msg::Aba(a) => self.aba = a,
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        // "Ao vivo": um ponto fixo com um halo que cresce e se apaga, sem parar.
        let ponto = || container(Space::new(10.0, 10.0)).style(|t| Style::default().background(t.danger).border(Border::rounded(5.0)));
        let halo = animate(ponto()).scale(2.6).opacity(0.0).repeat(Repeat::Forever).transition(Transition::new(1200).easing(Easing::EaseOut));
        let ao_vivo = row![stack![halo, ponto()], text("ao vivo").size(13)].spacing(10).align(Align::Center);
        let carregando = row![animate(roda()).rotate(360.0).repeat(Repeat::Forever).transition(Transition::new(900).easing(Easing::Linear)), text("sincronizando").size(13)].spacing(8).align(Align::Center);
        let topo = row![text("Animações").size(26).font(Font::SEMIBOLD), horizontal_space(), ao_vivo, carregando].spacing(24).align(Align::Center);

        let cartoes = row(CARTOES.iter().enumerate().map(|(i, (nome, desc, (r, g, b)))| {
            let corpo = column![text(*nome).size(20).font(Font::SEMIBOLD), text(*desc).size(13)].spacing(6).width(Length::FILL);
            let botao = button(corpo).padding(22.0).width(Length::FILL).on_press(Msg::Escolher(i)).style(cartao(Color::rgb8(*r, *g, *b), i == self.escolhido));
            animate(botao).hover(Keyframe::default().scale(1.04).offset(0.0, -4.0)).pressed(Keyframe::default().scale(0.97)).transition(Transition::new(180).easing(Easing::EaseOut)).into()
        }))
        .spacing(16);

        // Cascata: cada item espera um pouco mais que o de cima, na entrada e na saída.
        let lista = column(TAREFAS.iter().enumerate().map(|(i, t)| {
            let item = container(row![container(Space::new(8.0, 8.0)).style(|t| Style::default().background(t.primary).border(Border::rounded(4.0))), text(*t).size(14)].spacing(10).align(Align::Center)).padding([8u16, 12]).width(Length::FILL).style(|_| Style::default().background(Color::WHITE.alpha(0.06)).border(Border::rounded(8.0)));
            animate(item)
                .show(self.lista)
                .enter(Keyframe::default().opacity(0.0).offset(-24.0, 0.0))
                .exit(Keyframe::default().opacity(0.0).offset(24.0, 0.0))
                .transition(Transition::new(260).easing(Easing::EaseOut).delay(i as u64 * 60))
                .into()
        }))
        .spacing(6);
        let acoes = row![
            button(if self.lista { "Esconder lista" } else { "Mostrar lista" }).on_press(Msg::Lista),
            button(if self.painel { "Fechar painel" } else { "Abrir painel" }).on_press(Msg::Painel),
            button("Notificar").on_press(Msg::Notificar),
        ]
        .spacing(10);

        let controles = column![
            tabs(&["Geral", "Avançado", "Sobre"], self.aba, Msg::Aba),
            row![
                toggler("Modo escuro", self.ligado).on_toggle(Msg::Ligar),
                checkbox("Lembrar de mim", self.marcado).on_toggle(Msg::Marcar),
                radio("Um", 1, self.radio, Msg::Radio),
                radio("Dois", 2, self.radio, Msg::Radio),
            ]
            .spacing(20)
            .align(Align::Center),
            row![slider(0.0..=100.0, self.valor, Msg::Valor).width(220.0), progress_bar(0.0..=100.0, self.valor).width(Length::FILL)].spacing(16).align(Align::Center),
            row![
                pick_list(&OPCOES[..], self.opcao, Msg::Opcao).placeholder("Velocidade").width(180.0),
                date_picker(self.data, Msg::Data),
                tooltip(button("Passe o mouse").style(button_text), "A dica aparece depois de um instante", TooltipPosition::Top),
            ]
            .spacing(12)
            .align(Align::Center),
        ]
        .spacing(16);

        let conteudo = column![
            topo,
            secao("CARTÕES · hover cresce e sobe, clique afunda, escolhido ganha borda", cartoes),
            secao("CASCATA · entrada e saída com atraso por item", column![acoes, lista].spacing(14)),
            secao("CONTROLES PRONTOS · menus e calendário surgem, caixas enchem, slider cresce, barra escorrega", controles),
        ]
        .spacing(18)
        .padding(28.0)
        .width(Length::FILL);

        let fundo = container(Space::new(Length::FILL, Length::FILL)).style(|_| Style::default().background(Background::Linear { angle: 0.8, stops: vec![(0.0, Color::rgb8(30, 16, 70)), (0.5, Color::rgb8(14, 30, 70)), (1.0, Color::rgb8(8, 60, 90))] }));

        // O aviso entra pela direita e sai pela direita; escondido, não pega clique.
        let aviso = animate(container(row![text("✓").size(16), text(format!("Aviso nº {} enviado", self.aviso.unwrap_or(self.avisos))).size(14)].spacing(10)).padding([12u16, 18]).style(|t| Style::default().background(t.surface).border(Border::rounded(10.0).color(t.border, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.4), offset: Point::new(0.0, 8.0), blur: 20.0 })))
            .show(self.aviso.is_some())
            .enter(Keyframe::default().opacity(0.0).offset(80.0, 0.0))
            .exit(Keyframe::default().opacity(0.0).offset(80.0, 0.0))
            .transition(Transition::new(320).easing(Easing::CubicBezier(0.2, 1.4, 0.4, 1.0)));
        let canto = container(aviso).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End).padding(24.0);

        // O painel cresce do centro e some encolhendo.
        let painel = animate(container(column![text("Painel de vidro").size(20).font(Font::SEMIBOLD), text("Surgiu crescendo do centro; feche e ele encolhe e some.").size(14), button("Fechar").on_press(Msg::Painel)].spacing(12)).padding(28.0).style(|_| Style::glass(18.0)))
            .show(self.painel)
            .enter(Keyframe::default().opacity(0.0).scale(0.85))
            .exit(Keyframe::default().opacity(0.0).scale(0.9))
            .transition(Transition::new(260).easing(Easing::EaseOut));
        let meio = container(painel).center();

        stack![fundo, scrollable(conteudo).height(Length::FILL), canto, meio].into()
    }
}

fn main() -> Result<(), Error> {
    run(Settings { title: "Animações".into(), size: Size::new(960.0, 760.0), ..Default::default() }, || (Animacoes::default(), Task::none()))
}
