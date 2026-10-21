//! Xadrez contra o bot, no visual do pack (Baralho Pixelado): tabuleiro de feltro e creme, moldura
//! de madeira, peças em pixel art desenhadas aqui mesmo a partir de mapas de pixels.
//! Regras completas: xeque, mate, afogamento, roque, en passant, promoção (com escolha), empate por
//! material insuficiente, três repetições e 50 lances sem captura nem peão.
//! `cargo run --release --example xadrez_pixel`; `-- --capturar x.png` só gera o PNG.
use pelicano::column;
use pelicano::*;
use std::time::{Duration, Instant};

use crate::estilo::*;

const CASA: f32 = 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Cor {
    Branca,
    Preta,
}

impl Cor {
    fn outra(self) -> Cor {
        if self == Cor::Branca { Cor::Preta } else { Cor::Branca }
    }
    /// Topo, lateral e brilho do desenho.
    fn tintas(self) -> (Color, Color, Color) {
        match self {
            Cor::Branca => (Color::rgb8(0xec, 0xea, 0xf0), Color::rgb8(0xb9, 0xbf, 0xcc), Color::WHITE),
            Cor::Preta => (Color::rgb8(0xe2, 0x3b, 0x45), Color::rgb8(0x9e, 0x1e, 0x33), Color::rgb8(0xf7, 0xa6, 0xbc)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Tipo {
    Peao,
    Cavalo,
    Bispo,
    Torre,
    Dama,
    Rei,
}

const TIPOS: [Tipo; 6] = [Tipo::Peao, Tipo::Cavalo, Tipo::Bispo, Tipo::Torre, Tipo::Dama, Tipo::Rei];

impl Tipo {
    fn valor(self) -> i32 {
        [100, 320, 330, 500, 900, 0][self as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Peca {
    cor: Cor,
    tipo: Tipo,
}

/// Casa 0..64: coluna + 8 × linha (a1 = 0, h8 = 63).
type Sq = u8;

fn col(s: Sq) -> i8 {
    (s % 8) as i8
}
fn lin(s: Sq) -> i8 {
    (s / 8) as i8
}
fn sq(c: i8, l: i8) -> Option<Sq> {
    ((0..8).contains(&c) && (0..8).contains(&l)).then(|| (l * 8 + c) as Sq)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lance {
    de: Sq,
    para: Sq,
    promo: Option<Tipo>,
}

const ROQUE_BR: u8 = 1; // pequeno das brancas
const ROQUE_BD: u8 = 2; // grande das brancas
const ROQUE_PR: u8 = 4;
const ROQUE_PD: u8 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Posicao {
    tab: [Option<Peca>; 64],
    vez: Cor,
    roques: u8,
    /// A casa por onde o peão passou (alvo do en passant).
    ep: Option<Sq>,
    /// Meios-lances desde a última captura ou lance de peão (regra dos 50).
    meio: u32,
}

const CAVALO: [(i8, i8); 8] = [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)];
const REI: [(i8, i8); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
const RETAS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const DIAGONAIS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

impl Posicao {
    fn inicial() -> Posicao {
        Posicao::fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
    }

    fn fen(f: &str) -> Posicao {
        let partes: Vec<&str> = f.split_whitespace().collect();
        let mut tab = [None; 64];
        for (i, linha) in partes[0].split('/').enumerate() {
            let mut c = 0;
            for ch in linha.chars() {
                if let Some(n) = ch.to_digit(10) {
                    c += n as i8;
                    continue;
                }
                let tipo = match ch.to_ascii_lowercase() {
                    'p' => Tipo::Peao,
                    'n' => Tipo::Cavalo,
                    'b' => Tipo::Bispo,
                    'r' => Tipo::Torre,
                    'q' => Tipo::Dama,
                    _ => Tipo::Rei,
                };
                let cor = if ch.is_ascii_uppercase() { Cor::Branca } else { Cor::Preta };
                tab[sq(c, 7 - i as i8).expect("FEN dentro do tabuleiro") as usize] = Some(Peca { cor, tipo });
                c += 1;
            }
        }
        let roques = partes[2].chars().map(|ch| match ch {
            'K' => ROQUE_BR,
            'Q' => ROQUE_BD,
            'k' => ROQUE_PR,
            'q' => ROQUE_PD,
            _ => 0,
        });
        let ep = partes.get(3).and_then(|e| {
            let b = e.as_bytes();
            (b.len() == 2).then(|| sq((b[0] - b'a') as i8, (b[1] - b'1') as i8)).flatten()
        });
        Posicao { tab, vez: if partes[1] == "w" { Cor::Branca } else { Cor::Preta }, roques: roques.fold(0, |a, b| a | b), ep, meio: partes.get(4).and_then(|m| m.parse().ok()).unwrap_or(0) }
    }

    fn em(&self, s: Sq) -> Option<Peca> {
        self.tab[s as usize]
    }

    fn rei(&self, cor: Cor) -> Option<Sq> {
        (0..64).find(|&s| self.em(s) == Some(Peca { cor, tipo: Tipo::Rei }))
    }

    /// `s` é atacada por alguma peça de `por`?
    fn atacada(&self, s: Sq, por: Cor) -> bool {
        let (c, l) = (col(s), lin(s));
        let tem = |dc: i8, dl: i8, tipos: &[Tipo]| sq(c + dc, l + dl).and_then(|x| self.em(x)).is_some_and(|p| p.cor == por && tipos.contains(&p.tipo));
        let frente = if por == Cor::Branca { -1 } else { 1 };
        if tem(-1, frente, &[Tipo::Peao]) || tem(1, frente, &[Tipo::Peao]) {
            return true;
        }
        if CAVALO.iter().any(|&(dc, dl)| tem(dc, dl, &[Tipo::Cavalo])) || REI.iter().any(|&(dc, dl)| tem(dc, dl, &[Tipo::Rei])) {
            return true;
        }
        let desliza = |dirs: &[(i8, i8)], tipos: &[Tipo]| {
            dirs.iter().any(|&(dc, dl)| {
                let (mut x, mut y) = (c + dc, l + dl);
                while let Some(t) = sq(x, y) {
                    if let Some(p) = self.em(t) {
                        return p.cor == por && tipos.contains(&p.tipo);
                    }
                    x += dc;
                    y += dl;
                }
                false
            })
        };
        desliza(&RETAS, &[Tipo::Torre, Tipo::Dama]) || desliza(&DIAGONAIS, &[Tipo::Bispo, Tipo::Dama])
    }

    fn em_xeque(&self, cor: Cor) -> bool {
        self.rei(cor).is_some_and(|r| self.atacada(r, cor.outra()))
    }

    /// Lances que seguem o movimento das peças, sem olhar se deixam o rei em xeque.
    fn pseudo(&self) -> Vec<Lance> {
        let mut v = Vec::with_capacity(48);
        let eu = self.vez;
        for s in 0..64u8 {
            let Some(p) = self.em(s).filter(|p| p.cor == eu) else { continue };
            let (c, l) = (col(s), lin(s));
            let por = |para: Sq, v: &mut Vec<Lance>| v.push(Lance { de: s, para, promo: None });
            match p.tipo {
                Tipo::Peao => {
                    let (frente, base, ultima) = if eu == Cor::Branca { (1, 1, 7) } else { (-1, 6, 0) };
                    let avanca = |para: Sq, v: &mut Vec<Lance>| {
                        if lin(para) == ultima {
                            for t in [Tipo::Dama, Tipo::Cavalo, Tipo::Torre, Tipo::Bispo] {
                                v.push(Lance { de: s, para, promo: Some(t) });
                            }
                        } else {
                            v.push(Lance { de: s, para, promo: None });
                        }
                    };
                    if let Some(um) = sq(c, l + frente).filter(|&x| self.em(x).is_none()) {
                        avanca(um, &mut v);
                        if let Some(dois) = sq(c, l + 2 * frente).filter(|&x| l == base && self.em(x).is_none()) {
                            v.push(Lance { de: s, para: dois, promo: None });
                        }
                    }
                    for dc in [-1, 1] {
                        if let Some(x) = sq(c + dc, l + frente) {
                            if self.em(x).is_some_and(|q| q.cor != eu) || self.ep == Some(x) {
                                avanca(x, &mut v);
                            }
                        }
                    }
                }
                Tipo::Cavalo | Tipo::Rei => {
                    let saltos = if p.tipo == Tipo::Cavalo { &CAVALO } else { &REI };
                    for &(dc, dl) in saltos {
                        if let Some(x) = sq(c + dc, l + dl).filter(|&x| self.em(x).is_none_or(|q| q.cor != eu)) {
                            por(x, &mut v);
                        }
                    }
                }
                _ => {
                    let dirs: &[(i8, i8)] = match p.tipo {
                        Tipo::Torre => &RETAS,
                        Tipo::Bispo => &DIAGONAIS,
                        _ => &REI,
                    };
                    for &(dc, dl) in dirs {
                        let (mut x, mut y) = (c + dc, l + dl);
                        while let Some(t) = sq(x, y) {
                            match self.em(t) {
                                None => por(t, &mut v),
                                Some(q) => {
                                    if q.cor != eu {
                                        por(t, &mut v);
                                    }
                                    break;
                                }
                            }
                            x += dc;
                            y += dl;
                        }
                    }
                }
            }
        }
        // Roque: casas livres, e o rei não sai, não passa nem chega atacado.
        let (fila, pequeno, grande) = if eu == Cor::Branca { (0, ROQUE_BR, ROQUE_BD) } else { (56, ROQUE_PR, ROQUE_PD) };
        let livre = |d: &[u8]| d.iter().all(|&i| self.em(fila + i).is_none());
        let seguro = |d: &[u8]| d.iter().all(|&i| !self.atacada(fila + i, eu.outra()));
        if self.roques & pequeno != 0 && livre(&[5, 6]) && seguro(&[4, 5, 6]) {
            v.push(Lance { de: fila + 4, para: fila + 6, promo: None });
        }
        if self.roques & grande != 0 && livre(&[1, 2, 3]) && seguro(&[4, 3, 2]) {
            v.push(Lance { de: fila + 4, para: fila + 2, promo: None });
        }
        v
    }

    fn legais(&self) -> Vec<Lance> {
        self.pseudo().into_iter().filter(|l| !self.jogar(l).em_xeque(self.vez)).collect()
    }

    /// Onde o lance captura (a casa do peão no en passant) e o que.
    fn captura(&self, l: &Lance) -> Option<(Sq, Peca)> {
        if let Some(p) = self.em(l.para) {
            return Some((l.para, p));
        }
        let peao = self.em(l.de).is_some_and(|p| p.tipo == Tipo::Peao);
        (peao && self.ep == Some(l.para)).then(|| {
            let alvo = sq(col(l.para), lin(l.de)).expect("ao lado do peão");
            (alvo, self.em(alvo).expect("peão capturado no en passant"))
        })
    }

    /// A torre que anda junto no roque: (de, para).
    fn torre_do_roque(&self, l: &Lance) -> Option<(Sq, Sq)> {
        let rei = self.em(l.de).is_some_and(|p| p.tipo == Tipo::Rei);
        match (rei, l.para as i8 - l.de as i8) {
            (true, 2) => Some((l.de + 3, l.de + 1)),
            (true, -2) => Some((l.de - 4, l.de - 1)),
            _ => None,
        }
    }

    fn jogar(&self, l: &Lance) -> Posicao {
        let mut n = self.clone();
        let p = self.em(l.de).expect("lance sai de uma peça");
        let captura = self.captura(l);
        if let Some((s, _)) = captura {
            n.tab[s as usize] = None;
        }
        if let Some((a, b)) = self.torre_do_roque(l) {
            n.tab[b as usize] = n.tab[a as usize].take();
        }
        n.tab[l.de as usize] = None;
        n.tab[l.para as usize] = Some(Peca { tipo: l.promo.unwrap_or(p.tipo), ..p });
        n.ep = (p.tipo == Tipo::Peao && (l.para as i8 - l.de as i8).abs() == 16).then(|| (l.de + l.para) / 2);
        n.meio = if p.tipo == Tipo::Peao || captura.is_some() { 0 } else { self.meio + 1 };
        // Rei ou torre que sai (ou torre capturada) perde o roque daquele lado.
        for (s, bit) in [(4, ROQUE_BR | ROQUE_BD), (60, ROQUE_PR | ROQUE_PD), (0, ROQUE_BD), (7, ROQUE_BR), (56, ROQUE_PD), (63, ROQUE_PR)] {
            if l.de == s || l.para == s {
                n.roques &= !bit;
            }
        }
        n.vez = self.vez.outra();
        n
    }

    /// Ninguém consegue dar mate: reis sozinhos, ou com um bispo/cavalo, ou bispos da mesma cor.
    fn material_insuficiente(&self) -> bool {
        let resto: Vec<(Sq, Peca)> = (0..64).filter_map(|s| self.em(s).filter(|p| p.tipo != Tipo::Rei).map(|p| (s, p))).collect();
        match resto.as_slice() {
            [] => true,
            [(_, p)] => matches!(p.tipo, Tipo::Bispo | Tipo::Cavalo),
            [(a, p), (b, q)] => p.tipo == Tipo::Bispo && q.tipo == Tipo::Bispo && p.cor != q.cor && (col(*a) + lin(*a)) % 2 == (col(*b) + lin(*b)) % 2,
            _ => false,
        }
    }

    /// A posição para contar repetições: sem o relógio dos 50 lances.
    fn chave(&self) -> (Vec<Option<Peca>>, Cor, u8, Option<Sq>) {
        (self.tab.to_vec(), self.vez, self.roques, self.ep)
    }
}

#[cfg(test)]
fn perft(p: &Posicao, prof: u32) -> u64 {
    if prof == 0 {
        return 1;
    }
    p.legais().iter().map(|l| perft(&p.jogar(l), prof - 1)).sum()
}

// O bot: negamax com poda alfa-beta, aprofundamento progressivo e quiescência nas capturas.

/// Tabelas de posição (a "Simplified Evaluation Function"), da 8ª linha para a 1ª, vistas pelas brancas.
#[rustfmt::skip]
const PST: [[i8; 64]; 6] = [
    [0,0,0,0,0,0,0,0, 50,50,50,50,50,50,50,50, 10,10,20,30,30,20,10,10, 5,5,10,25,25,10,5,5, 0,0,0,20,20,0,0,0, 5,-5,-10,0,0,-10,-5,5, 5,10,10,-20,-20,10,10,5, 0,0,0,0,0,0,0,0],
    [-50,-40,-30,-30,-30,-30,-40,-50, -40,-20,0,0,0,0,-20,-40, -30,0,10,15,15,10,0,-30, -30,5,15,20,20,15,5,-30, -30,0,15,20,20,15,0,-30, -30,5,10,15,15,10,5,-30, -40,-20,0,5,5,0,-20,-40, -50,-40,-30,-30,-30,-30,-40,-50],
    [-20,-10,-10,-10,-10,-10,-10,-20, -10,0,0,0,0,0,0,-10, -10,0,5,10,10,5,0,-10, -10,5,5,10,10,5,5,-10, -10,0,10,10,10,10,0,-10, -10,10,10,10,10,10,10,-10, -10,5,0,0,0,0,5,-10, -20,-10,-10,-10,-10,-10,-10,-20],
    [0,0,0,0,0,0,0,0, 5,10,10,10,10,10,10,5, -5,0,0,0,0,0,0,-5, -5,0,0,0,0,0,0,-5, -5,0,0,0,0,0,0,-5, -5,0,0,0,0,0,0,-5, -5,0,0,0,0,0,0,-5, 0,0,0,5,5,0,0,0],
    [-20,-10,-10,-5,-5,-10,-10,-20, -10,0,0,0,0,0,0,-10, -10,0,5,5,5,5,0,-10, -5,0,5,5,5,5,0,-5, 0,0,5,5,5,5,0,-5, -10,5,5,5,5,5,0,-10, -10,0,5,0,0,0,0,-10, -20,-10,-10,-5,-5,-10,-10,-20],
    [-30,-40,-40,-50,-50,-40,-40,-30, -30,-40,-40,-50,-50,-40,-40,-30, -30,-40,-40,-50,-50,-40,-40,-30, -30,-40,-40,-50,-50,-40,-40,-30, -20,-30,-30,-40,-40,-30,-30,-20, -10,-20,-20,-20,-20,-20,-20,-10, 20,20,0,0,0,0,20,20, 20,30,10,0,0,10,30,20],
];

/// A nota da posição para quem joga.
fn avaliar(p: &Posicao) -> i32 {
    let mut s = 0;
    for x in 0..64u8 {
        if let Some(q) = p.em(x) {
            let i = if q.cor == Cor::Branca { (7 - lin(x)) * 8 + col(x) } else { lin(x) * 8 + col(x) } as usize;
            let v = q.tipo.valor() + PST[q.tipo as usize][i] as i32;
            s += if q.cor == p.vez { v } else { -v };
        }
    }
    s
}

const INF: i32 = 1_000_000;
const MATE: i32 = 100_000;

/// Capturas primeiro (a vítima mais valiosa pelo atacante mais barato), depois promoções.
fn ordenar(p: &Posicao, ls: &mut [Lance]) {
    ls.sort_by_key(|l| {
        let vitima = p.captura(l).map_or(0, |(_, q)| q.tipo.valor() * 10 - p.em(l.de).map_or(0, |a| a.tipo.valor()) / 10 + 10_000);
        -(vitima + l.promo.map_or(0, |t| t.valor()))
    });
}

// shortcut: a busca não vê empate por repetição nem guarda posições (sem tabela de transposição);
// entra se o difícil precisar ir mais fundo no mesmo tempo.
struct Busca {
    nos: u64,
    fim: Instant,
    parou: bool,
}

impl Busca {
    fn negamax(&mut self, p: &Posicao, prof: i32, ply: i32, mut a: i32, b: i32) -> i32 {
        self.nos += 1;
        if self.nos & 1023 == 0 && Instant::now() > self.fim {
            self.parou = true;
        }
        if self.parou {
            return 0;
        }
        let mut ls = p.legais();
        if ls.is_empty() {
            return if p.em_xeque(p.vez) { -MATE + ply } else { 0 };
        }
        if p.meio >= 100 {
            return 0;
        }
        if prof <= 0 {
            return self.quiescencia(p, a, b, 0);
        }
        ordenar(p, &mut ls);
        for l in &ls {
            let v = -self.negamax(&p.jogar(l), prof - 1, ply + 1, -b, -a);
            if v > a {
                a = v;
                if a >= b {
                    break;
                }
            }
        }
        a
    }

    /// Só capturas até a posição acalmar: parar no meio de uma troca engana a nota.
    fn quiescencia(&mut self, p: &Posicao, mut a: i32, b: i32, ply: i32) -> i32 {
        let parado = avaliar(p);
        if parado >= b {
            return b;
        }
        a = a.max(parado);
        if ply >= 6 {
            return a;
        }
        let mut caps: Vec<Lance> = p.pseudo().into_iter().filter(|l| p.captura(l).is_some() || l.promo == Some(Tipo::Dama)).collect();
        ordenar(p, &mut caps);
        for l in &caps {
            let n = p.jogar(l);
            if n.em_xeque(p.vez) {
                continue;
            }
            let v = -self.quiescencia(&n, -b, -a, ply + 1);
            if v >= b {
                return b;
            }
            a = a.max(v);
        }
        a
    }
}

/// O lance do bot: aprofunda até `prof` ou até o tempo acabar. Cada nota ganha um erro sorteado em
/// ±`ruido` (os níveis fáceis vacilam como gente); entre empatados, um sorteado.
fn melhor(p: &Posicao, prof: i32, ruido: i32, tempo: Duration) -> Option<Lance> {
    let mut ls = p.legais();
    ordenar(p, &mut ls);
    let mut semente = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1;
    let mut sortear = move || {
        semente ^= semente << 13;
        semente ^= semente >> 7;
        semente ^= semente << 17;
        semente
    };
    let mut busca = Busca { nos: 0, fim: Instant::now() + tempo, parou: false };
    let mut escolhido = *ls.first()?;
    for d in 1..=prof {
        let mut notas = Vec::with_capacity(ls.len());
        let mut a = -INF;
        for l in &ls {
            // Com erro, toda nota precisa ser exata; sem erro, a poda na raiz basta.
            let teto = if ruido > 0 { INF } else { -a };
            let v = -busca.negamax(&p.jogar(l), d - 1, 1, -INF, teto);
            if busca.parou {
                break;
            }
            a = a.max(v);
            notas.push((v, *l));
        }
        if busca.parou {
            break;
        }
        notas.sort_by_key(|(v, _)| -v);
        ls = notas.iter().map(|&(_, l)| l).collect();
        // Sem erro, as notas dos outros lances são só limites (podem empatar com a do melhor):
        // vale o primeiro com a nota máxima, que a ordenação estável deixa na frente.
        escolhido = if ruido == 0 { ls[0] } else { notas.iter().map(|&(v, l)| ((v + (sortear() % (2 * ruido as u64 + 1)) as i32 - ruido, sortear()), l)).max_by_key(|(k, _)| *k).map(|(_, l)| l)? };
    }
    Some(escolhido)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Nivel {
    Facil,
    Medio,
    Dificil,
}

impl Nivel {
    /// Profundidade, erro (um peão vale 100) e tempo máximo para pensar.
    fn busca(self) -> (i32, i32, Duration) {
        match self {
            Nivel::Facil => (1, 140, Duration::from_millis(400)),
            Nivel::Medio => (3, 40, Duration::from_millis(1200)),
            Nivel::Dificil => (6, 0, Duration::from_millis(2500)),
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

/// Os mapas das peças, 12 × 14: `X` é o corpo, `G` um enfeite dourado. Contorno, sombra e brilho
/// saem do formato.
#[rustfmt::skip]
const SPRITES: [[&str; 14]; 6] = [
    ["............", "............", "............", ".....XX.....", "....XXXX....", "....XXXX....", ".....XX.....", "....XXXX....", ".....XX.....", ".....XX.....", "....XXXX....", "...XXXXXX...", "..XXXXXXXX..", "..XXXXXXXX.."],
    ["............", ".....X.X....", "....XXXXX...", "...XXXXXXX..", "..XXX.XXXXX.", ".XXXXXXXXXX.", ".XXXXXXXXXX.", "..XX..XXXXX.", "......XXXXX.", ".....XXXXXX.", "....XXXXXX..", "...XXXXXXX..", "..XXXXXXXXX.", "..XXXXXXXXX."],
    [".....GG.....", "....XXXX....", "...XXX.XX...", "...XX.XXX...", "...XXXXXX...", "....XXXX....", ".....XX.....", "....XXXX....", ".....XX.....", ".....XX.....", "....XXXX....", "...XXXXXX...", "..XXXXXXXX..", "..XXXXXXXX.."],
    ["............", "............", "..XX.XX.XX..", "..XXXXXXXX..", "..XXXXXXXX..", "...XXXXXX...", "...XXXXXX...", "...XXXXXX...", "...XXXXXX...", "...XXXXXX...", "..XXXXXXXX..", "..XXXXXXXX..", ".XXXXXXXXXX.", ".XXXXXXXXXX."],
    [".G...GG...G.", ".XX..XX..XX.", ".XXX.XX.XXX.", ".XXXXXXXXXX.", "..XXXXXXXX..", "...XXXXXX...", "....XXXX....", "....XXXX....", "....XXXX....", "...XXXXXX...", "..XXXXXXXX..", "..GGGGGGGG..", ".XXXXXXXXXX.", ".XXXXXXXXXX."],
    [".....GG.....", "....GGGG....", ".....GG.....", "..XXXXXXXX..", ".XXXXXXXXXX.", ".XXXXXXXXXX.", "..XXXXXXXX..", "...XXXXXX...", "....XXXX....", "....XXXX....", "...XXXXXX...", "..XXXXXXXX..", ".XXXXXXXXXX.", ".XXXXXXXXXX."],
];

/// A peça centrada em `a`, cada pixel do mapa com `px` de lado.
fn desenhar_peca(r: &mut Renderer, a: Rect, p: Peca, px: f32) {
    let mapa = &SPRITES[p.tipo as usize];
    let dentro = |x: i32, y: i32| (0..12).contains(&x) && (0..14).contains(&y) && mapa[y as usize].as_bytes()[x as usize] != b'.';
    let (topo, lado, brilho) = p.cor.tintas();
    // Cada pixel: 0 nada, 1 contorno, 2 corpo, 3 lateral, 4 brilho, 5 ouro, 6 ouro escuro.
    let cor_em = |x: i32, y: i32| -> u8 {
        if !dentro(x, y) {
            return if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| dentro(x + dx, y + dy)) { 1 } else { 0 };
        }
        if mapa[y as usize].as_bytes()[x as usize] == b'G' {
            return if dentro(x + 1, y) && dentro(x, y + 1) { 5 } else { 6 };
        }
        if !dentro(x + 1, y) || !dentro(x, y + 1) {
            3
        } else if !dentro(x - 1, y) && !dentro(x, y - 1) || (!dentro(x - 1, y) && y % 3 == 0) {
            4
        } else {
            2
        }
    };
    let tinta = |k: u8| match k {
        1 => MARINHO,
        2 => topo,
        3 => lado,
        4 => brilho,
        5 => OURO,
        _ => OURO_ESCURO,
    };
    let x0 = (a.x + (a.width - 12.0 * px) / 2.0).round();
    let y0 = (a.y + (a.height - 14.0 * px) / 2.0).round();
    // Sombra dura: o formato com contorno, um pixel abaixo e à direita.
    for y in -1..15 {
        let mut x = -1;
        while x < 13 {
            if cor_em(x, y) == 0 {
                x += 1;
                continue;
            }
            let ini = x;
            while x < 13 && cor_em(x, y) != 0 {
                x += 1;
            }
            quad(r, x0 + (ini + 1) as f32 * px, y0 + (y + 1) as f32 * px, (x - ini) as f32 * px, px, Color::BLACK.alpha(0.3));
        }
    }
    // As cores, uma faixa por trecho igual da linha.
    for y in -1..15 {
        let mut x = -1;
        while x < 13 {
            let k = cor_em(x, y);
            let ini = x;
            while x < 13 && cor_em(x, y) == k {
                x += 1;
            }
            if k != 0 {
                quad(r, x0 + ini as f32 * px, y0 + y as f32 * px, (x - ini) as f32 * px, px, tinta(k));
            }
        }
    }
}

/// Quatro cantos em L: "dá para capturar aqui".
fn cantos(r: &mut Renderer, a: Rect, cor: Color) {
    let (l, e) = (3.0 * PX, PX);
    for (x, y, sx, sy) in [(a.x, a.y, 1.0, 1.0), (a.x + a.width, a.y, -1.0, 1.0), (a.x, a.y + a.height, 1.0, -1.0), (a.x + a.width, a.y + a.height, -1.0, -1.0)] {
        let (x0, y0) = (if sx > 0.0 { x } else { x - l }, if sy > 0.0 { y } else { y - e });
        quad(r, x0, y0, l, e, cor);
        let (x1, y1) = (if sx > 0.0 { x } else { x - e }, if sy > 0.0 { y } else { y - l });
        quad(r, x1, y1, e, l, cor);
    }
}

fn sprite<'a, M: 'a>(p: Peca, px: f32) -> Element<'a, M> {
    canvas(14.0 * px + px, 16.0 * px + px, move |r, a, _| desenhar_peca(r, a, p, px)).into()
}

/// O ícone do menu: rei branco e cavalo vermelho.
#[allow(dead_code)] // só o menu do pack usa; o exemplo sozinho não
pub(crate) fn icone<'a, M: 'a>() -> Element<'a, M> {
    canvas(160.0, 110.0, |r, a, _| {
        desenhar_peca(r, Rect { x: a.x + 18.0, y: a.y + 4.0, width: 72.0, height: 100.0 }, Peca { cor: Cor::Branca, tipo: Tipo::Rei }, 5.0);
        desenhar_peca(r, Rect { x: a.x + 76.0, y: a.y + 14.0, width: 72.0, height: 96.0 }, Peca { cor: Cor::Preta, tipo: Tipo::Cavalo }, 5.0);
    })
    .into()
}

#[derive(Debug, Clone)]
pub(crate) enum Msg {
    Casa(Sq),
    Promover(Tipo),
    CancelarPromocao,
    Bot(u32, Option<Lance>),
    Nivel(Nivel),
    Desfazer,
    Nova,
}

/// O último lance, para animar: a peça, a torre do roque e a capturada.
struct Ultimo {
    lance: Lance,
    torre: Option<(Sq, Sq)>,
    capturada: Option<(Sq, Peca)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Fim {
    Mate(Cor),
    Afogamento,
    Material,
    Repeticao,
    Cinquenta,
}

pub(crate) struct Xadrez {
    pos: Posicao,
    /// Todas as posições anteriores (repetições e desfazer).
    passado: Vec<Posicao>,
    /// O tamanho de `passado` antes de cada lance do jogador: o desfazer volta até ali.
    marcas: Vec<usize>,
    selecionada: Option<Sq>,
    promovendo: Option<(Sq, Sq)>,
    ultimo: Option<Ultimo>,
    nivel: Nivel,
    /// Muda a cada partida e a cada desfazer: a resposta atrasada do bot é jogada fora.
    partida: u32,
}

impl Xadrez {
    fn novo() -> Self {
        Xadrez { pos: Posicao::inicial(), passado: Vec::new(), marcas: Vec::new(), selecionada: None, promovendo: None, ultimo: None, nivel: Nivel::Medio, partida: 0 }
    }

    fn fim(&self) -> Option<Fim> {
        if self.pos.legais().is_empty() {
            return Some(if self.pos.em_xeque(self.pos.vez) { Fim::Mate(self.pos.vez.outra()) } else { Fim::Afogamento });
        }
        let chave = self.pos.chave();
        if self.pos.material_insuficiente() {
            Some(Fim::Material)
        } else if self.passado.iter().filter(|p| p.chave() == chave).count() >= 2 {
            Some(Fim::Repeticao)
        } else if self.pos.meio >= 100 {
            Some(Fim::Cinquenta)
        } else {
            None
        }
    }

    fn jogar(&mut self, l: Lance) {
        self.ultimo = Some(Ultimo { lance: l, torre: self.pos.torre_do_roque(&l), capturada: self.pos.captura(&l) });
        let novo = self.pos.jogar(&l);
        self.passado.push(std::mem::replace(&mut self.pos, novo));
        self.selecionada = None;
        self.promovendo = None;
    }

    fn jogar_eu(&mut self, l: Lance) -> Task<Msg> {
        self.marcas.push(self.passado.len());
        self.jogar(l);
        self.vez_do_bot()
    }

    /// O clique do jogador: escolhe a peça, depois o destino (o peão que promove pergunta a peça).
    fn clicar(&mut self, c: Sq) -> Task<Msg> {
        if self.pos.vez != Cor::Branca || self.fim().is_some() {
            return Task::none();
        }
        let ls = self.pos.legais();
        if let Some(s) = self.selecionada {
            let alvos: Vec<&Lance> = ls.iter().filter(|l| l.de == s && l.para == c).collect();
            match alvos.as_slice() {
                [] => {}
                [l] => return self.jogar_eu(**l),
                _ => {
                    self.promovendo = Some((s, c));
                    return Task::none();
                }
            }
        }
        self.selecionada = ls.iter().any(|l| l.de == c).then_some(c);
        Task::none()
    }

    fn vez_do_bot(&self) -> Task<Msg> {
        if self.pos.vez != Cor::Preta || self.fim().is_some() {
            return Task::none();
        }
        let (p, (prof, ruido, tempo), partida) = (self.pos.clone(), self.nivel.busca(), self.partida);
        Task::perform(
            async move {
                // Uma pausa para o lance do jogador terminar de deslizar.
                std::thread::sleep(Duration::from_millis(450));
                melhor(&p, prof, ruido, tempo)
            },
            move |l| Msg::Bot(partida, l),
        )
    }

    /// As peças que `cor` já capturou, da mais valiosa para a menos.
    fn capturadas(&self, cor: Cor) -> Vec<Peca> {
        let inicio: [usize; 6] = [8, 2, 2, 2, 1, 1];
        let vitima = cor.outra();
        TIPOS
            .iter()
            .rev()
            .flat_map(|&t| {
                let resta = self.pos.tab.iter().flatten().filter(|p| p.cor == vitima && p.tipo == t).count();
                std::iter::repeat_n(Peca { cor: vitima, tipo: t }, inicio[t as usize].saturating_sub(resta))
            })
            .collect()
    }

    fn casa(&self, c: Sq, legais: &[Lance]) -> Element<'_, Msg> {
        let escura = (col(c) + lin(c)) % 2 == 0;
        let peca = self.pos.em(c);
        let selecionada = self.selecionada == Some(c);
        let destino = self.selecionada.is_some_and(|s| legais.iter().any(|l| l.de == s && l.para == c));
        let captura = destino && self.pos.captura(&Lance { de: self.selecionada.unwrap_or(c), para: c, promo: None }).is_some();
        let rastro = self.ultimo.as_ref().is_some_and(|u| u.lance.de == c || u.lance.para == c);
        let xeque = peca.is_some_and(|p| p.tipo == Tipo::Rei && p.cor == self.pos.vez) && self.pos.em_xeque(self.pos.vez);
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
            if captura {
                cantos(r, a, OURO);
            } else if destino {
                let (x, y) = ((a.x + a.width / 2.0 - 2.0 * PX).round(), (a.y + a.height / 2.0 - 2.0 * PX).round());
                quad(r, x + PX, y + PX, 4.0 * PX, 4.0 * PX, Color::BLACK.alpha(0.35));
                quad(r, x, y, 4.0 * PX, 4.0 * PX, OURO);
            }
        });
        let mut camadas: Vec<Element<'_, Msg>> = vec![fundo.into()];
        // O rei em xeque: a casa pulsa em vermelho por baixo dele.
        if xeque {
            let brasa = canvas(Length::FILL, Length::FILL, |r, a, _| quad(r, a.x + PX, a.y + PX, a.width - 2.0 * PX, a.height - 2.0 * PX, ROSA.alpha(0.6)));
            camadas.push(animate(brasa).enter(Keyframe::default().opacity(0.15)).repeat(Repeat::Alternate).transition(Transition::new(520)).into());
        }
        // A capturada some encolhendo, depois que a outra passa.
        if let Some((_, p)) = self.ultimo.as_ref().and_then(|u| u.capturada).filter(|(s, _)| *s == c) {
            let fantasma = canvas(Length::FILL, Length::FILL, move |r, a, _| desenhar_peca(r, a, p, PX));
            camadas.push(animate(fantasma).opacity(0.0).scale(0.3).enter(Keyframe::default()).transition(Transition::new(240).delay(220).easing(Easing::EaseIn)).into());
        }
        if let Some(p) = peca {
            let desenho = canvas(Length::FILL, Length::FILL, move |r, a, _| desenhar_peca(r, a, p, PX));
            let mut anim = animate(desenho).offset(0.0, if selecionada { -2.0 * PX } else { 0.0 }).transition(Transition::new(320).easing(Easing::EaseOut));
            // A que acabou de jogar entra deslizando de onde saiu (a torre do roque também).
            let veio = self.ultimo.as_ref().and_then(|u| if u.lance.para == c { Some(u.lance.de) } else { u.torre.filter(|t| t.1 == c).map(|t| t.0) });
            if let Some(de) = veio {
                anim = anim.enter(Keyframe::default().offset((col(de) - col(c)) as f32 * CASA, (lin(c) - lin(de)) as f32 * CASA));
            }
            camadas.push(anim.into());
        }
        let clicavel = self.pos.vez == Cor::Branca && (destino || legais.iter().any(|l| l.de == c));
        button(stack(camadas)).padding(0.0).width(CASA).height(CASA).on_press_maybe(clicavel.then_some(Msg::Casa(c))).style(|_, _| Style::default()).into()
    }

    fn tabuleiro(&self) -> Element<'_, Msg> {
        let legais = if self.pos.vez == Cor::Branca && self.fim().is_none() { self.pos.legais() } else { Vec::new() };
        let rotulo = |s: String| texto(s, 14.0, MADEIRA_CLARA);
        let grade = column((0..8i8).rev().map(|l| row(std::iter::once(container(rotulo((l + 1).to_string())).width(24.0).height(CASA).align_y(Align::Center).into()).chain((0..8i8).map(|c| self.casa((l * 8 + c) as Sq, &legais)))).into()));
        let letras = row(std::iter::once(Space::new(24.0, 20.0).into()).chain((0..8u8).map(|c| container(rotulo(((b'a' + c) as char).to_string())).width(CASA).height(20.0).align_x(Align::Center).align_y(Align::Center).into())));
        container(column![grade, letras])
            .padding(Padding { top: 24.0, right: 24.0, bottom: 4.0, left: 0.0 })
            .style(|_| Style::default().background(MADEIRA).border(Border::rounded(0.0).color(MADEIRA_ESCURA, PX)).shadow(sombra_dura(8.0)))
            .into()
    }

    fn jogador(&self, nome: &'static str, cor: Cor) -> Element<'_, Msg> {
        let pegou = self.capturadas(cor);
        let saldo: i32 = pegou.iter().map(|p| p.tipo.valor()).sum::<i32>() - self.capturadas(cor.outra()).iter().map(|p| p.tipo.valor()).sum::<i32>();
        let detalhe = if saldo > 0 { format!("+{} DE MATERIAL", (saldo + 50) / 100) } else if pegou.is_empty() { "NENHUMA CAPTURA".into() } else { format!("{} CAPTURA{}", pegou.len(), if pegou.len() > 1 { "S" } else { "" }) };
        let da_vez = self.pos.vez == cor && self.fim().is_none();
        let marcador = animate(canvas(12.0, 12.0, |r, a, _| quad(r, a.x, a.y, a.width, a.height, OURO))).opacity(if da_vez { 0.15 } else { 0.0 }).enter(Keyframe::default().opacity(if da_vez { 1.0 } else { 0.0 })).repeat(Repeat::Alternate).transition(Transition::new(650));
        let icone = sprite(Peca { cor, tipo: Tipo::Rei }, 2.0);
        // Uma de cada tipo com a contagem: as 15 de um lado cabem numa linha.
        let tipos: Vec<(Peca, usize)> = TIPOS.iter().rev().filter_map(|&t| Some((*pegou.iter().find(|p| p.tipo == t)?, pegou.iter().filter(|p| p.tipo == t).count()))).collect();
        let pecas = row(tipos.into_iter().map(|(p, n)| row![sprite(p, 2.0), texto(if n > 1 { format!("×{n}") } else { String::new() }, 14.0, Color::WHITE.alpha(0.7))].align(Align::Center).into())).spacing(6);
        container(column![row![icone, column![texto(nome, 18.0, Color::WHITE), texto(detalhe, 14.0, MENTA)].spacing(8), horizontal_space(), marcador].spacing(12).align(Align::Center), pecas].spacing(6))
            .padding(12.0)
            .width(Length::FILL)
            .style(move |_| Style::default().background(FUNDO).border(Border::rounded(0.0).color(if da_vez { OURO } else { MARINHO }, PX)))
            .into()
    }

    fn painel(&self) -> Element<'_, Msg> {
        let estado: Element<'_, Msg> = match (self.fim(), self.pos.vez) {
            (Some(_), _) => texto("FIM DE JOGO", 18.0, OURO).into(),
            (None, Cor::Preta) => row![texto("PENSANDO", 18.0, OURO), animate(texto("...", 18.0, OURO)).opacity(0.1).repeat(Repeat::Alternate).transition(Transition::new(420))].into(),
            (None, Cor::Branca) if self.pos.em_xeque(Cor::Branca) => animate(texto("XEQUE!", 22.0, ROSA)).opacity(0.45).repeat(Repeat::Alternate).transition(Transition::new(500)).into(),
            (None, Cor::Branca) => texto("SUA VEZ", 18.0, OURO).into(),
        };
        let niveis = row([Nivel::Facil, Nivel::Medio, Nivel::Dificil].map(|n| botao(n.nome(), 14.0, Some(Msg::Nivel(n)), n == self.nivel))).spacing(10);
        let pode_desfazer = !self.marcas.is_empty() && (self.pos.vez == Cor::Branca || self.fim().is_some());
        container(
            column![
                self.jogador("BOT", Cor::Preta),
                self.jogador("VOCÊ", Cor::Branca),
                container(estado).height(30.0).align_y(Align::Center),
                texto("NÍVEL DO BOT", 14.0, MENTA),
                niveis,
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

    /// A escolha da peça do peão que chegou ao fim.
    fn promocao(&self) -> Element<'_, Msg> {
        let opcoes = row([Tipo::Dama, Tipo::Torre, Tipo::Bispo, Tipo::Cavalo].map(|t| {
            button(container(sprite(Peca { cor: Cor::Branca, tipo: t }, PX)).center())
                .padding(0.0)
                .width(88.0)
                .height(88.0)
                .on_press(Msg::Promover(t))
                .style(|_, s| Style::default().background(if s == Status::Hovered { FELTRO_CLARO } else { FELTRO }).border(Border::rounded(0.0).color(OURO, PX)).shadow(sombra_dura(PX)))
                .into()
        }))
        .spacing(14);
        let cartao = container(column![texto("PROMOVER O PEÃO", 24.0, OURO), opcoes, container(botao("CANCELAR", 14.0, Some(Msg::CancelarPromocao), false)).width(160.0)].spacing(20).align(Align::Center)).padding(32.0).style(|_| placa().shadow(sombra_dura(8.0)));
        let cortina = container(animate(cartao).enter(Keyframe::default().scale(0.7).opacity(0.0)).transition(Transition::new(200).easing(Easing::EaseOut))).center().style(|_| Style::default().background(Color::BLACK.alpha(0.45)));
        opaque(cortina).into()
    }

    /// O letreiro do fim, por cima de tudo, depois que o último lance termina de deslizar.
    fn letreiro(&self, fim: Fim) -> Element<'_, Msg> {
        let (titulo, sub, cor) = match fim {
            Fim::Mate(Cor::Branca) => ("XEQUE-MATE", "VOCÊ VENCEU!", OURO),
            Fim::Mate(Cor::Preta) => ("XEQUE-MATE", "O BOT VENCEU · QUE TAL A REVANCHE?", ROSA),
            Fim::Afogamento => ("EMPATE", "AFOGAMENTO: QUEM JOGA NÃO TEM LANCE", OURO),
            Fim::Material => ("EMPATE", "NINGUÉM TEM PEÇAS PARA DAR MATE", OURO),
            Fim::Repeticao => ("EMPATE", "A MESMA POSIÇÃO TRÊS VEZES", OURO),
            Fim::Cinquenta => ("EMPATE", "50 LANCES SEM CAPTURA NEM PEÃO", OURO),
        };
        let cartao = container(column![texto(titulo, 40.0, cor), texto(sub, 16.0, MENTA), Space::new(0.0, 8.0), container(botao("JOGAR DE NOVO", 18.0, Some(Msg::Nova), true)).width(260.0)].spacing(18).align(Align::Center)).padding(36.0).style(|_| placa().shadow(sombra_dura(8.0)));
        let entra = Transition::new(320).delay(500).easing(Easing::EaseOut);
        let cortina = container(animate(cartao).enter(Keyframe::default().scale(0.6).opacity(0.0)).transition(entra)).center().style(|_| Style::default().background(Color::BLACK.alpha(0.55)));
        opaque(animate(cortina).enter(Keyframe::default().opacity(0.0)).transition(entra)).into()
    }
}

impl App for Xadrez {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Casa(c) => return self.clicar(c),
            Msg::Promover(t) => {
                if let Some((de, para)) = self.promovendo {
                    let l = Lance { de, para, promo: Some(t) };
                    if self.pos.legais().contains(&l) {
                        return self.jogar_eu(l);
                    }
                }
            }
            Msg::CancelarPromocao => self.promovendo = None,
            Msg::Bot(p, l) => {
                if let (true, Cor::Preta, Some(l)) = (p == self.partida, self.pos.vez, l) {
                    self.jogar(l);
                }
            }
            Msg::Nivel(n) => self.nivel = n,
            Msg::Desfazer => {
                if let Some(m) = self.marcas.pop() {
                    self.pos = self.passado[m].clone();
                    self.passado.truncate(m);
                    *self = Xadrez { pos: self.pos.clone(), passado: std::mem::take(&mut self.passado), marcas: std::mem::take(&mut self.marcas), nivel: self.nivel, partida: self.partida + 1, ..Xadrez::novo() };
                }
            }
            Msg::Nova => *self = Xadrez { nivel: self.nivel, partida: self.partida + 1, ..Xadrez::novo() },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let titulo = container(text("XADREZ").font(LETREIRO).size(24.0).color(OURO)).padding([14u16, 24]).style(|_| placa());
        let mesa = column![titulo, row![self.tabuleiro(), self.painel()].spacing(36).align(Align::Start)].spacing(28).align(Align::Center);
        let mut camadas = vec![container(mesa).center().style(|_| Style::default().background(FUNDO)).into()];
        if self.promovendo.is_some() {
            camadas.push(self.promocao());
        }
        if let Some(f) = self.fim() {
            camadas.push(self.letreiro(f));
        }
        stack(camadas).into()
    }
}

/// O jogo pronto para abrir (no exemplo sozinho ou no menu do pack).
pub(crate) fn iniciar() -> (Xadrez, Task<Msg>) {
    (Xadrez::novo(), Task::none())
}

pub(crate) fn principal() -> Result<(), Error> {
    carregar_fontes();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        // Meio de partida: o bispo preto dá xeque de b4, o bispo de c1 escolhido para cobrir.
        let mut x = Xadrez::novo();
        x.passado.push(Posicao::inicial());
        x.marcas.push(0);
        x.pos = Posicao::fen("r2qk2r/ppp2ppp/2n1bn2/3p4/1b1P4/4PN2/PP3PPP/R1BQKB1R w KQkq - 2 8");
        x.ultimo = Some(Ultimo { lance: Lance { de: 61, para: 25, promo: None }, torre: None, capturada: None });
        x.selecionada = Some(2);
        let png = capture(&x.view(), Size::new(1100.0, 800.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("xadrez.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Xadrez".into(), size: Size::new(1100.0, 800.0), min_size: Some(Size::new(1060.0, 780.0)), ..Default::default() }, iniciar)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn casa(nome: &str) -> Sq {
        let b = nome.as_bytes();
        sq((b[0] - b'a') as i8, (b[1] - b'1') as i8).expect("casa válida")
    }

    #[test]
    fn perft_inicial_e_kiwipete() {
        let p = Posicao::inicial();
        assert_eq!([1, 2, 3].map(|d| perft(&p, d)), [20, 400, 8902]);
        let kiwi = Posicao::fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");
        assert_eq!(perft(&kiwi, 1), 48);
        assert_eq!(perft(&kiwi, 2), 2039);
    }

    #[test]
    fn roque_nao_passa_por_casa_atacada() {
        // A torre preta em f8 ataca f1: o roque pequeno não pode; o grande pode.
        let p = Posicao::fen("4kr2/8/8/8/8/8/8/R3K2R w KQ - 0 1");
        let ls = p.legais();
        assert!(!ls.contains(&Lance { de: casa("e1"), para: casa("g1"), promo: None }));
        assert!(ls.contains(&Lance { de: casa("e1"), para: casa("c1"), promo: None }));
        let depois = p.jogar(&Lance { de: casa("e1"), para: casa("c1"), promo: None });
        assert_eq!(depois.em(casa("d1")), Some(Peca { cor: Cor::Branca, tipo: Tipo::Torre }));
        assert_eq!(depois.roques, 0);
    }

    #[test]
    fn en_passant() {
        let p = Posicao::fen("4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1");
        let p = p.jogar(&Lance { de: casa("d7"), para: casa("d5"), promo: None });
        assert_eq!(p.ep, Some(casa("d6")));
        let l = Lance { de: casa("e5"), para: casa("d6"), promo: None };
        assert!(p.legais().contains(&l));
        let depois = p.jogar(&l);
        assert_eq!(depois.em(casa("d5")), None, "o peão preto saiu");
    }

    #[test]
    fn promocao_tem_quatro_escolhas() {
        let p = Posicao::fen("8/P6k/8/8/8/8/8/K7 w - - 0 1");
        let promos: Vec<Tipo> = p.legais().iter().filter(|l| l.de == casa("a7")).filter_map(|l| l.promo).collect();
        assert_eq!(promos.len(), 4);
        let depois = p.jogar(&Lance { de: casa("a7"), para: casa("a8"), promo: Some(Tipo::Cavalo) });
        assert_eq!(depois.em(casa("a8")), Some(Peca { cor: Cor::Branca, tipo: Tipo::Cavalo }));
    }

    #[test]
    fn afogamento_e_material() {
        let mut x = Xadrez::novo();
        x.pos = Posicao::fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        assert_eq!(x.fim(), Some(Fim::Afogamento));
        x.pos = Posicao::fen("7k/8/6K1/8/8/8/8/2B5 b - - 0 1");
        assert_eq!(x.fim(), Some(Fim::Material));
    }

    #[test]
    fn bot_acha_mate_em_um() {
        // Mate do corredor: Ra8#.
        let p = Posicao::fen("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        let l = melhor(&p, 3, 0, Duration::from_secs(5)).expect("tem lance");
        assert_eq!((l.de, l.para), (casa("a1"), casa("a8")));
    }

    #[test]
    fn partida_com_desfazer() {
        let mut x = Xadrez::novo();
        x.clicar(casa("e2"));
        x.clicar(casa("e4"));
        assert_eq!(x.pos.vez, Cor::Preta);
        let l = melhor(&x.pos, 2, 0, Duration::from_secs(5)).expect("bot responde");
        x.update(Msg::Bot(x.partida, Some(l)));
        assert_eq!(x.pos.vez, Cor::Branca);
        x.update(Msg::Desfazer);
        assert_eq!(x.pos, Posicao::inicial());
    }
}
