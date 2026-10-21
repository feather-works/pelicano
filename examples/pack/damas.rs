//! Damas brasileiras contra o bot, no visual do pack de jogos (Baralho Pixelado): fundo #1C1A2B,
//! dourado, moldura de madeira, sombras duras e a fonte Press Start 2P (a Pixelify Sans nos textos com acento). As peças são pixel art
//! desenhada aqui mesmo.
//! Regras: captura obrigatória e da maior quantidade, peão captura para trás, dama anda longe.
//! `cargo run --release --example damas`; `-- --capturar x.png` só gera o PNG.
//! shortcut: sem empate (repetição, 20 lances só de damas); entra quando o pack virar jogo publicado.
use pelicano::column;
use pelicano::*;
use std::time::Duration;

use crate::estilo::*;

const CASA: f32 = 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cor {
    Branca,
    Vermelha,
}

impl Cor {
    fn outra(self) -> Cor {
        if self == Cor::Branca { Cor::Vermelha } else { Cor::Branca }
    }
    /// Topo, lateral e brilho da peça.
    fn tintas(self) -> (Color, Color, Color) {
        match self {
            Cor::Branca => (Color::rgb8(0xec, 0xea, 0xf0), Color::rgb8(0xb9, 0xbf, 0xcc), Color::WHITE),
            Cor::Vermelha => (Color::rgb8(0xe2, 0x3b, 0x45), Color::rgb8(0x9e, 0x1e, 0x33), Color::rgb8(0xf7, 0xa6, 0xbc)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Peca {
    cor: Cor,
    dama: bool,
}

pub(crate) type Casa = (i8, i8); // (coluna 0..8 = a..h, linha 0..8 = 1..8); as escuras têm coluna + linha par
pub(crate) type Tabuleiro = [[Option<Peca>; 8]; 8];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Lance {
    /// Origem, cada pouso e o destino.
    caminho: Vec<Casa>,
    capturas: Vec<Casa>,
}

impl Lance {
    fn de(&self) -> Casa {
        self.caminho[0]
    }
    fn para(&self) -> Casa {
        *self.caminho.last().expect("lance tem origem")
    }
}

const DIAGONAIS: [(i8, i8); 4] = [(1, 1), (-1, 1), (1, -1), (-1, -1)];

fn inicial() -> Tabuleiro {
    let mut t = [[None; 8]; 8];
    for l in 0..8 {
        for c in 0..8 {
            if (c + l) % 2 == 0 && l != 3 && l != 4 {
                t[l][c] = Some(Peca { cor: if l < 3 { Cor::Branca } else { Cor::Vermelha }, dama: false });
            }
        }
    }
    t
}

fn dentro((c, l): Casa) -> bool {
    (0..8).contains(&c) && (0..8).contains(&l)
}

fn em(t: &Tabuleiro, (c, l): Casa) -> Option<Peca> {
    t[l as usize][c as usize]
}

/// Os lances permitidos a `cor`: havendo captura, só as que capturam mais.
fn lances(t: &Tabuleiro, cor: Cor) -> Vec<Lance> {
    let pecas: Vec<(Casa, Peca)> = (0..8).flat_map(|l| (0..8).map(move |c| (c, l))).filter_map(|c| em(t, c).filter(|p| p.cor == cor).map(|p| (c, p))).collect();
    let mut capturas = Vec::new();
    for &(de, p) in &pecas {
        saltos(t, p, &mut vec![de], &mut Vec::new(), &mut capturas);
    }
    if let Some(max) = capturas.iter().map(|l: &Lance| l.capturas.len()).max() {
        capturas.retain(|l| l.capturas.len() == max);
        return capturas;
    }
    let mut v = Vec::new();
    for &(de, p) in &pecas {
        for (dc, dl) in DIAGONAIS {
            // Peão só anda para a frente; dama desliza.
            if !p.dama && dl != if cor == Cor::Branca { 1 } else { -1 } {
                continue;
            }
            let mut c = (de.0 + dc, de.1 + dl);
            while dentro(c) && em(t, c).is_none() {
                v.push(Lance { caminho: vec![de, c], capturas: Vec::new() });
                if !p.dama {
                    break;
                }
                c = (c.0 + dc, c.1 + dl);
            }
        }
    }
    v
}

/// Continua a captura a partir do fim de `caminho`. As capturadas só saem no fim do lance:
/// até lá ninguém pula por cima delas de novo.
fn saltos(t: &Tabuleiro, p: Peca, caminho: &mut Vec<Casa>, capturas: &mut Vec<Casa>, saida: &mut Vec<Lance>) {
    let origem = caminho[0];
    let ocupante = |c: Casa| if c == origem { None } else { em(t, c) };
    let de = *caminho.last().expect("caminho tem origem");
    let mut continuou = false;
    for (dc, dl) in DIAGONAIS {
        let mut alvo = (de.0 + dc, de.1 + dl);
        if p.dama {
            while dentro(alvo) && ocupante(alvo).is_none() {
                alvo = (alvo.0 + dc, alvo.1 + dl);
            }
        }
        if !dentro(alvo) || capturas.contains(&alvo) || ocupante(alvo).is_none_or(|x| x.cor == p.cor) {
            continue;
        }
        let mut pouso = (alvo.0 + dc, alvo.1 + dl);
        while dentro(pouso) && ocupante(pouso).is_none() {
            continuou = true;
            caminho.push(pouso);
            capturas.push(alvo);
            saltos(t, p, caminho, capturas, saida);
            caminho.pop();
            capturas.pop();
            if !p.dama {
                break;
            }
            pouso = (pouso.0 + dc, pouso.1 + dl);
        }
    }
    if !continuou && !capturas.is_empty() {
        saida.push(Lance { caminho: caminho.clone(), capturas: capturas.clone() });
    }
}

fn aplicar(t: &Tabuleiro, l: &Lance) -> Tabuleiro {
    let mut n = *t;
    let mut p = em(t, l.de()).expect("lance sai de uma peça");
    let (c, li) = l.para();
    // Só vira dama quem termina o lance na última linha.
    if li == if p.cor == Cor::Branca { 7 } else { 0 } {
        p.dama = true;
    }
    n[l.de().1 as usize][l.de().0 as usize] = None;
    for &(cc, cl) in &l.capturas {
        n[cl as usize][cc as usize] = None;
    }
    n[li as usize][c as usize] = Some(p);
    n
}

// O bot: negamax com poda alfa-beta.

fn avaliar(t: &Tabuleiro, cor: Cor) -> i32 {
    let mut s = 0;
    for l in 0..8 {
        for c in 0..8 {
            if let Some(p) = t[l][c] {
                let avanco = if p.cor == Cor::Branca { l } else { 7 - l } as i32;
                let centro = if (2..6).contains(&c) { 3 } else { 0 };
                let v = if p.dama { 300 } else { 100 + avanco * 5 } + centro;
                s += if p.cor == cor { v } else { -v };
            }
        }
    }
    s
}

fn negamax(t: &Tabuleiro, cor: Cor, prof: i32, mut a: i32, b: i32) -> i32 {
    let ls = lances(t, cor);
    if ls.is_empty() {
        return -(10_000 + prof); // perder mais tarde é menos ruim
    }
    // Com captura pendente segue olhando: parar no meio de uma troca engana a nota.
    if prof <= 0 && ls[0].capturas.is_empty() {
        return avaliar(t, cor);
    }
    for l in &ls {
        let v = -negamax(&aplicar(t, l), cor.outra(), prof - 1, -b, -a);
        if v > a {
            a = v;
            if a >= b {
                break;
            }
        }
    }
    a
}

/// O lance de `cor` com a maior nota mais um erro sorteado em ±`ruido`: é o erro que faz o bot
/// vacilar como gente nos níveis fáceis. Entre empatados, um sorteado (as partidas não se repetem).
fn melhor(t: &Tabuleiro, cor: Cor, prof: i32, ruido: i32) -> Option<Lance> {
    let mut semente = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
    let mut sortear = move || {
        semente ^= semente << 13;
        semente ^= semente >> 7;
        semente ^= semente << 17;
        semente
    };
    lances(t, cor)
        .into_iter()
        .map(|l| {
            let nota = -negamax(&aplicar(t, &l), cor.outra(), prof - 1, -i32::MAX, i32::MAX);
            let erro = (sortear() % (2 * ruido as u64 + 1)) as i32 - ruido;
            ((nota + erro, sortear()), l)
        })
        .max_by_key(|(chave, _)| *chave)
        .map(|(_, l)| l)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Nivel {
    Facil,
    Medio,
    Dificil,
}

impl Nivel {
    /// Quantos lances o bot olha à frente e o tamanho do erro dele (um peão vale 100).
    fn busca(self) -> (i32, i32) {
        match self {
            Nivel::Facil => (1, 160),
            Nivel::Medio => (2, 70),
            Nivel::Dificil => (6, 0),
        }
    }
    fn nome(self) -> &'static str {
        match self {
            Nivel::Facil => "FÁCIL",
            Nivel::Medio => "MÉDIO",
            Nivel::Dificil => "DIFÍCIL",
        }
    }
}

// O desenho em pixel art.

fn quad(r: &mut Renderer, x: f32, y: f32, w: f32, h: f32, cor: Color) {
    r.fill_quad(Rect { x, y, width: w, height: h }, Some(&cor.into()), Border::rounded(0.0), None);
}

/// Círculo em degraus de `px`: uma faixa por linha de pixels.
fn disco(r: &mut Renderer, cx: f32, cy: f32, raio: i32, px: f32, cor: Color) {
    for y in -raio..raio {
        let meio = y as f32 + 0.5;
        let w = ((raio * raio) as f32 - meio * meio).sqrt().round();
        quad(r, cx - w * px, cy + y as f32 * px, 2.0 * w * px, px, cor);
    }
}

const COROA: [&str; 4] = ["X..XX..X", "XX.XX.XX", "XXXXXXXX", ".XXXXXX."];

/// A peça vista de lado: sombra dura, contorno marinho, lateral escura e topo com anel ou coroa.
fn desenhar_peca(r: &mut Renderer, a: Rect, p: Peca, px: f32) {
    let (cx, cy) = ((a.x + a.width / 2.0).round(), (a.y + a.height / 2.0).round());
    let (topo, lado, brilho) = p.cor.tintas();
    disco(r, cx + px, cy + 2.0 * px, 7, px, Color::BLACK.alpha(0.35));
    disco(r, cx, cy + px, 7, px, MARINHO);
    disco(r, cx, cy - px, 7, px, MARINHO);
    disco(r, cx, cy + px, 6, px, lado);
    disco(r, cx, cy - px, 6, px, topo);
    if p.dama {
        for (i, linha) in COROA.iter().enumerate() {
            for (j, ch) in linha.chars().enumerate() {
                if ch == 'X' {
                    let (x, y) = (cx + (j as f32 - 4.0) * px, cy + (i as f32 - 3.0) * px);
                    quad(r, x, y + px, px, px, OURO_ESCURO);
                    quad(r, x, y, px, px, OURO);
                }
            }
        }
    } else {
        disco(r, cx, cy - px, 4, px, lado);
        disco(r, cx, cy - px, 3, px, topo);
    }
    quad(r, cx - 4.0 * px, cy - 6.0 * px, 2.0 * px, px, brilho);
    quad(r, cx - 5.0 * px, cy - 5.0 * px, px, 2.0 * px, brilho);
}

/// Quatro cantos em L: "esta peça tem de jogar".
fn cantos(r: &mut Renderer, a: Rect, cor: Color) {
    let (l, e) = (3.0 * PX, PX);
    for (x, y, sx, sy) in [(a.x, a.y, 1.0, 1.0), (a.x + a.width, a.y, -1.0, 1.0), (a.x, a.y + a.height, 1.0, -1.0), (a.x + a.width, a.y + a.height, -1.0, -1.0)] {
        let (x0, y0) = (if sx > 0.0 { x } else { x - l }, if sy > 0.0 { y } else { y - e });
        quad(r, x0, y0, l, e, cor);
        let (x1, y1) = (if sx > 0.0 { x } else { x - e }, if sy > 0.0 { y } else { y - l });
        quad(r, x1, y1, e, l, cor);
    }
}


#[derive(Debug, Clone)]
pub(crate) enum Msg {
    Casa(Casa),
    Bot(u32, Option<Lance>),
    Nivel(Nivel),
    Desfazer,
    Nova,
}

pub(crate) struct Ultimo {
    lance: Lance,
    capturadas: Vec<(Casa, Peca)>,
}

pub(crate) struct Damas {
    tab: Tabuleiro,
    vez: Cor,
    selecionada: Option<Casa>,
    ultimo: Option<Ultimo>,
    /// As posições antes de cada lance do jogador (o "desfazer" volta o par jogador + bot).
    historico: Vec<Tabuleiro>,
    nivel: Nivel,
    /// Muda a cada partida e a cada desfazer: a resposta atrasada do bot é jogada fora.
    partida: u32,
}

impl Damas {
    fn novo() -> Self {
        Damas { tab: inicial(), vez: Cor::Branca, selecionada: None, ultimo: None, historico: Vec::new(), nivel: Nivel::Medio, partida: 0 }
    }

    fn vencedor(&self) -> Option<Cor> {
        lances(&self.tab, self.vez).is_empty().then(|| self.vez.outra())
    }

    fn jogar(&mut self, l: Lance) {
        let capturadas = l.capturas.iter().filter_map(|&c| em(&self.tab, c).map(|p| (c, p))).collect();
        self.tab = aplicar(&self.tab, &l);
        self.ultimo = Some(Ultimo { lance: l, capturadas });
        self.vez = self.vez.outra();
        self.selecionada = None;
    }

    /// O clique do jogador: escolhe a peça, depois o destino.
    fn clicar(&mut self, c: Casa) -> Task<Msg> {
        if self.vez != Cor::Branca {
            return Task::none();
        }
        let ls = lances(&self.tab, Cor::Branca);
        if let Some(l) = self.selecionada.and_then(|s| ls.iter().find(|l| l.de() == s && l.para() == c)) {
            self.historico.push(self.tab);
            self.jogar(l.clone());
            return self.vez_do_bot();
        }
        self.selecionada = ls.iter().any(|l| l.de() == c).then_some(c);
        Task::none()
    }

    fn vez_do_bot(&self) -> Task<Msg> {
        if self.vez != Cor::Vermelha || self.vencedor().is_some() {
            return Task::none();
        }
        let (t, (prof, ruido), partida) = (self.tab, self.nivel.busca(), self.partida);
        Task::perform(
            async move {
                // Uma pausa para o lance do jogador terminar de deslizar.
                std::thread::sleep(Duration::from_millis(550));
                melhor(&t, Cor::Vermelha, prof, ruido)
            },
            move |l| Msg::Bot(partida, l),
        )
    }

    fn casa(&self, c: Casa, legais: &[Lance]) -> Element<'_, Msg> {
        let escura = (c.0 + c.1) % 2 == 0;
        let peca = em(&self.tab, c);
        let selecionada = self.selecionada == Some(c);
        let destino = self.selecionada.is_some_and(|s| legais.iter().any(|l| l.de() == s && l.para() == c));
        let obrigada = self.selecionada.is_none() && legais.first().is_some_and(|l| !l.capturas.is_empty()) && legais.iter().any(|l| l.de() == c);
        let rastro = self.ultimo.as_ref().is_some_and(|u| u.lance.caminho.contains(&c));
        let fundo = canvas(Length::FILL, Length::FILL, move |r, a, _| {
            if escura {
                quad(r, a.x, a.y, a.width, a.height, FELTRO);
                quad(r, a.x, a.y, a.width, PX, FELTRO_CLARO);
                quad(r, a.x, a.y + a.height - PX, a.width, PX, FELTRO_ESCURO);
            } else {
                quad(r, a.x, a.y, a.width, a.height, CREME);
            }
            if rastro {
                quad(r, a.x, a.y, a.width, a.height, OURO.alpha(0.22));
            }
            if selecionada {
                quad(r, a.x, a.y, a.width, PX, OURO);
                quad(r, a.x, a.y + a.height - PX, a.width, PX, OURO);
                quad(r, a.x, a.y, PX, a.height, OURO);
                quad(r, a.x + a.width - PX, a.y, PX, a.height, OURO);
            }
            if obrigada {
                cantos(r, a, OURO);
            }
            if destino {
                let (x, y) = ((a.x + a.width / 2.0 - 2.0 * PX).round(), (a.y + a.height / 2.0 - 2.0 * PX).round());
                quad(r, x + PX, y + PX, 4.0 * PX, 4.0 * PX, Color::BLACK.alpha(0.35));
                quad(r, x, y, 4.0 * PX, 4.0 * PX, OURO);
            }
        });
        let mut camadas: Vec<Element<'_, Msg>> = vec![fundo.into()];
        // A peça que acabou de capturada some encolhendo, depois que a outra passa.
        if let Some(&(_, p)) = self.ultimo.as_ref().and_then(|u| u.capturadas.iter().find(|(x, _)| *x == c)) {
            let fantasma = canvas(Length::FILL, Length::FILL, move |r, a, _| desenhar_peca(r, a, p, PX));
            camadas.push(animate(fantasma).opacity(0.0).scale(0.3).enter(Keyframe::default()).transition(Transition::new(240).delay(220).easing(Easing::EaseIn)).into());
        }
        if let Some(p) = peca {
            let desenho = canvas(Length::FILL, Length::FILL, move |r, a, _| desenhar_peca(r, a, p, PX));
            let mut anim = animate(desenho).offset(0.0, if selecionada { -2.0 * PX } else { 0.0 }).transition(Transition::new(320).easing(Easing::EaseOut));
            // A que acabou de jogar entra deslizando de onde saiu.
            if let Some(u) = self.ultimo.as_ref().filter(|u| u.lance.para() == c) {
                let de = u.lance.de();
                anim = anim.enter(Keyframe::default().offset((de.0 - c.0) as f32 * CASA, (c.1 - de.1) as f32 * CASA));
            }
            camadas.push(anim.into());
        }
        let clicavel = self.vez == Cor::Branca && (destino || legais.iter().any(|l| l.de() == c));
        button(stack(camadas)).padding(0.0).width(CASA).height(CASA).on_press_maybe(clicavel.then_some(Msg::Casa(c))).style(|_, _| Style::default()).into()
    }

    fn tabuleiro(&self) -> Element<'_, Msg> {
        let legais = if self.vez == Cor::Branca { lances(&self.tab, Cor::Branca) } else { Vec::new() };
        let rotulo = |s: String| texto(s, 14.0, MADEIRA_CLARA);
        let grade = column((0..8).rev().map(|l| row(std::iter::once(container(rotulo((l + 1).to_string())).width(24.0).height(CASA).align_y(Align::Center).into()).chain((0..8).map(|c| self.casa((c, l), &legais)))).into()));
        let letras = row(std::iter::once(Space::new(24.0, 20.0).into()).chain((0..8u8).map(|c| container(rotulo(((b'a' + c) as char).to_string())).width(CASA).height(20.0).align_x(Align::Center).align_y(Align::Center).into())));
        container(column![grade, letras])
            .padding(Padding { top: 24.0, right: 24.0, bottom: 4.0, left: 0.0 })
            .style(|_| Style::default().background(MADEIRA).border(Border::rounded(0.0).color(MADEIRA_ESCURA, PX)).shadow(sombra_dura(8.0)))
            .into()
    }

    fn jogador(&self, nome: &'static str, cor: Cor) -> Element<'_, Msg> {
        let n = self.tab.iter().flatten().flatten().filter(|p| p.cor == cor).count();
        let damas = self.tab.iter().flatten().flatten().filter(|p| p.cor == cor && p.dama).count();
        let detalhe = if damas > 0 { format!("{n} PEÇAS · {damas} DAMA{}", if damas > 1 { "S" } else { "" }) } else { format!("{n} PEÇAS") };
        let da_vez = self.vez == cor && self.vencedor().is_none();
        let marcador = animate(canvas(12.0, 12.0, |r, a, _| quad(r, a.x, a.y, a.width, a.height, OURO))).opacity(if da_vez { 0.15 } else { 0.0 }).enter(Keyframe::default().opacity(if da_vez { 1.0 } else { 0.0 })).repeat(Repeat::Alternate).transition(Transition::new(650));
        let icone = canvas(36.0, 36.0, move |r, a, _| desenhar_peca(r, a, Peca { cor, dama: false }, 2.0));
        container(row![icone, column![texto(nome, 18.0, Color::WHITE), texto(detalhe, 14.0, MENTA)].spacing(10), horizontal_space(), marcador].spacing(14).align(Align::Center))
            .padding(12.0)
            .width(Length::FILL)
            .style(move |_| Style::default().background(FUNDO).border(Border::rounded(0.0).color(if da_vez { OURO } else { MARINHO }, PX)))
            .into()
    }

    fn painel(&self) -> Element<'_, Msg> {
        let obrigatoria = self.vez == Cor::Branca && lances(&self.tab, Cor::Branca).first().is_some_and(|l| !l.capturas.is_empty());
        let estado: Element<'_, Msg> = match (self.vencedor(), self.vez) {
            (Some(_), _) => texto("FIM DE JOGO", 18.0, OURO).into(),
            (None, Cor::Vermelha) => row![texto("PENSANDO", 18.0, OURO), animate(texto("...", 18.0, OURO)).opacity(0.1).repeat(Repeat::Alternate).transition(Transition::new(420))].into(),
            (None, Cor::Branca) if obrigatoria => animate(texto("CAPTURA OBRIGATÓRIA", 16.0, ROSA)).opacity(0.45).repeat(Repeat::Alternate).transition(Transition::new(700)).into(),
            (None, Cor::Branca) => texto("SUA VEZ", 18.0, OURO).into(),
        };
        let niveis = row([Nivel::Facil, Nivel::Medio, Nivel::Dificil].map(|n| botao(n.nome(), 14.0, Some(Msg::Nivel(n)), n == self.nivel))).spacing(10);
        let pode_desfazer = !self.historico.is_empty() && (self.vez == Cor::Branca || self.vencedor().is_some());
        let regras = column(["CAPTURA É OBRIGATÓRIA", "VALE A QUE CAPTURA MAIS", "PEÃO CAPTURA PARA TRÁS", "DAMA ANDA LONGE"].map(|r| texto(r, 14.0, Color::WHITE.alpha(0.45)).into())).spacing(6);
        container(
            column![
                self.jogador("BOT", Cor::Vermelha),
                self.jogador("VOCÊ", Cor::Branca),
                container(estado).height(28.0).align_y(Align::Center),
                texto("NÍVEL DO BOT", 14.0, MENTA),
                niveis,
                vertical_space(),
                regras,
                vertical_space(),
                botao("DESFAZER", 18.0, pode_desfazer.then_some(Msg::Desfazer), false),
                botao("NOVA PARTIDA", 18.0, Some(Msg::Nova), true),
            ]
            .spacing(14),
        )
        .padding(24.0)
        .width(340.0)
        .height(632.0)
        .style(|_| Style::default().background(PAINEL).border(Border::rounded(0.0).color(Color::BLACK, PX)).shadow(sombra_dura(8.0)))
        .into()
    }

    /// O letreiro do fim, por cima de tudo, depois que o último lance termina de deslizar.
    fn fim(&self, vencedor: Cor) -> Element<'_, Msg> {
        let (titulo, sub) = if vencedor == Cor::Branca { ("VOCÊ VENCEU!", "O BOT FICOU SEM LANCES") } else { ("O BOT VENCEU", "QUE TAL A REVANCHE?") };
        let cartao = container(column![texto(titulo, 40.0, OURO), texto(sub, 16.0, MENTA), Space::new(0.0, 8.0), container(botao("JOGAR DE NOVO", 18.0, Some(Msg::Nova), true)).width(260.0)].spacing(18).align(Align::Center)).padding(36.0).style(|_| placa().shadow(sombra_dura(8.0)));
        let entra = Transition::new(320).delay(500).easing(Easing::EaseOut);
        let cortina = container(animate(cartao).enter(Keyframe::default().scale(0.6).opacity(0.0)).transition(entra)).center().style(|_| Style::default().background(Color::BLACK.alpha(0.55)));
        animate(cortina).enter(Keyframe::default().opacity(0.0)).transition(entra).into()
    }
}

impl App for Damas {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Casa(c) => return self.clicar(c),
            Msg::Bot(p, l) => {
                if let (true, Cor::Vermelha, Some(l)) = (p == self.partida, self.vez, l) {
                    self.jogar(l);
                }
            }
            Msg::Nivel(n) => self.nivel = n,
            Msg::Desfazer => {
                if let Some(t) = self.historico.pop() {
                    *self = Damas { tab: t, historico: std::mem::take(&mut self.historico), nivel: self.nivel, partida: self.partida + 1, ..Damas::novo() };
                }
            }
            Msg::Nova => *self = Damas { nivel: self.nivel, partida: self.partida + 1, ..Damas::novo() },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let titulo = container(text("DAMAS").font(LETREIRO).size(24.0).color(OURO)).padding([14u16, 24]).style(|_| placa());
        let mesa = column![titulo, row![self.tabuleiro(), self.painel()].spacing(36).align(Align::Start)].spacing(28).align(Align::Center);
        let mut camadas = vec![container(mesa).center().style(|_| Style::default().background(FUNDO)).into()];
        if let Some(v) = self.vencedor() {
            camadas.push(self.fim(v));
        }
        stack(camadas).into()
    }
}

/// O ícone do menu: uma pedra branca e uma dama vermelha.
pub(crate) fn icone<'a, M: 'a>() -> Element<'a, M> {
    canvas(160.0, 110.0, |r, a, _| {
        let lugar = |x: f32, y: f32| Rect { x: a.x + x, y: a.y + y, width: 64.0, height: 64.0 };
        desenhar_peca(r, lugar(30.0, 34.0), Peca { cor: Cor::Branca, dama: false }, 4.0);
        desenhar_peca(r, lugar(70.0, 10.0), Peca { cor: Cor::Vermelha, dama: true }, 4.0);
    })
    .into()
}

/// O jogo pronto para abrir (no exemplo sozinho ou no menu do pack).
pub(crate) fn iniciar() -> (Damas, Task<Msg>) {
    (Damas::novo(), Task::none())
}

pub(crate) fn principal() -> Result<(), Error> {
    carregar_fontes();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        // Meio de partida: uma dama de cada lado, captura obrigatória à vista.
        let mut d = Damas::novo();
        d.tab = [[None; 8]; 8];
        for (c, l, cor, dama) in [(0, 0, Cor::Branca, false), (2, 0, Cor::Branca, false), (1, 1, Cor::Branca, false), (5, 1, Cor::Branca, false), (2, 2, Cor::Branca, false), (6, 2, Cor::Branca, false), (3, 3, Cor::Branca, false), (7, 7, Cor::Branca, true), (4, 4, Cor::Vermelha, false), (1, 5, Cor::Vermelha, false), (5, 5, Cor::Vermelha, false), (2, 6, Cor::Vermelha, false), (6, 6, Cor::Vermelha, false), (1, 7, Cor::Vermelha, false), (3, 7, Cor::Vermelha, false), (0, 2, Cor::Vermelha, true)] {
            d.tab[l][c] = Some(Peca { cor, dama });
        }
        d.ultimo = Some(Ultimo { lance: Lance { caminho: vec![(3, 5), (4, 4)], capturas: Vec::new() }, capturadas: Vec::new() });
        d.historico.push(inicial());
        d.selecionada = Some((3, 3));
        let png = capture(&d.view(), Size::new(1100.0, 800.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("damas.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Damas".into(), size: Size::new(1100.0, 800.0), min_size: Some(Size::new(1060.0, 780.0)), ..Default::default() }, iniciar)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn vazio(pecas: &[(i8, i8, Cor, bool)]) -> Tabuleiro {
        let mut t = [[None; 8]; 8];
        for &(c, l, cor, dama) in pecas {
            t[l as usize][c as usize] = Some(Peca { cor, dama });
        }
        t
    }

    #[test]
    fn abertura_tem_sete_lances() {
        assert_eq!(lances(&inicial(), Cor::Branca).len(), 7);
        assert_eq!(lances(&inicial(), Cor::Vermelha).len(), 7);
    }

    #[test]
    fn captura_e_obrigatoria_e_peao_captura_para_tras() {
        let t = vazio(&[(2, 2, Cor::Branca, false), (6, 0, Cor::Branca, false), (3, 1, Cor::Vermelha, false)]);
        let ls = lances(&t, Cor::Branca);
        assert_eq!(ls, vec![Lance { caminho: vec![(2, 2), (4, 0)], capturas: vec![(3, 1)] }]);
    }

    #[test]
    fn vale_a_que_captura_mais_e_so_some_no_fim() {
        // Uma dá dois saltos, a outra um: só a dupla vale.
        let t = vazio(&[(0, 0, Cor::Branca, false), (1, 1, Cor::Vermelha, false), (3, 3, Cor::Vermelha, false), (6, 2, Cor::Branca, false), (7, 3, Cor::Vermelha, false), (5, 3, Cor::Vermelha, false)]);
        let ls = lances(&t, Cor::Branca);
        assert!(ls.iter().all(|l| l.capturas.len() == 2), "{ls:?}");
        let depois = aplicar(&t, &ls[0]);
        assert!(ls[0].capturas.iter().all(|&c| em(&depois, c).is_none()));
    }

    #[test]
    fn dama_voa_e_captura_de_longe() {
        let t = vazio(&[(0, 0, Cor::Branca, true), (4, 4, Cor::Vermelha, false)]);
        let mut fins: Vec<Casa> = lances(&t, Cor::Branca).iter().map(Lance::para).collect();
        fins.sort();
        assert_eq!(fins, vec![(5, 5), (6, 6), (7, 7)]);
    }

    #[test]
    fn promove_so_terminando_na_ultima_linha() {
        let t = vazio(&[(1, 6, Cor::Branca, false)]);
        let l = lances(&t, Cor::Branca).remove(0);
        assert_eq!(em(&aplicar(&t, &l), l.para()), Some(Peca { cor: Cor::Branca, dama: true }));
    }

    #[test]
    fn bot_pega_a_peca_solta_e_ve_a_derrota() {
        let t = vazio(&[(3, 3, Cor::Branca, false), (4, 4, Cor::Vermelha, false), (0, 6, Cor::Vermelha, false)]);
        let l = melhor(&t, Cor::Vermelha, 3, 0).expect("tem lance");
        assert_eq!(l.capturas, vec![(3, 3)]);
        assert_eq!(melhor(&vazio(&[(0, 0, Cor::Branca, false)]), Cor::Vermelha, 3, 0), None);
    }

    #[test]
    fn jogada_completa_com_resposta() {
        let mut d = Damas::novo();
        d.clicar((2, 2));
        assert_eq!(d.selecionada, Some((2, 2)));
        d.clicar((3, 3));
        assert_eq!(d.vez, Cor::Vermelha);
        let l = melhor(&d.tab, Cor::Vermelha, 2, 70).expect("bot responde");
        d.update(Msg::Bot(d.partida, Some(l)));
        assert_eq!(d.vez, Cor::Branca);
        d.update(Msg::Desfazer);
        assert_eq!(d.tab, inicial());
    }
}
