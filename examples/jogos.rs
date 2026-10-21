//! O pack de jogos offline: um menu com todos os jogos na mesma janela, cada um com um botão de
//! volta. Os jogos moram em `pack/`; cada um também abre sozinho (`--example damas`, ...).
//! `cargo run --release --example jogos`; `-- --capturar x.png` só gera o PNG do menu.
use pelicano::{column, row};
use pelicano::*;

#[path = "pack/estilo.rs"]
mod estilo;
#[path = "pack/cartas.rs"]
mod cartas;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/damas.rs"]
mod damas;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/paciencia.rs"]
mod paciencia;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/buraco.rs"]
mod buraco;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/xadrez.rs"]
mod xadrez;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/poker.rs"]
mod poker;

use estilo::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Jogo {
    Paciencia,
    Buraco,
    Poker,
    Damas,
    Xadrez,
}

/// Nome, descrição e ícone de cada jogo, na ordem do menu.
const JOGOS: [(Jogo, &str, &str); 5] = [
    (Jogo::Paciencia, "PACIÊNCIA", "Klondike de uma em uma, sempre com solução"),
    (Jogo::Buraco, "BURACO", "Em duplas ou a dois, até 1000 pontos"),
    (Jogo::Poker, "POKER", "Texas Hold'em contra três bots"),
    (Jogo::Damas, "DAMAS", "Regras brasileiras contra o bot, três níveis"),
    (Jogo::Xadrez, "XADREZ", "Regras completas contra o bot, três níveis"),
];

fn icone<'a>(j: Jogo) -> Element<'a, Msg> {
    match j {
        Jogo::Paciencia => paciencia::icone(),
        Jogo::Buraco => buraco::icone(),
        Jogo::Poker => poker::icone(),
        Jogo::Damas => damas::icone(),
        Jogo::Xadrez => xadrez::icone(),
    }
}

enum Tela {
    Menu,
    Paciencia(paciencia::Paciencia),
    Buraco(buraco::Buraco),
    Poker(poker::Poker),
    Damas(damas::Damas),
    Xadrez(xadrez::Xadrez),
}

#[derive(Debug, Clone)]
enum Msg {
    Abrir(Jogo),
    Menu,
    Paciencia(paciencia::Msg),
    Buraco(buraco::Msg),
    Poker(poker::Msg),
    Damas(damas::Msg),
    Xadrez(xadrez::Msg),
}

struct Pack {
    tela: Tela,
}

/// O cartão de um jogo: sobe e acende a borda com o mouse em cima.
fn cartao<'a>(j: Jogo, nome: &'static str, descricao: &'static str) -> Element<'a, Msg> {
    let vitrine = container(icone(j)).center().width(180.0).height(120.0).style(|_| Style::default().background(FELTRO).border(Border::rounded(0.0).color(MADEIRA, PX)));
    let corpo = column![vitrine, texto(nome, 22.0, OURO), container(texto(descricao, 14.0, Color::WHITE.alpha(0.6))).width(180.0)].spacing(14);
    let botao = button(corpo).padding(16.0).on_press(Msg::Abrir(j)).style(|_, s| {
        let acesa = matches!(s, Status::Hovered | Status::Pressed);
        let d = if s == Status::Pressed { PX } else if acesa { 3.0 * PX } else { 2.0 * PX };
        Style::default().background(PAINEL).text_color(Color::WHITE).border(Border::rounded(0.0).color(if acesa { OURO } else { MARINHO }, PX)).shadow(Shadow { color: Color::BLACK.alpha(0.55), offset: Point::new(d, d), blur: 0.0 })
    });
    mouse_area(animate(botao).hover(Keyframe::default().offset(0.0, -PX * 1.5)).pressed(Keyframe::default().offset(PX, PX)).transition(Transition::new(120).easing(Easing::EaseOut)))
        .cursor_icon(CursorIcon::Pointer)
        .into()
}

impl Pack {
    fn menu(&self) -> Element<'_, Msg> {
        let titulo = container(text("JOGOS").font(LETREIRO).size(32.0).color(OURO)).padding([16u16, 28]).style(|_| placa());
        let cartoes = row(JOGOS.iter().map(|&(j, n, d)| cartao(j, n, d))).spacing(20).wrap();
        let tela = column![titulo, texto("PACK OFFLINE · ESCOLHA UM JOGO", 16.0, MENTA), Space::new(0.0, 12.0), cartoes, Space::new(0.0, 12.0), texto("FEITO COM PELICANO", 12.0, Color::WHITE.alpha(0.3))].spacing(18).align(Align::Center);
        container(container(tela).max_width(1180.0)).center().style(|_| Style::default().background(FUNDO)).into()
    }
}

/// O jogo com o botão de volta ao menu por cima, no canto.
fn com_volta<'a>(jogo: Element<'a, Msg>) -> Element<'a, Msg> {
    let volta = container(botao("‹ MENU", 14.0, Some(Msg::Menu), false)).width(120.0).padding(16.0);
    stack([jogo, volta.into()]).into()
}

impl App for Pack {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match (msg, &mut self.tela) {
            (Msg::Abrir(Jogo::Paciencia), _) => {
                let (j, t) = paciencia::iniciar();
                self.tela = Tela::Paciencia(j);
                t.map(Msg::Paciencia)
            }
            (Msg::Abrir(Jogo::Buraco), _) => {
                let (j, t) = buraco::iniciar();
                self.tela = Tela::Buraco(j);
                t.map(Msg::Buraco)
            }
            (Msg::Abrir(Jogo::Poker), _) => {
                let (j, t) = poker::iniciar();
                self.tela = Tela::Poker(j);
                t.map(Msg::Poker)
            }
            (Msg::Abrir(Jogo::Xadrez), _) => {
                let (j, t) = xadrez::iniciar();
                self.tela = Tela::Xadrez(j);
                t.map(Msg::Xadrez)
            }
            (Msg::Abrir(Jogo::Damas), _) => {
                let (j, t) = damas::iniciar();
                self.tela = Tela::Damas(j);
                t.map(Msg::Damas)
            }
            (Msg::Menu, _) => {
                self.tela = Tela::Menu;
                Task::none()
            }
            // Mensagens de um jogo já fechado (relógio, bot pensando) caem aqui e somem.
            (Msg::Paciencia(m), Tela::Paciencia(j)) => j.update(m).map(Msg::Paciencia),
            (Msg::Damas(m), Tela::Damas(j)) => j.update(m).map(Msg::Damas),
            (Msg::Buraco(m), Tela::Buraco(j)) => j.update(m).map(Msg::Buraco),
            (Msg::Poker(m), Tela::Poker(j)) => j.update(m).map(Msg::Poker),
            (Msg::Xadrez(m), Tela::Xadrez(j)) => j.update(m).map(Msg::Xadrez),
            _ => Task::none(),
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        match &self.tela {
            Tela::Menu => self.menu(),
            Tela::Paciencia(j) => com_volta(j.view().map(Msg::Paciencia)),
            Tela::Damas(j) => com_volta(j.view().map(Msg::Damas)),
            Tela::Buraco(j) => com_volta(j.view().map(Msg::Buraco)),
            Tela::Poker(j) => com_volta(j.view().map(Msg::Poker)),
            Tela::Xadrez(j) => com_volta(j.view().map(Msg::Xadrez)),
        }
    }
}

fn main() -> Result<(), Error> {
    carregar_fontes();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let png = capture(&Pack { tela: Tela::Menu }.view(), Size::new(1240.0, 940.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("jogos.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Jogos".into(), size: Size::new(1240.0, 940.0), min_size: Some(Size::new(1200.0, 900.0)), ..Default::default() }, || (Pack { tela: Tela::Menu }, Task::none()))
}
