//! Paciência (Klondike) com o baralho pixelado do pack: arraste as cartas, ou dê clique duplo e a
//! carta vai sozinha para o melhor lugar. Compra de uma em uma, voltas sem limite no monte. Só
//! entram na mesa jogos que o resolvedor consegue terminar.
//! `cargo run --release --example paciencia`; `-- --capturar x.png` só gera o PNG.
//! shortcut: sem pontuação nem "compra de três"; entram se o pack pedir.
use pelicano::{column, row};
use pelicano::*;
use std::time::{Duration, Instant};

use crate::estilo::*;
use crate::cartas::*;

const CW: f32 = 120.0;
const CH: f32 = 168.0;
const VAO: f32 = 36.0;
/// Onde começam as colunas, abaixo do monte e das fundações.
const TOPO: f32 = CH + 44.0;
const LARGURA: f32 = 7.0 * CW + 6.0 * VAO;
const ALTURA: f32 = 620.0;
/// Quanto aparece de cada carta numa coluna (as abertas apertam quando a coluna cresce).
const FECHADA: f32 = 14.0;
const ABERTA: f32 = 36.0;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Carta {
    naipe: u8,
    valor: u8,
    aberta: bool,
}

impl Carta {
    fn vermelha(self) -> bool {
        self.naipe % 2 == 0
    }
    /// A mesma carta, aberta ou não: a identidade dela no baralho (0..52).
    fn id(self) -> usize {
        self.naipe as usize * 13 + self.valor as usize - 1
    }
    fn svg(self) -> Svg {
        if self.aberta { svg_carta(self.naipe, self.valor) } else { svg_verso() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pilha {
    Monte,
    Descarte,
    Fundacao(usize),
    Coluna(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dica {
    Mover(Pilha, usize, Pilha),
    Comprar,
}

fn xorshift(mut s: u64) -> u64 {
    s ^= s << 13;
    s ^= s >> 7;
    s ^ (s << 17)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mesa {
    monte: Vec<Carta>,
    descarte: Vec<Carta>,
    /// Uma por naipe, na ordem de `FACES`.
    fundacoes: [Vec<Carta>; 4],
    colunas: [Vec<Carta>; 7],
    jogadas: u32,
}

impl Mesa {
    fn embaralhada(semente: u64) -> Mesa {
        let mut cartas: Vec<Carta> = (0..4).flat_map(|naipe| (1..=13).map(move |valor| Carta { naipe, valor, aberta: false })).collect();
        let mut s = semente | 1;
        for i in (1..cartas.len()).rev() {
            s = xorshift(s);
            cartas.swap(i, (s % (i as u64 + 1)) as usize);
        }
        let mut colunas: [Vec<Carta>; 7] = Default::default();
        for (i, coluna) in colunas.iter_mut().enumerate() {
            for j in 0..=i {
                let c = cartas.pop().expect("52 cartas bastam");
                coluna.push(Carta { aberta: j == i, ..c });
            }
        }
        Mesa { monte: cartas, descarte: Vec::new(), fundacoes: Default::default(), colunas, jogadas: 0 }
    }

    fn pilha(&self, p: Pilha) -> &Vec<Carta> {
        match p {
            Pilha::Monte => &self.monte,
            Pilha::Descarte => &self.descarte,
            Pilha::Fundacao(f) => &self.fundacoes[f],
            Pilha::Coluna(i) => &self.colunas[i],
        }
    }

    fn pilha_mut(&mut self, p: Pilha) -> &mut Vec<Carta> {
        match p {
            Pilha::Monte => &mut self.monte,
            Pilha::Descarte => &mut self.descarte,
            Pilha::Fundacao(f) => &mut self.fundacoes[f],
            Pilha::Coluna(i) => &mut self.colunas[i],
        }
    }

    fn pode(&self, cartas: &[Carta], destino: Pilha) -> bool {
        let Some(&c) = cartas.first() else { return false };
        match destino {
            Pilha::Fundacao(f) => cartas.len() == 1 && c.naipe as usize == f && c.valor as usize == self.fundacoes[f].len() + 1,
            Pilha::Coluna(i) => match self.colunas[i].last() {
                None => c.valor == 13,
                Some(t) => t.aberta && t.vermelha() != c.vermelha() && t.valor == c.valor + 1,
            },
            _ => false,
        }
    }

    /// Quais cartas a partir de `idx` podem ser pegas juntas: só abertas, e no descarte e na
    /// fundação só a de cima.
    fn pegavel(&self, p: Pilha, idx: usize) -> bool {
        let pilha = self.pilha(p);
        match p {
            Pilha::Monte => false,
            Pilha::Descarte | Pilha::Fundacao(_) => idx + 1 == pilha.len(),
            Pilha::Coluna(_) => pilha.get(idx).is_some_and(|c| c.aberta),
        }
    }

    /// Leva `pilha[idx..]` para `destino` se a regra deixa, e abre a carta que ficou por cima.
    fn mover(&mut self, origem: Pilha, idx: usize, destino: Pilha) -> bool {
        if origem == destino || !self.pegavel(origem, idx) || !self.pode(&self.pilha(origem)[idx..], destino) {
            return false;
        }
        let cartas = self.pilha_mut(origem).split_off(idx);
        self.pilha_mut(destino).extend(cartas);
        if let (Pilha::Coluna(_), Some(c)) = (origem, self.pilha_mut(origem).last_mut()) {
            c.aberta = true;
        }
        self.jogadas += 1;
        true
    }

    /// Para onde a carta clicada vai sozinha: fundação, depois uma coluna, a vazia por último.
    fn destino_auto(&self, origem: Pilha, idx: usize) -> Option<Pilha> {
        let cartas = &self.pilha(origem)[idx..];
        let cheias = (0..7).filter(|&i| !self.colunas[i].is_empty()).map(Pilha::Coluna);
        let vazias = (0..7).filter(|&i| self.colunas[i].is_empty()).map(Pilha::Coluna);
        (0..4).map(Pilha::Fundacao).chain(cheias).chain(vazias).filter(|&d| d != origem).find(|&d| self.pode(cartas, d))
    }

    /// Vira uma carta do monte; monte vazio recolhe o descarte.
    fn comprar(&mut self) -> bool {
        if let Some(c) = self.monte.pop() {
            self.descarte.push(Carta { aberta: true, ..c });
        } else if !self.descarte.is_empty() {
            self.monte = self.descarte.drain(..).rev().map(|c| Carta { aberta: false, ..c }).collect();
        } else {
            return false;
        }
        self.jogadas += 1;
        true
    }

    fn venceu(&self) -> bool {
        self.fundacoes.iter().all(|f| f.len() == 13)
    }

    /// Tudo aberto e nada no monte: o resto é só subir as cartas.
    fn pode_completar(&self) -> bool {
        !self.venceu() && self.monte.is_empty() && self.descarte.is_empty() && self.colunas.iter().flatten().all(|c| c.aberta)
    }

    /// O próximo lance do "completar": a menor carta que já sobe.
    fn proximo_auto(&self) -> Option<(Pilha, usize, Pilha)> {
        (0..7)
            .filter_map(|i| {
                let c = *self.colunas[i].last()?;
                let f = Pilha::Fundacao(c.naipe as usize);
                self.pode(&[c], f).then_some((c.valor, (Pilha::Coluna(i), self.colunas[i].len() - 1, f)))
            })
            .min_by_key(|(v, _)| *v)
            .map(|(_, m)| m)
    }

    /// Uma jogada que ajuda, da melhor para a pior; `None` é jogo perdido. De coluna para coluna
    /// só conta se abre uma carta fechada, esvazia a coluna para um rei ou libera uma que sobe.
    /// shortcut: não considera descer carta da fundação; se fizer falta, um "perdeu" sai cedo.
    fn dica(&self) -> Option<Dica> {
        if self.venceu() {
            return None;
        }
        let fundacao = |c: Carta| Pilha::Fundacao(c.naipe as usize);
        let colunas = || (0..7).map(Pilha::Coluna);
        for o in [Pilha::Descarte].into_iter().chain(colunas()) {
            if let Some(&c) = self.pilha(o).last().filter(|c| self.pode(&[**c], fundacao(**c))) {
                return Some(Dica::Mover(o, self.pilha(o).len() - 1, fundacao(c)));
            }
        }
        let rei_esperando = self.monte.iter().chain(&self.descarte).any(|c| c.valor == 13) || self.colunas.iter().any(|col| col.iter().position(|c| c.aberta).is_some_and(|i| i > 0 && col[i].valor == 13));
        for (i, col) in self.colunas.iter().enumerate() {
            let Some(primeira) = col.iter().position(|c| c.aberta) else { continue };
            for idx in primeira..col.len() {
                let util = if idx == primeira { primeira > 0 || rei_esperando } else { self.pode(&[col[idx - 1]], fundacao(col[idx - 1])) };
                // Rei sozinho de uma coluna para outra vazia não muda nada.
                let destino = colunas().filter(|&d| d != Pilha::Coluna(i)).find(|&d| self.pode(&col[idx..], d) && !(idx == 0 && self.pilha(d).is_empty()));
                if let Some(d) = destino.filter(|_| util) {
                    return Some(Dica::Mover(Pilha::Coluna(i), idx, d));
                }
            }
        }
        if let Some(&c) = self.descarte.last() {
            if let Some(d) = colunas().find(|&d| self.pode(&[c], d)) {
                return Some(Dica::Mover(Pilha::Descarte, self.descarte.len() - 1, d));
            }
        }
        // Uma carta mais funda no monte ou no descarte que serve: comprar até ela.
        let serve = |c: Carta| self.pode(&[c], fundacao(c)) || colunas().any(|d| self.pode(&[c], d));
        self.monte.iter().chain(&self.descarte).any(|&c| serve(c)).then_some(Dica::Comprar)
    }

    /// A carta de id `id`, como está na mesa (aberta ou não).
    fn carta(&self, id: usize) -> Carta {
        let c = Carta { naipe: (id / 13) as u8, valor: (id % 13) as u8 + 1, aberta: true };
        self.onde(c).map_or(c, |(p, i)| self.pilha(p)[i])
    }

    fn onde(&self, c: Carta) -> Option<(Pilha, usize)> {
        let pilhas = [Pilha::Monte, Pilha::Descarte].into_iter().chain((0..4).map(Pilha::Fundacao)).chain((0..7).map(Pilha::Coluna));
        pilhas.into_iter().find_map(|p| self.pilha(p).iter().position(|x| x.id() == c.id()).map(|i| (p, i)))
    }

    /// O canto de cima da carta `idx` da pilha, na mesa.
    fn posicao(&self, p: Pilha, idx: usize) -> Point {
        let x = |col: usize| col as f32 * (CW + VAO);
        match p {
            // O monte tem espessura: as de baixo descem um pouco.
            Pilha::Monte => {
                let d = ((self.monte.len().saturating_sub(idx + 1)) / 6).min(3) as f32 * 2.0;
                Point::new(d, d)
            }
            Pilha::Descarte => Point::new(x(1), 0.0),
            Pilha::Fundacao(f) => Point::new(x(3 + f), 0.0),
            Pilha::Coluna(i) => {
                let col = &self.colunas[i];
                let fechadas = col.iter().filter(|c| !c.aberta).count() as f32;
                let abertas = col.len() as f32 - fechadas;
                let livre = ALTURA - TOPO - CH - fechadas * FECHADA;
                let passo = if abertas > 1.0 { (livre / (abertas - 1.0)).min(ABERTA) } else { ABERTA };
                let y = col[..idx.min(col.len())].iter().map(|c| if c.aberta { passo } else { FECHADA }).sum::<f32>();
                Point::new(x(i), TOPO + y)
            }
        }
    }
}

// O resolvedor: só entram na mesa jogos que ele consegue terminar.

/// Quantas posições o resolvedor olha antes de desistir do embaralhamento.
const LIMITE: usize = 10_000;

/// A posição sem o que não muda o jogo: colunas em qualquer ordem e o monte como um anel (com
/// voltas sem limite, onde a próxima compra está não importa).
fn chave(m: &Mesa) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut colunas: Vec<Vec<u8>> = m.colunas.iter().map(|c| c.iter().map(|x| x.id() as u8 | if x.aberta { 64 } else { 0 }).collect()).collect();
    colunas.sort();
    let mut anel: Vec<u8> = m.monte.iter().rev().chain(&m.descarte).map(|x| x.id() as u8).collect();
    if let Some(i) = anel.iter().enumerate().min_by_key(|(_, x)| **x).map(|(i, _)| i) {
        anel.rotate_left(i);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (m.fundacoes.iter().map(Vec::len).collect::<Vec<_>>(), colunas, anel).hash(&mut h);
    h.finish()
}

/// Sobe o que não faz falta na mesa: até 2, ou quando as fundações da outra cor já passaram.
fn subir_seguras(m: &mut Mesa) {
    loop {
        let segura = |c: Carta| c.valor <= 2 || (0..4).filter(|&f| (f % 2 == 0) != c.vermelha()).all(|f| m.fundacoes[f].len() + 1 >= c.valor as usize);
        let lance = [Pilha::Descarte].into_iter().chain((0..7).map(Pilha::Coluna)).find_map(|o| {
            let &c = m.pilha(o).last()?;
            (segura(c) && m.pode(&[c], Pilha::Fundacao(c.naipe as usize))).then_some((o, m.pilha(o).len() - 1, Pilha::Fundacao(c.naipe as usize)))
        });
        match lance {
            Some((o, i, d)) => {
                m.mover(o, i, d);
            }
            None => return,
        }
    }
}

/// As posições seguintes, a mais promissora primeiro.
fn sucessores(m: &Mesa) -> Vec<Mesa> {
    let mut v = Vec::new();
    let tentar = |o: Pilha, i: usize, d: Pilha, v: &mut Vec<Mesa>| {
        let mut n = m.clone();
        if n.mover(o, i, d) {
            v.push(n);
        }
    };
    for i in 0..7 {
        if let Some(&c) = m.colunas[i].last() {
            tentar(Pilha::Coluna(i), m.colunas[i].len() - 1, Pilha::Fundacao(c.naipe as usize), &mut v);
        }
    }
    let rei_esperando = m.monte.iter().chain(&m.descarte).any(|c| c.valor == 13) || m.colunas.iter().any(|c| c.iter().position(|x| x.aberta).is_some_and(|i| i > 0 && c[i].valor == 13));
    for i in 0..7 {
        let col = &m.colunas[i];
        let Some(primeira) = col.iter().position(|c| c.aberta) else { continue };
        for idx in primeira..col.len() {
            let util = if idx == primeira { primeira > 0 || rei_esperando } else { m.pode(&[col[idx - 1]], Pilha::Fundacao(col[idx - 1].naipe as usize)) };
            if !util {
                continue;
            }
            for j in (0..7).filter(|&j| j != i && !(idx == 0 && m.colunas[j].is_empty())) {
                tentar(Pilha::Coluna(i), idx, Pilha::Coluna(j), &mut v);
            }
        }
    }
    // Do monte: qualquer carta chega ao topo do descarte comprando; uma volta basta.
    let anel = m.monte.len() + m.descarte.len();
    for k in m.monte.iter().chain(&m.descarte).map(|c| c.id()).collect::<Vec<_>>() {
        let mut n = m.clone();
        for _ in 0..=anel {
            if n.descarte.last().is_some_and(|c| c.id() == k) {
                break;
            }
            n.comprar();
        }
        let &c = n.descarte.last().expect("a carta está no anel");
        for d in std::iter::once(Pilha::Fundacao(c.naipe as usize)).chain((0..7).map(Pilha::Coluna)) {
            let mut x = n.clone();
            if x.mover(Pilha::Descarte, x.descarte.len() - 1, d) {
                v.push(x);
            }
        }
    }
    v
}

/// Procura, em profundidade, um jeito de terminar; desiste depois de `limite` posições.
fn tem_solucao(m: &Mesa, limite: usize) -> bool {
    let mut vistos = std::collections::HashSet::new();
    let mut pilha = vec![m.clone()];
    while let Some(mut m) = pilha.pop() {
        subir_seguras(&mut m);
        if m.venceu() {
            return true;
        }
        if !vistos.insert(chave(&m)) {
            continue;
        }
        if vistos.len() > limite {
            return false;
        }
        pilha.extend(sucessores(&m).into_iter().rev());
    }
    false
}

/// Embaralha até sair um jogo que o resolvedor termina.
fn distribuir(mut semente: u64) -> Mesa {
    loop {
        let m = Mesa::embaralhada(semente);
        if tem_solucao(&m, LIMITE) {
            return m;
        }
        semente = xorshift(semente);
    }
}

/// A pilha embaixo do centro da carta solta.
fn alvo_em(centro: Point) -> Option<Pilha> {
    let col = ((centro.x - CW / 2.0) / (CW + VAO)).round().clamp(0.0, 6.0) as usize;
    if centro.y >= TOPO - 20.0 {
        Some(Pilha::Coluna(col))
    } else {
        (col >= 3).then(|| Pilha::Fundacao(col - 3))
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Msg {
    Comprar,
    Pegar(Pilha, usize),
    /// Clique duplo: a carta vai sozinha para o melhor lugar.
    Enviar(Pilha, usize),
    Mouse(Point),
    Soltar,
    Pousou(u32),
    Desfazer,
    Novo,
    Completar,
    Auto(u32),
    Tique(u32),
    /// A mesa com solução que o resolvedor achou em segundo plano.
    Distribuir(u32, Mesa),
    /// Um quadro da festa da vitória (as cartas pulando).
    Quadro(u32),
    /// Um quadro dos giros de carta.
    Girar(u32),
    Dica,
}

pub(crate) struct Arraste {
    origem: Pilha,
    idx: usize,
    inicio: Point,
    mouse: Point,
}

pub(crate) struct Paciencia {
    mesa: Mesa,
    historico: Vec<Mesa>,
    arraste: Option<Arraste>,
    /// Cartas a caminho do lugar novo: de onde saíram, de qual voo são e quanto esperam para sair.
    voo: Vec<(Carta, Point, u32, u64)>,
    voos: u32,
    mouse: Point,
    segundos: u32,
    /// Muda a cada jogo: o relógio e o "completar" do jogo anterior param.
    partida: u32,
    dica: Option<Dica>,
    /// O resolvedor ainda procura a mesa deste jogo.
    embaralhando: bool,
    /// Cartas girando do verso para a face: id, quando começou e quanto espera (ms).
    girando: Vec<(usize, Instant, u64)>,
    /// Já há um relógio de quadros redesenhando os giros.
    girando_tique: bool,
    /// A festa da vitória: as cartas saindo das fundações e quicando pela tela.
    cascata: Option<Cascata>,
}

/// Uma carta pulando: posição e velocidade em pixels por quadro, na mesa.
pub(crate) struct Pulo {
    carta: Carta,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

pub(crate) struct Cascata {
    pulos: Vec<Pulo>,
    quadros: u32,
    lancadas: usize,
    sorteio: u64,
    fim: bool,
}

/// O chão onde as cartas quicam (um pouco abaixo do feltro) e o quanto elas saem da tela.
const CHAO: f32 = ALTURA - CH + 30.0;
const MARGEM: f32 = 260.0;

impl Cascata {
    /// Um passo da física; a cada 7 quadros sai mais uma carta, da fundação seguinte.
    fn passo(&mut self, m: &mut Mesa) {
        self.quadros += 1;
        if self.quadros % 7 == 1 {
            if let Some(f) = (0..4).map(|k| (self.lancadas + k) % 4).find(|&f| !m.fundacoes[f].is_empty()) {
                let pos = m.posicao(Pilha::Fundacao(f), 0);
                let carta = m.fundacoes[f].pop().expect("fundação não vazia");
                self.sorteio = xorshift(self.sorteio);
                let lado = if self.sorteio % 3 == 0 { 1.0 } else { -1.0 };
                let (vx, vy) = (lado * (3.0 + (self.sorteio >> 8) as f32 % 5.0), -((self.sorteio >> 16) as f32 % 9.0));
                self.pulos.push(Pulo { carta, x: pos.x, y: pos.y, vx, vy });
                self.lancadas += 1;
            }
        }
        for p in &mut self.pulos {
            p.vy += 0.6;
            p.x += p.vx;
            p.y += p.vy;
            if p.y > CHAO {
                p.y = CHAO;
                p.vy = -p.vy * 0.78;
            }
        }
        self.pulos.retain(|p| p.x > -CW - MARGEM && p.x < LARGURA + MARGEM);
        self.fim = self.pulos.is_empty() && m.fundacoes.iter().all(Vec::is_empty);
    }
}

/// Quanto dura o giro de uma carta (metade mostrando o verso, metade a face).
const GIRO: u64 = 280;

/// A carta girando no eixo vertical: o verso estreita até virar uma linha e a face abre do
/// outro lado. Desenhada quadro a quadro pela hora atual.
fn carta_girando<'a>(c: Carta, inicio: Instant, atraso: u64) -> Element<'a, Msg> {
    canvas(CW, CH, move |r, a, _| {
        let t = ((inicio.elapsed().as_millis() as f32 - atraso as f32) / GIRO as f32).clamp(0.0, 1.0);
        let (face, k) = if t < 0.5 { (svg_verso(), 1.0 - 2.0 * t) } else { (svg_carta(c.naipe, c.valor), 2.0 * t - 1.0) };
        let w = (CW * k).max(2.0);
        // No meio do giro a carta sobe um pouco: a sombra se afasta.
        let alto = (1.0 - k) * PX;
        let lugar = Rect { x: a.x + (CW - w) / 2.0, y: a.y - alto, width: w, height: CH };
        r.fill_quad(lugar, Some(&Color::WHITE.into()), Border::rounded(7.0_f32.min(w / 2.0)), Some(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(PX + alto, PX + alto), blur: 0.0 }));
        r.draw_svg(&face, lugar, None);
    })
    .into()
}

fn quadro_do_giro(partida: u32) -> Task<Msg> {
    Task::perform(async { std::thread::sleep(Duration::from_millis(16)) }, move |_| Msg::Girar(partida))
}

fn quadro(partida: u32) -> Task<Msg> {
    Task::perform(async { std::thread::sleep(Duration::from_millis(16)) }, move |_| Msg::Quadro(partida))
}

fn semente() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64)
}

fn embaralhar(partida: u32) -> Task<Msg> {
    Task::perform(async { distribuir(semente()) }, move |m| Msg::Distribuir(partida, m))
}

fn relogio(partida: u32) -> Task<Msg> {
    Task::perform(async { std::thread::sleep(Duration::from_secs(1)) }, move |_| Msg::Tique(partida))
}

/// A carta com a sombra dura das mesas do baralho.
fn carta<'a>(c: Carta, sombra: f32) -> Element<'a, Msg> {
    container(svg(c.svg()).width(CW).height(CH))
        .style(move |_| Style::default().background(Color::WHITE).border(Border::rounded(7.0)).shadow(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(sombra, sombra), blur: 0.0 }))
        .into()
}

fn no_lugar<'a>(el: impl Into<Element<'a, Msg>>, p: Point) -> Element<'a, Msg> {
    container(el).padding(Padding { top: p.y, right: 0.0, bottom: 0.0, left: p.x }).into()
}

/// O lugar vazio de uma pilha, marcado no feltro.
fn vaga<'a>(dentro: Option<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(dentro.unwrap_or_else(|| Space::new(0.0, 0.0).into()))
        .center()
        .width(CW)
        .height(CH)
        .style(|_| Style::default().background(FELTRO_ESCURO).border(Border::rounded(7.0).color(MENTA.alpha(0.3), PX / 2.0)))
        .into()
}

impl Paciencia {
    /// O jogo esperando o resolvedor: o baralho todo fechado no monte.
    fn embaralhando(partida: u32) -> Self {
        let mut m = Mesa::embaralhada(semente());
        m.monte = m.colunas.iter_mut().flat_map(std::mem::take).chain(m.monte).map(|c| Carta { aberta: false, ..c }).collect();
        Paciencia { mesa: m, embaralhando: true, ..Paciencia::novo(0, partida) }
    }

    fn novo(semente: u64, partida: u32) -> Self {
        Paciencia { mesa: Mesa::embaralhada(semente), historico: Vec::new(), arraste: None, voo: Vec::new(), voos: 0, mouse: Point::ORIGIN, segundos: 0, partida, dica: None, embaralhando: false, girando: Vec::new(), girando_tique: false, cascata: None }
    }

    /// Joga e faz as cartas voarem de `origens` (onde estavam na tela) até o lugar novo; sem
    /// `voar`, encaixam na hora (soltas com o mouse já estão lá). Lance proibido sempre volta voando.
    fn fazer(&mut self, origem: Pilha, idx: usize, destino: Pilha, origens: Vec<Point>, voar: bool) -> Task<Msg> {
        let antes = self.mesa.clone();
        let cartas = self.mesa.pilha(origem)[idx..].to_vec();
        if !self.mesa.mover(origem, idx, destino) {
            return self.voar(cartas, origens);
        }
        // A carta de baixo que estava fechada gira para a face, junto com o voo.
        let virou = matches!(origem, Pilha::Coluna(_)) && idx > 0 && !antes.pilha(origem)[idx - 1].aberta;
        self.historico.push(antes);
        let tarefa = if voar { self.voar(cartas, origens) } else { Task::none() };
        match self.mesa.pilha(origem).last().filter(|_| virou) {
            Some(&c) => Task::batch([tarefa, self.girar(c.id(), 0)]),
            None => tarefa,
        }
    }

    /// Põe a carta `id` a girar daqui a `atraso` ms, com o relógio de quadros andando.
    fn girar(&mut self, id: usize, atraso: u64) -> Task<Msg> {
        self.girando.retain(|(x, _, _)| *x != id);
        self.girando.push((id, Instant::now(), atraso));
        if std::mem::replace(&mut self.girando_tique, true) {
            Task::none()
        } else {
            quadro_do_giro(self.partida)
        }
    }

    fn voar(&mut self, cartas: Vec<Carta>, origens: Vec<Point>) -> Task<Msg> {
        self.voar_em_fila(cartas, origens, 0)
    }

    /// Como `voar`, mas cada carta sai `passo` ms depois da anterior (a distribuição).
    fn voar_em_fila(&mut self, cartas: Vec<Carta>, origens: Vec<Point>, passo: u64) -> Task<Msg> {
        self.voos += 1;
        let id = self.voos;
        let n = cartas.len() as u64;
        for (k, (c, p)) in cartas.into_iter().zip(origens).enumerate() {
            self.voo.retain(|(x, _, _, _)| x.id() != c.id());
            self.voo.push((c, p, id, k as u64 * passo));
        }
        let espera = 260 + n.saturating_sub(1) * passo;
        Task::perform(async move { std::thread::sleep(Duration::from_millis(espera)) }, move |_| Msg::Pousou(id))
    }

    fn escondida(&self, c: Carta) -> bool {
        self.voo.iter().any(|(x, _, _, _)| x.id() == c.id()) || self.girando.iter().any(|(x, _, _)| *x == c.id()) || self.arraste.as_ref().is_some_and(|a| self.mesa.pilha(a.origem)[a.idx..].iter().any(|x| x.id() == c.id()))
    }

    fn mesa_de_jogo(&self) -> Element<'_, Msg> {
        let m = &self.mesa;
        let mut camadas: Vec<Element<'_, Msg>> = vec![Space::new(LARGURA, ALTURA).into()];
        // Os lugares vazios: o monte recolhe o descarte, a fundação mostra o ás que espera.
        let reciclar = (!m.descarte.is_empty()).then(|| texto("↻", 40.0, MENTA.alpha(0.7)).into());
        camadas.push(no_lugar(mouse_area(vaga(reciclar)).on_press(Msg::Comprar).cursor_icon(CursorIcon::Pointer), Point::ORIGIN));
        camadas.push(no_lugar(container(texto("MONTE", 12.0, MENTA)).width(CW).align_x(Align::Center), Point::new(0.0, CH + 14.0)));
        camadas.push(no_lugar(vaga(None), m.posicao(Pilha::Descarte, 0)));
        for f in 0..4 {
            let as_ = Carta { naipe: f as u8, valor: 1, aberta: true };
            camadas.push(no_lugar(vaga(Some(animate(svg(as_.svg()).width(CW - 24.0).height(CH - 34.0)).opacity(0.18).into())), m.posicao(Pilha::Fundacao(f), 0)));
        }
        for i in 0..7 {
            camadas.push(no_lugar(vaga(None), m.posicao(Pilha::Coluna(i), 0)));
        }
        // As cartas, de baixo para cima; do monte, do descarte e das fundações só as de cima.
        let pilhas = [Pilha::Monte, Pilha::Descarte].into_iter().chain((0..4).map(Pilha::Fundacao)).chain((0..7).map(Pilha::Coluna));
        for p in pilhas {
            let cartas = m.pilha(p);
            let desde = if matches!(p, Pilha::Coluna(_)) { 0 } else { cartas.len().saturating_sub(if p == Pilha::Monte { 4 } else { 2 }) };
            for (idx, &c) in cartas.iter().enumerate().skip(desde) {
                if self.escondida(c) {
                    continue;
                }
                let el: Element<'_, Msg> = if p == Pilha::Monte {
                    mouse_area(carta(c, PX)).on_press(Msg::Comprar).cursor_icon(CursorIcon::Pointer).into()
                } else if m.pegavel(p, idx) {
                    mouse_area(carta(c, PX)).on_press(Msg::Pegar(p, idx)).on_double_click(Msg::Enviar(p, idx)).cursor_icon(CursorIcon::Pointer).into()
                } else {
                    carta(c, PX)
                };
                camadas.push(no_lugar(el, m.posicao(p, idx)));
            }
        }
        // As que voam: um lugar fixo pelo destino (altura na pilha, depois a pilha), para cada uma
        // entrar animada uma vez e ficar por baixo das que vão cobri-la.
        let pilhas = [Pilha::Monte, Pilha::Descarte].into_iter().chain((0..4).map(Pilha::Fundacao)).chain((0..7).map(Pilha::Coluna));
        let vaga = |p: Pilha, idx: usize| idx.min(51) * 13 + pilhas.clone().position(|x| x == p).expect("pilha conhecida");
        let mut voando: Vec<Element<'_, Msg>> = (0..52 * 13).map(|_| Space::new(0.0, 0.0).into()).collect();
        for &(c, de, _, atraso) in &self.voo {
            if let Some((p, idx)) = m.onde(c) {
                let alvo = m.posicao(p, idx);
                let entrada = Keyframe::default().offset(de.x - alvo.x, de.y - alvo.y);
                let face = match self.girando.iter().find(|(x, _, _)| *x == c.id()) {
                    Some(&(_, inicio, giro)) => carta_girando(m.pilha(p)[idx], inicio, giro),
                    None => carta(m.pilha(p)[idx], PX),
                };
                voando[vaga(p, idx)] = no_lugar(animate(face).enter(entrada).transition(Transition::new(220).delay(atraso).easing(Easing::EaseOut)), alvo);
            }
        }
        // As que giram paradas no lugar (as que giram voando já saíram acima).
        for &(id, inicio, giro) in &self.girando {
            if let Some((p, idx)) = m.onde(m.carta(id)).filter(|_| !self.voo.iter().any(|(x, _, _, _)| x.id() == id)) {
                voando[vaga(p, idx)] = no_lugar(carta_girando(m.pilha(p)[idx], inicio, giro), m.posicao(p, idx));
            }
        }
        // As cartas da mesa num stack só: o dos voos fica sempre no mesmo lugar (a chave da animação).
        // O primeiro filho dá o tamanho do stack: a mesa inteira, senão as de fora não aparecem.
        let voando = std::iter::once(Space::new(LARGURA, ALTURA).into()).chain(voando);
        let mut camadas: Vec<Element<'_, Msg>> = vec![stack(camadas).into(), stack(voando).into()];
        // As que estão na mão: seguem o mouse, mais altas (sombra maior).
        if let Some(a) = &self.arraste {
            let d = Point::new(a.mouse.x - a.inicio.x, a.mouse.y - a.inicio.y);
            let mao = m.pilha(a.origem)[a.idx..].iter().enumerate().map(|(k, &c)| {
                let p = m.posicao(a.origem, a.idx + k);
                no_lugar(carta(c, 3.0 * PX), Point::new(p.x + d.x - PX, p.y + d.y - PX))
            });
            camadas.push(stack(std::iter::once(Space::new(LARGURA, ALTURA).into()).chain(mao)).into());
        }
        // A festa: posição pelo deslocamento (não mexe no layout), por cima de tudo.
        if let Some(c) = &self.cascata {
            camadas.push(stack(std::iter::once(Space::new(LARGURA, ALTURA).into()).chain(c.pulos.iter().map(|p| no_lugar(animate(carta(p.carta, PX)).offset(p.x, p.y).transition(Transition::new(1)), Point::ORIGIN)))).into());
        }
        // A dica: a carta que deve ir e o lugar dela, piscando em dourado.
        let moldura = |p: Point, h: f32| -> Element<'_, Msg> {
            let caixa = container(Space::new(CW, h)).style(|_| Style::default().background(OURO.alpha(0.18)).border(Border::rounded(7.0).color(OURO, PX)));
            no_lugar(animate(caixa).enter(Keyframe::default().opacity(0.2)).repeat(Repeat::Alternate).transition(Transition::new(450)), p)
        };
        match self.dica {
            Some(Dica::Comprar) => camadas.push(moldura(Point::ORIGIN, CH)),
            Some(Dica::Mover(o, idx, d)) => {
                let (de, ate) = (m.posicao(o, idx), m.posicao(o, m.pilha(o).len() - 1));
                camadas.push(moldura(de, ate.y - de.y + CH));
                camadas.push(moldura(m.posicao(d, m.pilha(d).len().saturating_sub(1)), CH));
            }
            None => {}
        }
        stack(camadas).into()
    }

    /// O letreiro do fim (vitória ou derrota), por cima de tudo; a mesa embaixo não recebe cliques.
    fn letreiro(&self, titulo: &'static str, venceu: bool) -> Element<'_, Msg> {
        let resumo = format!("{} JOGADAS · {}", self.mesa.jogadas, tempo(self.segundos));
        let mut botoes = vec![container(botao("JOGAR DE NOVO", 18.0, Some(Msg::Novo), true)).width(240.0).into()];
        if !venceu {
            botoes.insert(0, container(botao("DESFAZER", 18.0, Some(Msg::Desfazer), false)).width(200.0).into());
        }
        let cartao = container(column![texto(titulo, 40.0, if venceu { OURO } else { ROSA }), texto(resumo, 16.0, MENTA), Space::new(0.0, 8.0), row(botoes).spacing(16)].spacing(18).align(Align::Center)).padding(36.0).style(|_| placa().shadow(sombra_dura(8.0)));
        let entra = Transition::new(320).delay(400).easing(Easing::EaseOut);
        let cortina = container(animate(cartao).enter(Keyframe::default().scale(0.6).opacity(0.0)).transition(entra)).center().style(|_| Style::default().background(Color::BLACK.alpha(0.55)));
        opaque(animate(cortina).enter(Keyframe::default().opacity(0.0)).transition(entra)).into()
    }
}

fn tempo(s: u32) -> String {
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl Paciencia {
    fn atualizar(&mut self, msg: Msg) -> Task<Msg> {
        // A dica some na próxima jogada.
        if !matches!(msg, Msg::Mouse(_) | Msg::Pousou(_) | Msg::Tique(_) | Msg::Girar(_)) {
            self.dica = None;
        }
        match msg {
            Msg::Dica => self.dica = self.mesa.dica(),
            Msg::Comprar => {
                let antes = self.mesa.clone();
                let de = self.mesa.posicao(Pilha::Monte, self.mesa.monte.len().saturating_sub(1));
                let virou = !self.mesa.monte.is_empty();
                if self.mesa.comprar() {
                    self.historico.push(antes);
                    // Virar uma carta: ela desliza do monte; recolher o descarte não anima.
                    if let Some(&c) = self.mesa.descarte.last().filter(|_| virou) {
                        let voo = self.voar(vec![c], vec![de]);
                        return Task::batch([voo, self.girar(c.id(), 0)]);
                    }
                }
            }
            Msg::Pegar(p, idx) => {
                if self.mesa.pegavel(p, idx) {
                    self.voo.retain(|(c, _, _, _)| !self.mesa.pilha(p)[idx..].iter().any(|x| x.id() == c.id()));
                    self.arraste = Some(Arraste { origem: p, idx, inicio: self.mouse, mouse: self.mouse });
                }
            }
            Msg::Mouse(p) => {
                self.mouse = p;
                if let Some(a) = &mut self.arraste {
                    a.mouse = p;
                }
            }
            Msg::Girar(p) => {
                if p == self.partida {
                    self.girando.retain(|(_, inicio, atraso)| inicio.elapsed().as_millis() < (atraso + GIRO + 30) as u128);
                    if !self.girando.is_empty() {
                        return quadro_do_giro(p);
                    }
                }
                self.girando_tique = false;
            }
            Msg::Quadro(p) => {
                if let Some(c) = self.cascata.as_mut().filter(|c| p == self.partida && !c.fim) {
                    c.passo(&mut self.mesa);
                    if !c.fim {
                        return quadro(p);
                    }
                }
            }
            Msg::Soltar => {
                // Um clique durante a festa pula direto para o letreiro.
                if let Some(c) = self.cascata.as_mut() {
                    c.pulos.clear();
                    self.mesa.fundacoes.iter_mut().for_each(Vec::clear);
                    c.fim = true;
                    return Task::none();
                }
                let Some(a) = self.arraste.take() else { return Task::none() };
                let d = Point::new(a.mouse.x - a.inicio.x, a.mouse.y - a.inicio.y);
                let n = self.mesa.pilha(a.origem).len() - a.idx;
                let onde: Vec<Point> = (0..n).map(|k| self.mesa.posicao(a.origem, a.idx + k)).map(|p| Point::new(p.x + d.x, p.y + d.y)).collect();
                // Quase sem mexer é um clique: a carta fica onde está (o clique duplo é que manda).
                if d.x.abs() + d.y.abs() < 6.0 {
                    return Task::none();
                }
                return match alvo_em(Point::new(onde[0].x + CW / 2.0, onde[0].y + CH / 2.0)) {
                    Some(dest) => self.fazer(a.origem, a.idx, dest, onde, false),
                    None => {
                        let cartas = self.mesa.pilha(a.origem)[a.idx..].to_vec();
                        self.voar(cartas, onde)
                    }
                };
            }
            Msg::Enviar(p, idx) => {
                self.arraste = None;
                if let Some(d) = self.mesa.destino_auto(p, idx).filter(|_| self.mesa.pegavel(p, idx)) {
                    let de = (idx..self.mesa.pilha(p).len()).map(|k| self.mesa.posicao(p, k)).collect();
                    return self.fazer(p, idx, d, de, true);
                }
            }
            Msg::Pousou(id) => {
                self.voo.retain(|(_, _, v, _)| *v != id);
            }
            Msg::Desfazer => {
                if let Some(m) = self.historico.pop() {
                    self.mesa = m;
                    self.voo.clear();
                    self.girando.clear();
                    self.arraste = None;
                }
            }
            Msg::Novo => {
                *self = Paciencia::embaralhando(self.partida + 1);
                return embaralhar(self.partida);
            }
            Msg::Distribuir(p, m) => {
                if p == self.partida && self.embaralhando {
                    self.mesa = m;
                    self.embaralhando = false;
                    // Dá as cartas do monte, linha por linha, como na mesa de verdade.
                    let cartas: Vec<Carta> = (0..7).flat_map(|j| (j..7).map(move |i| (i, j))).map(|(i, j)| self.mesa.colunas[i][j]).collect();
                    let n = cartas.len();
                    // A de cima de cada coluna chega fechada e gira ao pousar.
                    let giros: Vec<Task<Msg>> = cartas.iter().enumerate().filter(|(_, c)| c.aberta).map(|(k, c)| (c.id(), k as u64 * 30 + 200)).collect::<Vec<_>>().into_iter().map(|(id, atraso)| self.girar(id, atraso)).collect();
                    return Task::batch([self.voar_em_fila(cartas, vec![Point::ORIGIN; n], 30), relogio(p)].into_iter().chain(giros));
                }
            }
            Msg::Completar => return Task::done(Msg::Auto(self.partida)),
            Msg::Auto(p) => {
                if let Some((o, i, d)) = self.mesa.proximo_auto().filter(|_| p == self.partida) {
                    let de = vec![self.mesa.posicao(o, i)];
                    let voo = self.fazer(o, i, d, de, true);
                    return Task::batch([voo, Task::perform(async { std::thread::sleep(Duration::from_millis(140)) }, move |_| Msg::Auto(p))]);
                }
            }
            Msg::Tique(p) => {
                if p == self.partida && self.cascata.is_none() && self.mesa.dica().is_some() {
                    self.segundos += 1;
                    return relogio(p);
                }
            }
        }
        Task::none()
    }

}

impl App for Paciencia {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let tarefa = self.atualizar(msg);
        if self.mesa.venceu() && self.cascata.is_none() {
            // A última carta termina de voar, e começa a festa.
            self.cascata = Some(Cascata { pulos: Vec::new(), quadros: 0, lancadas: 0, sorteio: semente() | 1, fim: false });
            let p = self.partida;
            return Task::batch([tarefa, Task::perform(async { std::thread::sleep(Duration::from_millis(350)) }, move |_| Msg::Quadro(p))]);
        }
        tarefa
    }

    fn view(&self) -> Element<'_, Msg> {
        let titulo = container(text("PACIÊNCIA").font(PIXEL).size(32.0).color(OURO)).padding([10u16, 24]).style(|_| placa());
        let feltro = container(self.mesa_de_jogo()).padding(28.0).style(|_| Style::default().background(FELTRO).border(Border::rounded(0.0).color(MADEIRA_ESCURA, PX)));
        let moldura = container(feltro).padding(10.0).style(|_| Style::default().background(MADEIRA).border(Border::rounded(0.0).color(MADEIRA_ESCURA, PX)).shadow(sombra_dura(8.0)));
        let placar = texto(format!("JOGADAS {} · TEMPO {}", self.mesa.jogadas, tempo(self.segundos)), 16.0, MENTA);
        let completar: Element<'_, Msg> = if self.cascata.is_none() && self.mesa.pode_completar() {
            container(animate(botao("COMPLETAR", 16.0, Some(Msg::Completar), true)).opacity(0.75).repeat(Repeat::Alternate).transition(Transition::new(600))).width(200.0).into()
        } else {
            Space::new(200.0, 0.0).into()
        };
        let rodape = row![
            container(botao("DESFAZER", 16.0, (!self.historico.is_empty()).then_some(Msg::Desfazer), false)).width(180.0),
            container(botao("DICA", 16.0, Some(Msg::Dica), false)).width(120.0),
            horizontal_space(),
            placar,
            horizontal_space(),
            completar,
            container(botao("NOVO JOGO", 16.0, Some(Msg::Novo), true)).width(200.0),
        ]
        .spacing(16)
        .align(Align::Center)
        .width(LARGURA + 2.0 * (28.0 + 10.0 + 2.0 * PX));
        let mesa = column![titulo, moldura, rodape].spacing(22).align(Align::Center);
        let mut camadas = vec![container(mesa).center().style(|_| Style::default().background(FUNDO)).into()];
        if self.embaralhando {
            // Só aparece se o resolvedor demorar: o normal é menos de um décimo de segundo.
            let aviso = container(texto("EMBARALHANDO...", 24.0, OURO)).padding([14u16, 24]).style(|_| placa());
            camadas.push(opaque(container(animate(aviso).enter(Keyframe::default().opacity(0.0)).transition(Transition::new(200).delay(250))).center()).into());
        } else if self.voo.is_empty() && self.arraste.is_none() {
            if self.cascata.as_ref().is_some_and(|c| c.fim) {
                camadas.push(self.letreiro("VOCÊ VENCEU!", true));
            } else if self.cascata.is_none() && self.mesa.dica().is_none() {
                camadas.push(self.letreiro("SEM JOGADAS · VOCÊ PERDEU", false));
            }
        }
        // Por fora de tudo: o mouse é seguido e solto em qualquer ponto da janela.
        mouse_area(stack(camadas)).on_move(Msg::Mouse).on_release(Msg::Soltar).into()
    }
}

/// O ícone do menu: três cartas em leque.
pub(crate) fn icone<'a, M: 'a>() -> Element<'a, M> {
    let carta = |naipe: u8, valor: u8, graus: f32, x: f32| -> Element<'a, M> {
        let face = container(svg(svg_carta(naipe, valor)).width(64.0).height(90.0)).style(|_| Style::default().background(Color::WHITE).border(Border::rounded(5.0)).shadow(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(3.0, 3.0), blur: 0.0 }));
        container(animate(face).rotate(graus)).padding(Padding { top: 12.0, right: 0.0, bottom: 0.0, left: x }).into()
    };
    stack([Space::new(160.0, 110.0).into(), carta(2, 12, -14.0, 20.0), carta(1, 13, 0.0, 48.0), carta(0, 1, 14.0, 76.0)]).into()
}

/// O jogo pronto para abrir (no exemplo sozinho ou no menu do pack).
pub(crate) fn iniciar() -> (Paciencia, Task<Msg>) {
    (Paciencia::embaralhando(0), embaralhar(0))
}

pub(crate) fn principal() -> Result<(), Error> {
    carregar_fontes();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let mut p = Paciencia::novo(42, 0);
        for _ in 0..3 {
            p.mesa.comprar();
        }
        // Umas jogadas de verdade para a mesa não ficar no começo.
        for _ in 0..40 {
            let lance = (0..7).map(Pilha::Coluna).chain([Pilha::Descarte]).find_map(|o| {
                let pilha = p.mesa.pilha(o);
                let idx = pilha.iter().position(|c| c.aberta)?;
                let d = p.mesa.destino_auto(o, if o == Pilha::Descarte { pilha.len() - 1 } else { idx })?;
                // Rei de coluna sem nada embaixo não precisa ir para outra vazia.
                (!(idx == 0 && matches!(d, Pilha::Coluna(_)) && matches!(o, Pilha::Coluna(_)))).then_some((o, if o == Pilha::Descarte { pilha.len() - 1 } else { idx }, d))
            });
            match lance {
                Some((o, i, d)) => {
                    p.mesa.mover(o, i, d);
                }
                None => {
                    p.mesa.comprar();
                }
            }
        }
        p.segundos = 197;
        p.dica = p.mesa.dica();
        p.historico.push(p.mesa.clone());
        let png = capture(&p.view(), Size::new(1240.0, 940.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("paciencia.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Paciência".into(), size: Size::new(1240.0, 940.0), min_size: Some(Size::new(1200.0, 900.0)), ..Default::default() }, iniciar)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn c(naipe: u8, valor: u8) -> Carta {
        Carta { naipe, valor, aberta: true }
    }

    #[test]
    fn distribuicao() {
        let m = Mesa::embaralhada(1);
        assert_eq!(m.monte.len(), 24);
        for (i, col) in m.colunas.iter().enumerate() {
            assert_eq!(col.len(), i + 1);
            assert!(col.last().is_some_and(|c| c.aberta));
            assert_eq!(col.iter().filter(|c| c.aberta).count(), 1);
        }
        let mut ids: Vec<usize> = m.monte.iter().chain(m.colunas.iter().flatten()).map(|c| c.id()).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<_>>());
    }

    #[test]
    fn regras_de_coluna_e_fundacao() {
        let mut m = Mesa::embaralhada(1);
        m.colunas = Default::default();
        m.colunas[0] = vec![Carta { aberta: false, ..c(1, 4) }, c(0, 8)]; // 8 de copas por cima
        assert!(m.pode(&[c(1, 7)], Pilha::Coluna(0)), "7 preto no 8 vermelho");
        assert!(!m.pode(&[c(2, 7)], Pilha::Coluna(0)), "7 vermelho no 8 vermelho");
        assert!(m.pode(&[c(3, 13)], Pilha::Coluna(1)), "rei na vazia");
        assert!(!m.pode(&[c(3, 12)], Pilha::Coluna(1)));
        assert!(m.pode(&[c(2, 1)], Pilha::Fundacao(2)));
        assert!(!m.pode(&[c(2, 1)], Pilha::Fundacao(0)), "cada fundação é de um naipe");
        // Mover o 8 abre o 4 que estava embaixo.
        m.colunas[1] = vec![c(3, 9)];
        assert!(m.mover(Pilha::Coluna(0), 1, Pilha::Coluna(1)));
        assert!(m.colunas[0][0].aberta);
        assert_eq!(m.jogadas, 1);
    }

    #[test]
    fn monte_vira_e_recolhe() {
        let mut m = Mesa::embaralhada(3);
        let primeira = *m.monte.last().unwrap();
        for _ in 0..24 {
            assert!(m.comprar());
        }
        assert!(m.monte.is_empty());
        assert!(m.comprar(), "recolhe o descarte");
        assert_eq!(m.monte.len(), 24);
        assert_eq!(*m.monte.last().unwrap(), Carta { aberta: false, ..primeira });
    }

    #[test]
    fn clique_prefere_fundacao_e_completar_sobe_a_menor() {
        let mut m = Mesa::embaralhada(1);
        m.monte.clear();
        m.colunas = Default::default();
        m.colunas[0] = vec![c(0, 2)];
        m.colunas[1] = vec![c(0, 1)];
        m.colunas[2] = vec![c(1, 3)];
        assert_eq!(m.destino_auto(Pilha::Coluna(1), 0), Some(Pilha::Fundacao(0)));
        assert!(m.pode_completar());
        assert_eq!(m.proximo_auto(), Some((Pilha::Coluna(1), 0, Pilha::Fundacao(0))));
    }

    #[test]
    fn dica_e_derrota() {
        let mut m = Mesa::embaralhada(1);
        m.monte.clear();
        m.colunas = Default::default();
        m.colunas[0] = vec![Carta { aberta: false, ..c(0, 13) }, c(1, 2)];
        m.colunas[1] = vec![c(0, 9)];
        assert_eq!(m.dica(), None, "nada se mexe: perdeu");
        m.monte = vec![Carta { aberta: false, ..c(1, 1) }];
        assert_eq!(m.dica(), Some(Dica::Comprar), "o ás de espadas está no monte");
        m.comprar();
        assert_eq!(m.dica(), Some(Dica::Mover(Pilha::Descarte, 0, Pilha::Fundacao(1))));
        m.mover(Pilha::Descarte, 0, Pilha::Fundacao(1));
        assert_eq!(m.dica(), Some(Dica::Mover(Pilha::Coluna(0), 1, Pilha::Fundacao(1))), "o 2 sobe e abre o rei");
    }

    #[test]
    fn resolvedor_termina_os_jogos_que_aceita() {
        let t = std::time::Instant::now();
        let (mut aceitos, mut s) = (0, 1u64);
        for _ in 0..30 {
            s = xorshift(s);
            aceitos += tem_solucao(&Mesa::embaralhada(s), LIMITE) as u32;
        }
        eprintln!("{aceitos}/30 com solução em {:?}", t.elapsed());
        assert!(aceitos >= 15, "o resolvedor acha solução na maioria dos jogos: {aceitos}/30");
        let m = distribuir(99);
        assert!(tem_solucao(&m, LIMITE));
    }

    #[test]
    fn vitoria_faz_as_cartas_pularem_ate_sair() {
        let mut p = Paciencia::novo(1, 0);
        p.mesa.monte.clear();
        p.mesa.colunas = Default::default();
        p.mesa.colunas[0] = vec![c(3, 13)];
        p.mesa.fundacoes = std::array::from_fn(|f| (1..=13).map(|v| c(f as u8, v)).filter(|x| *x != c(3, 13)).collect());
        p.update(Msg::Enviar(Pilha::Coluna(0), 0));
        assert!(p.cascata.is_some(), "a última carta começa a festa");
        for _ in 0..20_000 {
            if p.cascata.as_ref().is_some_and(|c| c.fim) {
                break;
            }
            p.update(Msg::Quadro(0));
        }
        let c = p.cascata.as_ref().unwrap();
        assert!(c.fim && c.lancadas == 52, "as 52 saem da tela");
    }

    #[test]
    fn soltar_em_cima_de_uma_coluna() {
        assert_eq!(alvo_em(Point::new(CW / 2.0 + 2.0 * (CW + VAO) + 20.0, TOPO + 100.0)), Some(Pilha::Coluna(2)));
        assert_eq!(alvo_em(Point::new(CW / 2.0 + 4.0 * (CW + VAO), CH / 2.0)), Some(Pilha::Fundacao(1)));
        assert_eq!(alvo_em(Point::new(CW / 2.0, CH / 2.0)), None);
    }
}



