//! Poker Texas Hold'em sem limite: você contra três bots, na mesa pixelada do pack (feltro,
//! moldura de madeira, fichas desenhadas aqui mesmo). Blinds 10/20 dobram a cada 10 mãos;
//! all-in com potes paralelos; os bots calculam a chance de ganhar por Monte Carlo.
//! `cargo run --release --example poker`; `-- --capturar x.png` só gera o PNG.
//! shortcut: aumento curto de all-in reabre a rodada (a regra oficial não reabre); muda se alguém reclamar.
use pelicano::{column, row};
use pelicano::*;
use std::time::Duration;

use crate::cartas::*;
use crate::estilo::*;

const AREA: Size = Size { width: 1200.0, height: 900.0 };
const MESA: Rect = Rect { x: 100.0, y: 96.0, width: 1000.0, height: 520.0 };
const BARALHO: Point = Point { x: 846.0, y: 150.0 };
const POTE: Point = Point { x: 600.0, y: 474.0 };
/// As cartas comunitárias: cinco de 80x112 a partir daqui.
const COMUNS: Point = Point { x: 376.0, y: 300.0 };
const PEQ: Size = Size { width: 80.0, height: 112.0 };
const GRANDE: Size = Size { width: 120.0, height: 168.0 };

/// Onde cada lugar fica: 0 você (embaixo), 1 esquerda, 2 topo, 3 direita.
struct Lugar {
    cartas: Point,
    placa: Point,
    aposta: Point,
}

const LUGARES: [Lugar; 4] = [
    Lugar { cartas: Point { x: 505.0, y: 640.0 }, placa: Point { x: 285.0, y: 664.0 }, aposta: Point { x: 776.0, y: 530.0 } },
    Lugar { cartas: Point { x: 150.0, y: 270.0 }, placa: Point { x: 100.0, y: 398.0 }, aposta: Point { x: 352.0, y: 474.0 } },
    Lugar { cartas: Point { x: 556.0, y: 122.0 }, placa: Point { x: 346.0, y: 132.0 }, aposta: Point { x: 744.0, y: 186.0 } },
    Lugar { cartas: Point { x: 926.0, y: 270.0 }, placa: Point { x: 910.0, y: 398.0 }, aposta: Point { x: 848.0, y: 474.0 } },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct C {
    naipe: u8,
    /// 2..=14; o ás vale 14 (e 1 só na sequência A-2-3-4-5).
    valor: u8,
}

fn baralho() -> Vec<C> {
    (0..4).flat_map(|naipe| (2..=14).map(move |valor| C { naipe, valor })).collect()
}

fn xorshift(mut s: u64) -> u64 {
    s ^= s << 13;
    s ^= s >> 7;
    s ^ (s << 17)
}

fn semente() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1
}

/// Um sorteio em 0..1.
fn sorte(s: &mut u64) -> f32 {
    *s = xorshift(*s);
    (*s >> 40) as f32 / (1u64 << 24) as f32
}

// A força da mão: categoria nos bits de cima, desempate em 5 casas de 4 bits.

const NOMES: [&str; 9] = ["CARTA ALTA", "PAR", "DOIS PARES", "TRINCA", "SEQUÊNCIA", "FLUSH", "FULL HOUSE", "QUADRA", "STRAIGHT FLUSH"];

fn nome_da_mao(forca: u32) -> &'static str {
    match forca >> 20 {
        8 if (forca >> 16) & 15 == 14 => "ROYAL FLUSH",
        c => NOMES[c as usize],
    }
}

/// Até cinco cartas (sequência e flush só com cinco).
fn pontuar(cs: &[C]) -> u32 {
    let mut cont = [0u8; 15];
    for c in cs {
        cont[c.valor as usize] += 1;
    }
    // Os valores por (quantas vezes, valor), do mais forte ao mais fraco.
    let mut grupos: Vec<(u8, u8)> = (2..=14u8).rev().filter(|&v| cont[v as usize] > 0).map(|v| (cont[v as usize], v)).collect();
    grupos.sort_by(|a, b| b.cmp(a));
    let flush = cs.len() == 5 && cs.iter().all(|c| c.naipe == cs[0].naipe);
    let alta = if cs.len() == 5 && grupos.len() == 5 {
        if grupos[0].1 - grupos[4].1 == 4 {
            Some(grupos[0].1)
        } else if grupos.iter().map(|g| g.1).eq([14, 5, 4, 3, 2]) {
            Some(5)
        } else {
            None
        }
    } else {
        None
    };
    let categoria = match (alta, flush, grupos[0].0, grupos.get(1).map_or(0, |g| g.0)) {
        (Some(_), true, _, _) => 8,
        (_, _, 4, _) => 7,
        (_, _, 3, 2) => 6,
        (_, true, _, _) => 5,
        (Some(_), _, _, _) => 4,
        (_, _, 3, _) => 3,
        (_, _, 2, 2) => 2,
        (_, _, 2, _) => 1,
        _ => 0,
    };
    let desempate: Vec<u8> = match alta {
        Some(a) if categoria >= 4 && categoria != 5 => vec![a],
        _ => grupos.iter().map(|g| g.1).collect(),
    };
    desempate.iter().chain(std::iter::repeat(&0)).take(5).fold(categoria, |acc, &v| (acc << 4) | v as u32)
}

/// A melhor mão de cinco entre as cartas dadas, e quais são as cinco.
fn melhor(cs: &[C]) -> (u32, Vec<C>) {
    if cs.len() <= 5 {
        return (pontuar(cs), cs.to_vec());
    }
    let n = cs.len();
    (0u32..1 << n)
        .filter(|m| m.count_ones() == 5)
        .map(|m| {
            let cinco: Vec<C> = (0..n).filter(|i| m >> i & 1 == 1).map(|i| cs[i]).collect();
            (pontuar(&cinco), cinco)
        })
        .max_by_key(|(f, _)| *f)
        .expect("há pelo menos uma combinação")
}

/// Divide as fichas postas (`totais`) entre os que não desistiram, pote por pote: cada all-in
/// fecha um nível. Ficha que não divide exato vai para quem está mais perto à esquerda do dealer.
fn dividir(totais: [u32; 4], vivos: [bool; 4], forcas: [u32; 4], dealer: usize) -> [u32; 4] {
    let mut premio = [0u32; 4];
    let mut niveis: Vec<u32> = (0..4).filter(|&i| vivos[i]).map(|i| totais[i]).collect();
    niveis.sort();
    niveis.dedup();
    let (mut antes, mut distribuido, mut ultimos) = (0, 0, Vec::new());
    for nivel in niveis {
        let pote: u32 = totais.iter().map(|&t| t.min(nivel) - t.min(antes)).sum();
        let aptos: Vec<usize> = (0..4).filter(|&i| vivos[i] && totais[i] >= nivel).collect();
        let topo = aptos.iter().map(|&i| forcas[i]).max().unwrap_or(0);
        let ganham: Vec<usize> = (1..=4).map(|k| (dealer + k) % 4).filter(|i| aptos.contains(i) && forcas[*i] == topo).collect();
        repartir(&mut premio, &ganham, pote);
        distribuido += pote;
        antes = nivel;
        ultimos = ganham;
    }
    // Quem desistiu depois de pôr mais que todos os vivos: a sobra vai para o último pote.
    let sobra = totais.iter().sum::<u32>() - distribuido;
    if sobra > 0 && !ultimos.is_empty() {
        repartir(&mut premio, &ultimos, sobra);
    }
    premio
}

fn repartir(premio: &mut [u32; 4], ganham: &[usize], pote: u32) {
    if ganham.is_empty() {
        return;
    }
    let (parte, resto) = (pote / ganham.len() as u32, pote % ganham.len() as u32);
    for (k, &g) in ganham.iter().enumerate() {
        premio[g] += parte + u32::from((k as u32) < resto);
    }
}

// Os bots.

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Estilo {
    /// 0 só joga mão boa, 1 joga quase tudo.
    solto: f32,
    /// A chance de aumentar quando poderia só pagar.
    agressivo: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Acao {
    Desistir,
    /// Passar ou pagar: o que faltar para igualar.
    Pagar,
    /// Aumentar para este total na rodada (acima das fichas vira tudo).
    Aumentar(u32),
}

#[derive(Debug, Clone)]
struct Contexto {
    mao: [C; 2],
    mesa: Vec<C>,
    oponentes: usize,
    /// Tudo o que está na mesa, apostas da rodada inclusive.
    pote: u32,
    falta: u32,
    fichas: u32,
    maior: u32,
    min_para: u32,
    estilo: Estilo,
}

/// A chance de ganhar (empate conta a parte) contra `oponentes` mãos ao acaso.
fn equidade(mao: [C; 2], mesa: &[C], oponentes: usize, sims: u32, s: &mut u64) -> f32 {
    let resto: Vec<C> = baralho().into_iter().filter(|c| !mao.contains(c) && !mesa.contains(c)).collect();
    let faltam = 5 - mesa.len();
    let mut pontos = 0.0;
    let mut r = resto.clone();
    for _ in 0..sims {
        for k in 0..faltam + 2 * oponentes {
            *s = xorshift(*s);
            let j = k + (*s % (r.len() - k) as u64) as usize;
            r.swap(k, j);
        }
        let mut todas: Vec<C> = mesa.iter().copied().chain(r[..faltam].iter().copied()).collect();
        todas.extend(mao);
        let minha = melhor(&todas).0;
        let mut empates = 1;
        let mut perdeu = false;
        for o in 0..oponentes {
            todas.truncate(5);
            todas.extend(&r[faltam + 2 * o..faltam + 2 * o + 2]);
            let dele = melhor(&todas).0;
            if dele > minha {
                perdeu = true;
                break;
            }
            empates += u32::from(dele == minha);
        }
        if !perdeu {
            pontos += 1.0 / empates as f32;
        }
    }
    pontos / sims as f32
}

fn decidir(c: &Contexto, semente: u64) -> Acao {
    let mut s = semente | 1;
    let eq = equidade(c.mao, &c.mesa, c.oponentes.max(1), 300, &mut s);
    // 1.0 é a força média de uma mão nesta mesa; o estilo e um pouco de sorte mexem nela.
    let forca = eq * (c.oponentes + 1) as f32 + (sorte(&mut s) - 0.5) * 0.3 + (c.estilo.solto - 0.5) * 0.4;
    let odds = c.falta as f32 / (c.pote + c.falta).max(1) as f32;
    let pode_aumentar = c.fichas > c.falta;
    let aumento = |frac: f32| Acao::Aumentar((c.maior + ((c.pote + c.falta) as f32 * frac) as u32).max(c.min_para).div_ceil(5) * 5);
    let quer = sorte(&mut s) < c.estilo.agressivo;
    if pode_aumentar && quer && forca > 2.2 {
        return aumento(1.0);
    }
    if pode_aumentar && quer && forca > 1.5 {
        return aumento(0.5 + sorte(&mut s) * 0.3);
    }
    if c.falta == 0 {
        // Um blefe de vez em quando.
        return if pode_aumentar && sorte(&mut s) < c.estilo.agressivo * 0.15 { aumento(0.5) } else { Acao::Pagar };
    }
    if eq + 0.03 >= odds || sorte(&mut s) < 0.04 {
        Acao::Pagar
    } else {
        Acao::Desistir
    }
}

// O jogo.

#[derive(Debug, Clone)]
struct Jogador {
    nome: &'static str,
    fichas: u32,
    cartas: Option<[C; 2]>,
    /// Posto nesta rodada de apostas.
    aposta: u32,
    /// Posto na mão inteira (para os potes).
    total: u32,
    desistiu: bool,
    tudo: bool,
    fora: bool,
    /// O que fez por último, escrito na placa.
    acao: Option<String>,
    /// Cartas à mostra (showdown ou all-in).
    mostrar: bool,
    estilo: Estilo,
}

impl Jogador {
    fn novo(nome: &'static str, solto: f32, agressivo: f32) -> Self {
        Jogador { nome, fichas: 1000, cartas: None, aposta: 0, total: 0, desistiu: false, tudo: false, fora: false, acao: None, mostrar: false, estilo: Estilo { solto, agressivo } }
    }
    /// Ainda disputa esta mão.
    fn vivo(&self) -> bool {
        !self.fora && !self.desistiu && self.cartas.is_some()
    }
}

/// Fichas indo do lugar para o pote, ou do pote para quem ganhou.
#[derive(Debug, Clone)]
struct Voo {
    vaga: usize,
    de: Point,
    ate: Point,
    valor: u32,
    id: u32,
}

#[derive(Debug, Clone)]
pub(crate) enum Msg {
    Agir(Acao),
    Bot(u32, usize, Acao),
    Revelar(u32),
    Proxima(u32),
    Pousou(u32),
    Novo,
}

pub(crate) struct Poker {
    jogadores: [Jogador; 4],
    baralho: Vec<C>,
    mesa: Vec<C>,
    dealer: usize,
    maos: u32,
    bb: u32,
    pote: u32,
    maior: u32,
    min_aumento: u32,
    agiram: [bool; 4],
    /// Quem agiu por último: a vez segue dali.
    cursor: usize,
    vez: Option<usize>,
    /// Todos em all-in (menos um): as cartas que faltam saem sozinhas.
    runout: bool,
    /// A mão acabou; `resultado` diz quem levou.
    fim: bool,
    resultado: Option<String>,
    destaque: Vec<C>,
    /// Muda a cada passo do jogo: tarefa atrasada de um passo velho é ignorada.
    token: u32,
    semente: u64,
    voos: Vec<Voo>,
    voo_id: u32,
}

impl Poker {
    fn novo(semente: u64) -> Self {
        let jogadores = [Jogador::novo("VOCÊ", 0.5, 0.5), Jogador::novo("TONHO", 0.8, 0.6), Jogador::novo("BIA", 0.35, 0.55), Jogador::novo("DUDA", 0.7, 0.2)];
        let mut p = Poker { jogadores, baralho: Vec::new(), mesa: Vec::new(), dealer: 3, maos: 0, bb: 20, pote: 0, maior: 0, min_aumento: 20, agiram: [false; 4], cursor: 0, vez: None, runout: false, fim: false, resultado: None, destaque: Vec::new(), token: 0, semente, voos: Vec::new(), voo_id: 0 };
        p.nova_mao();
        p
    }

    fn proximo_ativo(&self, de: usize) -> usize {
        (1..=4).map(|k| (de + k) % 4).find(|&i| !self.jogadores[i].fora).unwrap_or(de)
    }

    fn acabou_o_jogo(&self) -> bool {
        self.jogadores[0].fichas == 0 || self.jogadores[1..].iter().all(|j| j.fichas == 0)
    }

    fn nova_mao(&mut self) {
        for j in &mut self.jogadores {
            j.fora |= j.fichas == 0;
            *j = Jogador { cartas: None, aposta: 0, total: 0, desistiu: false, tudo: false, acao: None, mostrar: false, ..j.clone() };
        }
        self.token += 1;
        self.mesa.clear();
        self.destaque.clear();
        (self.pote, self.fim, self.runout, self.resultado) = (0, false, false, None);
        if self.acabou_o_jogo() {
            self.vez = None;
            self.fim = true;
            return;
        }
        self.maos += 1;
        self.bb = 20 << ((self.maos - 1) / 10).min(10);
        self.dealer = self.proximo_ativo(self.dealer);
        self.semente = xorshift(self.semente);
        let mut s = self.semente;
        self.baralho = baralho();
        for i in (1..52).rev() {
            s = xorshift(s);
            self.baralho.swap(i, (s % (i as u64 + 1)) as usize);
        }
        let ordem = self.ordem_da_mesa();
        for &i in &ordem {
            let (a, b) = (self.baralho.pop().expect("52 cartas"), self.baralho.pop().expect("52 cartas"));
            self.jogadores[i].cartas = Some([a, b]);
        }
        // No mano a mano o dealer é o small blind.
        let sb = if ordem.len() == 2 { self.dealer } else { ordem[0] };
        let bb = self.proximo_ativo(sb);
        self.por(sb, self.bb / 2);
        self.por(bb, self.bb);
        self.jogadores[sb].acao = Some(format!("SMALL {}", self.bb / 2));
        self.jogadores[bb].acao = Some(format!("BIG {}", self.bb));
        self.maior = self.jogadores[sb].aposta.max(self.jogadores[bb].aposta);
        self.min_aumento = self.bb;
        self.agiram = [false; 4];
        self.cursor = bb;
        self.vez = None;
        self.avancar();
    }

    /// Os que estão na mesa, a partir da esquerda do dealer.
    fn ordem_da_mesa(&self) -> Vec<usize> {
        (1..=4).map(|k| (self.dealer + k) % 4).filter(|&i| !self.jogadores[i].fora).collect()
    }

    fn por(&mut self, i: usize, valor: u32) {
        let j = &mut self.jogadores[i];
        let v = valor.min(j.fichas);
        j.fichas -= v;
        j.aposta += v;
        j.total += v;
        j.tudo = j.fichas == 0;
    }

    fn aplicar(&mut self, i: usize, a: Acao) {
        let (aposta, fichas) = (self.jogadores[i].aposta, self.jogadores[i].fichas);
        let rotulo = match a {
            Acao::Desistir => {
                self.jogadores[i].desistiu = true;
                "DESISTIU".to_string()
            }
            Acao::Aumentar(para) if para.min(aposta + fichas) > self.maior => {
                // Abaixo do mínimo só vale se for tudo.
                let para = para.max(self.maior + self.min_aumento).min(aposta + fichas);
                let antes = self.maior;
                self.por(i, para - aposta);
                if para - antes >= self.min_aumento {
                    self.min_aumento = para - antes;
                }
                self.maior = para;
                self.agiram = [false; 4];
                if self.jogadores[i].tudo {
                    "TUDO!".to_string()
                } else if antes == 0 {
                    format!("APOSTOU {}", milhar(para))
                } else {
                    format!("AUMENTOU P/ {}", milhar(para))
                }
            }
            Acao::Pagar | Acao::Aumentar(_) => {
                let falta = self.maior - aposta;
                self.por(i, falta);
                match (falta, self.jogadores[i].tudo) {
                    (0, _) => "PASSOU".to_string(),
                    (_, true) => "TUDO!".to_string(),
                    _ => format!("PAGOU {}", milhar(falta.min(fichas))),
                }
            }
        };
        self.jogadores[i].acao = Some(rotulo);
        self.agiram[i] = true;
        self.cursor = i;
    }

    fn proximo(&self) -> Option<usize> {
        (1..=4).map(|k| (self.cursor + k) % 4).find(|&i| {
            let j = &self.jogadores[i];
            j.vivo() && !j.tudo && (j.aposta < self.maior || !self.agiram[i])
        })
    }

    /// Passa a vez, fecha a rodada ou a mão.
    fn avancar(&mut self) {
        self.token += 1;
        if self.jogadores.iter().filter(|j| j.vivo()).count() <= 1 {
            return self.encerrar();
        }
        if let Some(i) = self.proximo() {
            self.vez = Some(i);
            return;
        }
        self.vez = None;
        self.recolher();
        if self.mesa.len() == 5 {
            return self.encerrar();
        }
        if self.jogadores.iter().filter(|j| j.vivo() && !j.tudo).count() <= 1 {
            self.runout = true;
            for j in self.jogadores.iter_mut().filter(|j| j.vivo()) {
                j.mostrar = true;
            }
            return;
        }
        self.virar_rua();
        self.avancar();
    }

    fn virar_rua(&mut self) {
        let n = if self.mesa.is_empty() { 3 } else { 1 };
        for _ in 0..n {
            let c = self.baralho.pop().expect("sobram cartas");
            self.mesa.push(c);
        }
        self.maior = 0;
        self.min_aumento = self.bb;
        self.agiram = [false; 4];
        self.cursor = self.dealer;
        for j in &mut self.jogadores {
            if j.vivo() && !j.tudo {
                j.acao = None;
            }
        }
    }

    /// No all-in, a próxima rua (ou o showdown).
    fn revelar(&mut self) {
        self.token += 1;
        if self.mesa.len() < 5 {
            self.virar_rua();
        } else {
            self.encerrar();
        }
    }

    /// As apostas da rodada vão para o pote.
    fn recolher(&mut self) {
        let id = self.novo_voo();
        for i in 0..4 {
            let v = std::mem::take(&mut self.jogadores[i].aposta);
            if v > 0 {
                self.pote += v;
                self.voos.push(Voo { vaga: i, de: LUGARES[i].aposta, ate: POTE, valor: v, id });
            }
        }
    }

    fn novo_voo(&mut self) -> u32 {
        self.voo_id += 1;
        self.voo_id
    }

    fn encerrar(&mut self) {
        self.recolher();
        (self.vez, self.runout, self.fim) = (None, false, true);
        let vivos = self.jogadores.clone().map(|j| j.vivo());
        let totais = self.jogadores.clone().map(|j| j.total);
        let mut forcas = [0u32; 4];
        let mut melhores: [Vec<C>; 4] = Default::default();
        let showdown = vivos.iter().filter(|v| **v).count() > 1;
        for i in (0..4).filter(|&i| vivos[i] && showdown) {
            let cs: Vec<C> = self.jogadores[i].cartas.expect("vivo tem cartas").iter().chain(&self.mesa).copied().collect();
            (forcas[i], melhores[i]) = melhor(&cs);
            self.jogadores[i].mostrar = true;
        }
        let premio = dividir(totais, vivos, forcas, self.dealer);
        let id = self.novo_voo();
        for i in (0..4).filter(|&i| premio[i] > 0) {
            self.jogadores[i].fichas += premio[i];
            self.voos.push(Voo { vaga: 4 + i, de: POTE, ate: LUGARES[i].aposta, valor: premio[i], id });
        }
        self.pote = 0;
        let ganhou: Vec<usize> = (0..4).filter(|&i| premio[i] > 0).collect();
        let quem = |i: usize| self.jogadores[i].nome;
        self.resultado = Some(match (showdown, ganhou.as_slice()) {
            (false, [g]) => format!("{} LEVOU {}", quem(*g), milhar(premio[*g])),
            (true, [g]) => format!("{} GANHOU {} COM {}", quem(*g), milhar(premio[*g]), nome_da_mao(forcas[*g])),
            (_, gs) => {
                let nomes: Vec<&str> = gs.iter().map(|&g| quem(g)).collect();
                format!("{} DIVIDEM O POTE", nomes.join(" E "))
            }
        });
        if showdown {
            let topo = (0..4).max_by_key(|&i| forcas[i]).expect("quatro lugares");
            self.destaque = std::mem::take(&mut melhores[topo]);
        }
    }

    fn contexto(&self, i: usize) -> Contexto {
        let j = &self.jogadores[i];
        Contexto {
            mao: j.cartas.expect("quem joga tem cartas"),
            mesa: self.mesa.clone(),
            oponentes: self.jogadores.iter().enumerate().filter(|(k, o)| *k != i && o.vivo()).count(),
            pote: self.pote + self.jogadores.iter().map(|o| o.aposta).sum::<u32>(),
            falta: self.maior - j.aposta,
            fichas: j.fichas,
            maior: self.maior,
            min_para: self.min_para(),
            estilo: j.estilo,
        }
    }

    fn min_para(&self) -> u32 {
        if self.maior == 0 { self.bb } else { self.maior + self.min_aumento }
    }

    /// A tarefa que o estado pede: a jogada do bot, a próxima rua, a próxima mão.
    fn agendar(&self) -> Task<Msg> {
        let token = self.token;
        let esperar = move |ms: u64| async move { std::thread::sleep(Duration::from_millis(ms)) };
        let voos = match self.voos.last() {
            Some(v) => {
                let id = v.id;
                Task::perform(esperar(450), move |_| Msg::Pousou(id))
            }
            None => Task::none(),
        };
        let passo = if self.fim {
            if self.acabou_o_jogo() { Task::none() } else { Task::perform(esperar(4500), move |_| Msg::Proxima(token)) }
        } else if self.runout {
            Task::perform(esperar(1100), move |_| Msg::Revelar(token))
        } else if let Some(i) = self.vez.filter(|&i| i != 0) {
            let (ctx, s) = (self.contexto(i), xorshift(self.semente ^ token as u64));
            Task::perform(
                async move {
                    std::thread::sleep(Duration::from_millis(700));
                    decidir(&ctx, s)
                },
                move |a| Msg::Bot(token, i, a),
            )
        } else {
            Task::none()
        };
        Task::batch([voos, passo])
    }

    // O desenho.

    fn view_mesa(&self) -> Element<'_, Msg> {
        let mut camadas: Vec<Element<'_, Msg>> = vec![Space::new(AREA.width, AREA.height).into()];
        camadas.push(no_lugar(canvas(MESA.width + 12.0, MESA.height + 12.0, |r, a, _| desenhar_mesa(r, a)), Point::new(MESA.x, MESA.y)));
        let titulo = container(text("POKER · TEXAS HOLD'EM").font(LETREIRO).size(18.0).color(OURO)).padding([12u16, 18]).style(|_| placa());
        camadas.push(no_lugar(container(titulo).width(AREA.width).align_x(Align::Center), Point::new(0.0, 14.0)));
        let info = texto(format!("BLINDS {}/{} · MÃO {}", milhar(self.bb / 2), milhar(self.bb), self.maos), 14.0, MENTA);
        camadas.push(no_lugar(container(info).width(300.0).align_x(Align::End), Point::new(AREA.width - 300.0, 28.0)));
        // O baralho, de onde as cartas saem.
        let maco = stack(std::iter::once(Space::new(PEQ.width + 6.0, PEQ.height + 6.0).into()).chain((0..3).map(|k| no_lugar(carta_el(None, PEQ), Point::new(k as f32 * 3.0, (2 - k) as f32 * 3.0)))));
        camadas.push(no_lugar(maco, BARALHO));
        // O pote.
        if self.pote > 0 {
            camadas.push(no_lugar(fichas_el(self.pote, 5), Point::new(POTE.x - largura_fichas(self.pote, 5) / 2.0, POTE.y - 22.0)));
            let rotulo = text(format!("POTE {}", milhar(self.pote))).font(LETREIRO).size(14.0).color(OURO);
            camadas.push(no_lugar(container(rotulo).width(400.0).align_x(Align::Center), Point::new(POTE.x - 200.0, COMUNS.y + PEQ.height + 14.0)));
        }
        // As comunitárias: cada uma entra deslizando do baralho, as do flop uma depois da outra.
        let rua = if self.mesa.len() <= 3 { 0 } else { self.mesa.len() - 1 };
        let comuns = (0..5).map(|k| match self.mesa.get(k) {
            Some(&c) => {
                let p = pos_comum(k);
                let entra = Keyframe::default().offset(BARALHO.x - p.x, BARALHO.y - p.y);
                no_lugar(animate(carta_el(Some(c), PEQ)).enter(entra).transition(Transition::new(300).delay(k.saturating_sub(rua) as u64 * 120).easing(Easing::EaseOut)), p)
            }
            None => Space::new(0.0, 0.0).into(),
        });
        camadas.push(camada(comuns));
        for i in 0..4 {
            camadas.push(self.placa(i));
        }
        // As apostas na frente de cada um: entram vindas da placa.
        let apostas = (0..4).map(|i| {
            let v = self.jogadores[i].aposta;
            if v == 0 {
                return Space::new(0.0, 0.0).into();
            }
            let l = &LUGARES[i];
            let de = Point::new(l.placa.x + 95.0 - l.aposta.x, l.placa.y + 40.0 - l.aposta.y);
            no_lugar(animate(aposta_el(v)).enter(Keyframe::default().offset(de.x, de.y).opacity(0.0)).transition(Transition::new(260).easing(Easing::EaseOut)), Point::new(l.aposta.x - 60.0, l.aposta.y - 26.0))
        });
        camadas.push(camada(apostas));
        camadas.push(self.cartas_dos_lugares());
        // As cinco cartas da mão vencedora, marcadas em dourado.
        for c in &self.destaque {
            if let Some((p, t)) = self.onde_esta(*c) {
                let moldura = container(Space::new(t.width, t.height)).style(|_| Style::default().border(Border::rounded(6.0).color(OURO, PX)));
                camadas.push(no_lugar(animate(moldura).enter(Keyframe::default().opacity(0.3)).repeat(Repeat::Alternate).transition(Transition::new(500)), p));
            }
        }
        // Fichas voando, num lugar fixo por vaga (a chave da animação).
        let voando = (0..8).map(|vaga| match self.voos.iter().find(|v| v.vaga == vaga) {
            Some(v) => {
                let canto = Point::new(v.ate.x - largura_fichas(v.valor, 3) / 2.0, v.ate.y - 22.0);
                no_lugar(animate(fichas_el(v.valor, 3)).enter(Keyframe::default().offset(v.de.x - v.ate.x, v.de.y - v.ate.y)).transition(Transition::new(380).easing(Easing::EaseOut)), canto)
            }
            None => Space::new(0.0, 0.0).into(),
        });
        camadas.push(camada(voando));
        if let Some(nome) = self.jogadores[0].cartas.filter(|_| !self.jogadores[0].desistiu).map(|cs| nome_da_mao(melhor(&cs.iter().chain(&self.mesa).copied().collect::<Vec<_>>()).0)) {
            camadas.push(no_lugar(column![texto("SUA MÃO", 12.0, MENTA), texto(nome, 18.0, OURO)].spacing(6), Point::new(712.0, 690.0)));
        }
        camadas.push(no_lugar(container(self.barra()).width(AREA.width).align_x(Align::Center), Point::new(0.0, 836.0)));
        stack(camadas).into()
    }

    /// Onde uma carta à mostra está desenhada, e o tamanho dela.
    fn onde_esta(&self, c: C) -> Option<(Point, Size)> {
        if let Some(k) = self.mesa.iter().position(|x| *x == c) {
            return Some((pos_comum(k), PEQ));
        }
        (0..4).find_map(|i| {
            let k = self.jogadores[i].cartas?.iter().position(|x| *x == c)?;
            Some(pos_carta(i, k))
        })
    }

    fn cartas_dos_lugares(&self) -> Element<'_, Msg> {
        let ordem = self.ordem_da_mesa();
        let lugares = (0..4).map(|i| {
            let j = &self.jogadores[i];
            let Some(cs) = j.cartas.filter(|_| !(j.desistiu && i != 0)) else { return Space::new(0.0, 0.0).into() };
            let aberta = i == 0 || j.mostrar;
            let posicao_na_ordem = ordem.iter().position(|&o| o == i).unwrap_or(0);
            let cartas = (0..2).map(|k| {
                let (p, t) = pos_carta(i, k);
                let el = carta_el(aberta.then_some(cs[k]), t);
                let anim = if j.mostrar && i != 0 {
                    // Mostrar: vira com um pulo.
                    animate(el).enter(Keyframe::default().scale(0.6).opacity(0.0)).transition(Transition::new(240).easing(Easing::EaseOut))
                } else {
                    let atraso = (k * ordem.len() + posicao_na_ordem) as u64 * 80;
                    animate(el).enter(Keyframe::default().offset(BARALHO.x - p.x, BARALHO.y - p.y)).transition(Transition::new(280).delay(atraso).easing(Easing::EaseOut))
                };
                no_lugar(anim.opacity(if j.desistiu { 0.4 } else { 1.0 }), p)
            });
            // Ao mostrar, o índice muda: as cartas entram de novo, agora abertas.
            let mut filhos: Vec<Element<'_, Msg>> = if j.mostrar && i != 0 { vec![Space::new(0.0, 0.0).into()] } else { Vec::new() };
            filhos.extend(cartas);
            camada(filhos)
        });
        // A cada mão o índice também muda, para a distribuição animar de novo.
        let mut filhos: Vec<Element<'_, Msg>> = if self.maos % 2 == 0 { vec![Space::new(0.0, 0.0).into()] } else { Vec::new() };
        filhos.push(camada(lugares));
        camada(filhos)
    }

    fn placa(&self, i: usize) -> Element<'_, Msg> {
        let j = &self.jogadores[i];
        let l = &LUGARES[i];
        let da_vez = self.vez == Some(i);
        let acao = match (&j.acao, j.fora, da_vez && i != 0) {
            (_, true, _) => "FORA".to_string(),
            (_, _, true) => "PENSANDO...".to_string(),
            (Some(a), _, _) => a.clone(),
            (None, _, _) => String::new(),
        };
        let dealer = i == self.dealer && !j.fora;
        let cabeca = row![texto(j.nome, 16.0, Color::WHITE), horizontal_space(), botao_dealer(dealer)].align(Align::Center);
        let corpo = column![cabeca, texto(milhar(j.fichas), 14.0, MENTA), texto(acao, 12.0, if j.desistiu || j.fora { Color::WHITE.alpha(0.45) } else { OURO })].spacing(6);
        let caixa = container(corpo).padding(10.0).width(190.0).height(92.0).style(move |_| Style::default().background(PAINEL).border(Border::rounded(0.0).color(MARINHO, PX)).shadow(sombra_dura(PX)));
        let apagada = animate(caixa).opacity(if j.desistiu || j.fora { 0.5 } else { 1.0 });
        let mut camadas: Vec<Element<'_, Msg>> = vec![apagada.into()];
        if da_vez {
            let moldura = container(Space::new(190.0, 92.0)).style(|_| Style::default().border(Border::rounded(0.0).color(OURO, PX)));
            camadas.push(animate(moldura).enter(Keyframe::default().opacity(0.25)).repeat(Repeat::Alternate).transition(Transition::new(550)).into());
        }
        no_lugar(stack(camadas), l.placa)
    }

    /// Embaixo: os botões na sua vez, o resultado no fim da mão.
    fn barra(&self) -> Element<'_, Msg> {
        let eu = &self.jogadores[0];
        if self.fim {
            let texto_fim = self.resultado.clone().unwrap_or_default();
            let proxima = (!self.acabou_o_jogo()).then(|| Msg::Proxima(self.token));
            return row![texto(texto_fim, 20.0, OURO), container(botao_txt("PRÓXIMA MÃO".into(), proxima, true)).width(220.0)].spacing(24).align(Align::Center).into();
        }
        if self.vez != Some(0) {
            let status = match self.vez {
                Some(i) => format!("VEZ DE {}", self.jogadores[i].nome),
                None if self.runout => "TODOS NO TUDO: VIRANDO AS CARTAS".to_string(),
                None => String::new(),
            };
            return texto(status, 18.0, MENTA).into();
        }
        let falta = self.maior - eu.aposta;
        let tudo = eu.aposta + eu.fichas;
        let pote = self.pote + self.jogadores.iter().map(|j| j.aposta).sum::<u32>();
        let min = self.min_para();
        let pagar = match falta {
            0 => "PASSAR".to_string(),
            f if f >= eu.fichas => format!("PAGAR {}", milhar(eu.fichas)),
            f => format!("PAGAR {}", milhar(f)),
        };
        let aumento = |rotulo: &str, para: u32| -> Element<'_, Msg> {
            let para = para.max(min).div_ceil(5) * 5;
            let pode = eu.fichas > falta && para < tudo;
            container(botao_txt(format!("{rotulo} {}", milhar(para.min(tudo))), pode.then_some(Msg::Agir(Acao::Aumentar(para))), false)).width(150.0).into()
        };
        row![
            container(botao_txt("DESISTIR".into(), (falta > 0).then_some(Msg::Agir(Acao::Desistir)), false)).width(150.0),
            container(botao_txt(pagar, Some(Msg::Agir(Acao::Pagar)), true)).width(170.0),
            aumento("MÍN", min),
            aumento("½ POTE", self.maior + (pote + falta) / 2),
            aumento("POTE", self.maior + pote + falta),
            container(botao_txt(format!("TUDO {}", milhar(eu.fichas)), (eu.fichas > 0).then_some(Msg::Agir(Acao::Aumentar(tudo))), false)).width(170.0),
        ]
        .spacing(12)
        .into()
    }

    /// O letreiro do fim do jogo; a mesa embaixo não recebe cliques.
    fn letreiro(&self) -> Element<'_, Msg> {
        let (titulo, cor, sub) = if self.jogadores[0].fichas == 0 { ("VOCÊ QUEBROU", ROSA, "AS FICHAS ACABARAM") } else { ("VOCÊ LIMPOU A MESA!", OURO, "OS TRÊS BOTS QUEBRARAM") };
        let resumo = format!("{sub} · {} MÃOS", self.maos);
        let cartao = container(column![texto(titulo, 40.0, cor), texto(resumo, 16.0, MENTA), Space::new(0.0, 8.0), container(botao_txt("JOGAR DE NOVO".into(), Some(Msg::Novo), true)).width(260.0)].spacing(18).align(Align::Center)).padding(36.0).style(|_| placa().shadow(sombra_dura(8.0)));
        let entra = Transition::new(320).delay(900).easing(Easing::EaseOut);
        let cortina = container(animate(cartao).enter(Keyframe::default().scale(0.6).opacity(0.0)).transition(entra)).center().style(|_| Style::default().background(Color::BLACK.alpha(0.55)));
        opaque(animate(cortina).enter(Keyframe::default().opacity(0.0)).transition(entra)).into()
    }
}

fn pos_comum(k: usize) -> Point {
    Point::new(COMUNS.x + k as f32 * (PEQ.width + 12.0), COMUNS.y)
}

fn pos_carta(i: usize, k: usize) -> (Point, Size) {
    let l = &LUGARES[i];
    if i == 0 {
        (Point::new(l.cartas.x + k as f32 * 70.0, l.cartas.y), GRANDE)
    } else {
        (Point::new(l.cartas.x + k as f32 * 44.0, l.cartas.y), PEQ)
    }
}

fn milhar(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, ch) in s.chars().enumerate() {
        if k > 0 && (s.len() - k) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

/// Uma camada da mesa inteira. O primeiro filho de um `stack` dá o tamanho a todos: começa com a
/// área toda, senão o que vem depois fica espremido em nada.
fn camada<'a>(filhos: impl IntoIterator<Item = Element<'a, Msg>>) -> Element<'a, Msg> {
    stack(std::iter::once(Space::new(AREA.width, AREA.height).into()).chain(filhos)).into()
}

fn no_lugar<'a, M: 'a>(el: impl Into<Element<'a, M>>, p: Point) -> Element<'a, M> {
    container(el).padding(Padding { top: p.y, right: 0.0, bottom: 0.0, left: p.x }).into()
}

fn carta_el<'a, M: 'a>(c: Option<C>, t: Size) -> Element<'a, M> {
    let s = match c {
        Some(c) => svg_carta(c.naipe, if c.valor == 14 { 1 } else { c.valor }),
        None => svg_verso(),
    };
    let raio = t.width / 16.0;
    container(svg(s).width(t.width).height(t.height)).style(move |_| Style::default().background(Color::WHITE).border(Border::rounded(raio)).shadow(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(PX, PX), blur: 0.0 })).into()
}

/// O botão pixelado do pack com rótulo calculado (o de `estilo` só aceita texto fixo).
fn botao_txt<'a>(rotulo: String, msg: Option<Msg>, ativo: bool) -> Element<'a, Msg> {
    let corpo = button(container(text(rotulo).font(PIXEL).size(16.0)).width(Length::FILL).align_x(Align::Center))
        .padding([14u16, 8])
        .width(Length::FILL)
        .on_press_maybe(msg)
        .style(move |_, s| {
            let (fundo, tinta) = if ativo { (OURO, FUNDO) } else { (PAINEL, OURO) };
            let (fundo, tinta) = match s {
                Status::Hovered => (fundo.mix(Color::WHITE, 0.14), tinta),
                Status::Disabled => (fundo, tinta.alpha(0.35)),
                _ => (fundo, tinta),
            };
            let d = if s == Status::Pressed || s == Status::Disabled { 0.0 } else { PX };
            Style::default().background(fundo).text_color(tinta).border(Border::rounded(0.0).color(if s == Status::Disabled { OURO.alpha(0.35) } else { OURO }, PX)).shadow(Shadow { color: Color::BLACK, offset: Point::new(d, d), blur: 0.0 })
        });
    animate(corpo).pressed(Keyframe::default().offset(PX, PX)).transition(Transition::new(60)).into()
}

fn botao_dealer<'a>(mostrar: bool) -> Element<'a, Msg> {
    if !mostrar {
        return Space::new(24.0, 24.0).into();
    }
    let disco = canvas(24.0, 24.0, |r, a, _| {
        let (cx, cy) = ((a.x + a.width / 2.0).round(), (a.y + a.height / 2.0).round());
        disco(r, cx + 2.0, cy + 2.0, 6, 2.0, Color::BLACK.alpha(0.4));
        disco(r, cx, cy, 6, 2.0, MARINHO);
        disco(r, cx, cy, 5, 2.0, Color::rgb8(0xec, 0xea, 0xf0));
    });
    stack![disco, container(text("D").font(LETREIRO).size(10.0).color(MARINHO)).center()].into()
}

// Pixel art: mesa e fichas.

fn quad(r: &mut Renderer, x: f32, y: f32, w: f32, h: f32, cor: Color) {
    r.fill_quad(Rect { x, y, width: w, height: h }, Some(&cor.into()), Border::rounded(0.0), None);
}

fn disco(r: &mut Renderer, cx: f32, cy: f32, raio: i32, px: f32, cor: Color) {
    for y in -raio..raio {
        let meio = y as f32 + 0.5;
        let w = ((raio * raio) as f32 - meio * meio).sqrt().round();
        quad(r, cx - w * px, cy + y as f32 * px, 2.0 * w * px, px, cor);
    }
}

/// Uma pílula em degraus (as pontas são meio círculo), uma faixa por degrau.
fn pilula(r: &mut Renderer, x: f32, y: f32, w: f32, h: f32, passo: f32, cor: Color) {
    let raio = h / 2.0;
    let mut yy = 0.0;
    while yy < h {
        let meio = yy + passo / 2.0 - raio;
        let recuo = ((raio - (raio * raio - meio * meio).max(0.0).sqrt()) / passo).round() * passo;
        quad(r, x + recuo, y + yy, w - 2.0 * recuo, passo.min(h - yy), cor);
        yy += passo;
    }
}

fn desenhar_mesa(r: &mut Renderer, a: Rect) {
    let (w, h) = (MESA.width, MESA.height);
    let p = 8.0;
    pilula(r, a.x + 12.0, a.y + 12.0, w, h, p, Color::BLACK.alpha(0.4));
    pilula(r, a.x, a.y, w, h, p, MADEIRA_ESCURA);
    pilula(r, a.x + 6.0, a.y + 6.0, w - 12.0, h - 12.0, p, MADEIRA);
    pilula(r, a.x + 14.0, a.y + 14.0, w - 28.0, h - 28.0, p, Color::rgb8(0xb9, 0x7d, 0x47));
    pilula(r, a.x + 22.0, a.y + 22.0, w - 44.0, h - 44.0, p, MADEIRA_ESCURA);
    pilula(r, a.x + 28.0, a.y + 28.0, w - 56.0, h - 56.0, p, FELTRO_ESCURO);
    pilula(r, a.x + 34.0, a.y + 34.0, w - 68.0, h - 68.0, p, FELTRO);
    // A linha clara por dentro, como nas mesas de cassino.
    pilula(r, a.x + 70.0, a.y + 70.0, w - 140.0, h - 140.0, p, FELTRO_CLARO);
    pilula(r, a.x + 74.0, a.y + 74.0, w - 148.0, h - 148.0, p, FELTRO);
}

/// As cores das fichas, da maior para a menor: (valor, topo, lateral).
const FICHAS: [(u32, Color, Color); 5] = [
    (500, OURO, OURO_ESCURO),
    (100, Color::rgb8(0x4a, 0x4f, 0x8f), MARINHO),
    (25, Color::rgb8(0x86, 0xae, 0xe6), Color::rgb8(0x4a, 0x6f, 0xb0)),
    (5, Color::rgb8(0xe2, 0x3b, 0x45), Color::rgb8(0x9e, 0x1e, 0x33)),
    (1, Color::rgb8(0xec, 0xea, 0xf0), Color::rgb8(0xb9, 0xbf, 0xcc)),
];

/// O valor em pilhas: (cor, quantas), no máximo `max` pilhas de até 8.
fn pilhas(mut valor: u32, max: usize) -> Vec<(usize, u32)> {
    let mut v = Vec::new();
    for (k, (d, _, _)) in FICHAS.iter().enumerate() {
        let n = valor / d;
        valor %= d;
        if n > 0 {
            v.push((k, n.min(8)));
        }
    }
    v.truncate(max);
    v
}

fn largura_fichas(valor: u32, max: usize) -> f32 {
    pilhas(valor, max).len().max(1) as f32 * 40.0
}

fn ficha(r: &mut Renderer, cx: f32, y: f32, topo: Color, lado: Color) {
    let p = 3.0;
    let elipse = |r: &mut Renderer, y: f32, cor: Color| {
        for (k, w) in [8.0, 12.0, 12.0, 8.0].iter().enumerate() {
            quad(r, cx - w * p / 2.0, y + k as f32 * p, w * p, p, cor);
        }
    };
    elipse(r, y + 2.0 * p, lado);
    elipse(r, y, topo);
    // As listras brancas da borda.
    quad(r, cx - 6.0 * p, y + 2.0 * p, p, 2.0 * p, Color::WHITE.alpha(0.85));
    quad(r, cx + 5.0 * p, y + 2.0 * p, p, 2.0 * p, Color::WHITE.alpha(0.85));
    quad(r, cx - p, y, 2.0 * p, p, Color::WHITE.alpha(0.6));
}

fn desenhar_pilhas(r: &mut Renderer, a: Rect, ps: &[(usize, u32)]) {
    for (k, &(tipo, n)) in ps.iter().enumerate() {
        let cx = a.x + 20.0 + k as f32 * 40.0;
        let base = a.y + a.height - 18.0;
        let (_, topo, lado) = FICHAS[tipo];
        quad(r, cx - 15.0, base + 15.0, 36.0, 6.0, Color::BLACK.alpha(0.3));
        for m in 0..n {
            ficha(r, cx, base - m as f32 * 6.0, topo, lado);
        }
    }
}

fn fichas_el<'a, M: 'a>(valor: u32, max: usize) -> Element<'a, M> {
    let ps = pilhas(valor, max);
    canvas(largura_fichas(valor, max), 66.0, move |r, a, _| desenhar_pilhas(r, a, &ps)).into()
}

/// A aposta na frente do lugar: as fichas e o valor.
fn aposta_el<'a>(valor: u32) -> Element<'a, Msg> {
    container(column![fichas_el(valor, 3), texto(milhar(valor), 14.0, Color::WHITE)].spacing(2).align(Align::Center)).width(120.0).align_x(Align::Center).into()
}

/// O ícone do menu: duas cartas e uma pilha de fichas.
#[allow(dead_code)] // só o menu do pack usa
pub(crate) fn icone<'a, M: 'a>() -> Element<'a, M> {
    let t = Size::new(56.0, 78.0);
    let a = carta_el(Some(C { naipe: 1, valor: 14 }), t);
    let k = carta_el(Some(C { naipe: 0, valor: 13 }), t);
    stack![
        Space::new(160.0, 110.0),
        no_lugar(animate(a).rotate(-10.0), Point::new(22.0, 12.0)),
        no_lugar(animate(k).rotate(8.0), Point::new(62.0, 8.0)),
        no_lugar(fichas_el(625, 3), Point::new(84.0, 44.0)),
    ]
    .into()
}

impl App for Poker {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Agir(a) if self.vez == Some(0) => {
                self.aplicar(0, a);
                self.avancar();
            }
            Msg::Bot(t, i, a) if t == self.token && self.vez == Some(i) => {
                self.aplicar(i, a);
                self.avancar();
            }
            Msg::Revelar(t) if t == self.token && self.runout => self.revelar(),
            Msg::Proxima(t) if t == self.token && self.fim && !self.acabou_o_jogo() => self.nova_mao(),
            Msg::Pousou(id) => {
                self.voos.retain(|v| v.id != id);
                return Task::none();
            }
            Msg::Novo => {
                let token = self.token + 1;
                *self = Poker::novo(semente());
                self.token += token;
            }
            _ => return Task::none(),
        }
        self.agendar()
    }

    fn view(&self) -> Element<'_, Msg> {
        let mut camadas = vec![container(self.view_mesa()).center().style(|_| Style::default().background(FUNDO)).into()];
        if self.fim && self.acabou_o_jogo() {
            camadas.push(self.letreiro());
        }
        stack(camadas).into()
    }
}

/// O jogo pronto para abrir (no exemplo sozinho ou no menu do pack).
pub(crate) fn iniciar() -> (Poker, Task<Msg>) {
    let p = Poker::novo(semente());
    let t = p.agendar();
    (p, t)
}

/// Joga sozinho (todos decidem como bot) até a condição, sem tarefas: testes e captura.
fn jogar_ate(p: &mut Poker, parar: impl Fn(&Poker) -> bool, passos: usize) {
    for _ in 0..passos {
        if parar(p) {
            return;
        }
        if p.fim {
            if p.acabou_o_jogo() {
                return;
            }
            p.nova_mao();
        } else if p.runout {
            p.revelar();
        } else if let Some(i) = p.vez {
            let a = decidir(&p.contexto(i), xorshift(p.semente ^ p.token as u64));
            p.aplicar(i, a);
            p.avancar();
        }
    }
}

pub(crate) fn principal() -> Result<(), Error> {
    carregar_fontes();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        // Uma mão no turn, com aposta na mesa e a vez sua.
        let mut p = Poker::novo(11);
        jogar_ate(&mut p, |p| p.mesa.len() == 4 && p.vez == Some(0) && p.maior > 0 && !p.jogadores[0].desistiu, 4000);
        p.voos.clear();
        let png = capture(&p.view(), Size::new(1240.0, 940.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("poker.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Poker".into(), size: Size::new(1240.0, 940.0), min_size: Some(Size::new(1220.0, 920.0)), ..Default::default() }, iniciar)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cs(s: &str) -> Vec<C> {
        // "Ah Kd 10s 2c": valor + naipe (h copas, s espadas, d ouros, c paus).
        s.split_whitespace()
            .map(|t| {
                let (v, n) = t.split_at(t.len() - 1);
                let valor = match v {
                    "A" => 14,
                    "K" => 13,
                    "Q" => 12,
                    "J" => 11,
                    v => v.parse().expect("valor"),
                };
                C { naipe: "hsdc".find(n).expect("naipe") as u8, valor }
            })
            .collect()
    }

    fn f(s: &str) -> u32 {
        melhor(&cs(s)).0
    }

    #[test]
    fn categorias() {
        let casos = [
            ("Ah Kd 9s 7c 2h 3d 4s", "CARTA ALTA"),
            ("Ah Ad 9s 7c 2h 3d Js", "PAR"),
            ("Ah Ad 9s 9c 2h 3d Js", "DOIS PARES"),
            ("Ah Ad As 9c 2h 3d Js", "TRINCA"),
            ("5h 6d 7s 8c 9h Kd Ks", "SEQUÊNCIA"),
            ("Ah 2d 3s 4c 5h Kd Ks", "SEQUÊNCIA"),
            ("Ah 2h 9h 4h 5h Kd Ks", "FLUSH"),
            ("Ah Ad As 9c 9h 3d Js", "FULL HOUSE"),
            ("Ah Ad As Ac 9h 3d Js", "QUADRA"),
            ("5h 6h 7h 8h 9h Kd Ks", "STRAIGHT FLUSH"),
            ("10s Js Qs Ks As 2d 2c", "ROYAL FLUSH"),
        ];
        for (mao, nome) in casos {
            assert_eq!(nome_da_mao(f(mao)), nome, "{mao}");
        }
    }

    #[test]
    fn desempates() {
        assert!(f("Kh Kd Ah 7c 2s 3d 9s") > f("Kh Kd Qh 7c 2s 3d 9s"), "kicker do par");
        assert!(f("6h 2d 3s 4c 5h Kd Qs") > f("Ah 2d 3s 4c 5h Kd Qs"), "a roda (A-5) é a menor sequência");
        assert!(f("Ah Ad Ks Kc 2h 3d 4s") > f("Ah Ad Qs Qc Jh Jd 4s"), "dois pares: o segundo par decide");
        assert_eq!(f("Ah Kd 9s 7c 2h 3d 4s"), f("Ad Kh 9c 7s 2d 3c 4h"), "mesma mão, outro naipe");
        // O flush conta as cinco cartas.
        assert!(f("Ah 9h 7h 5h 3h 2d 2c") > f("Ah 9h 7h 5h 2h 3d 3c"));
        // Das sete, as cinco melhores: a dupla 2 fica de fora do full house de A sobre K.
        let (_, cinco) = melhor(&cs("Ah Ad As Kc Kh 2d 2s"));
        assert!(!cinco.iter().any(|c| c.valor == 2));
    }

    #[test]
    fn potes_paralelos_com_tres_all_ins() {
        // Postos 100, 300, 600, 600; o mais forte é o de 100, depois o de 300.
        let totais = [100, 300, 600, 600];
        let vivos = [true; 4];
        let forcas = [40, 30, 20, 10];
        let p = dividir(totais, vivos, forcas, 0);
        assert_eq!(p, [400, 600, 600, 0]);
        assert_eq!(p.iter().sum::<u32>(), totais.iter().sum::<u32>());
        // Quem desistiu só alimenta; empate divide e a ficha que sobra vai à esquerda do dealer.
        let p = dividir([50, 205, 205, 0], [false, true, true, false], [0, 9, 9, 0], 0);
        assert_eq!(p, [0, 230, 230, 0]);
        let p = dividir([0, 101, 100, 0], [false, true, true, false], [0, 9, 9, 0], 1);
        assert_eq!(p, [0, 101, 100, 0]);
    }

    #[test]
    fn aumento_minimo() {
        let mut p = Poker::novo(3);
        // Pré-flop: o mínimo é dobrar o big blind.
        let i = p.vez.expect("alguém joga");
        assert_eq!(p.min_para(), 40);
        p.aplicar(i, Acao::Aumentar(25));
        assert_eq!(p.maior, 40, "abaixo do mínimo sobe para o mínimo");
        p.avancar();
        let k = p.vez.expect("o próximo joga");
        p.aplicar(k, Acao::Aumentar(100));
        assert_eq!((p.maior, p.min_aumento), (100, 60));
        assert_eq!(p.min_para(), 160);
    }

    #[test]
    fn partidas_inteiras_conservam_as_fichas() {
        for s in 1..6 {
            let mut p = Poker::novo(s);
            for _ in 0..40 {
                jogar_ate(&mut p, |p| p.fim, 500);
                let soma: u32 = p.jogadores.iter().map(|j| j.fichas + j.aposta).sum::<u32>() + p.pote;
                assert_eq!(soma, 4000, "semente {s}, mão {}", p.maos);
                if p.acabou_o_jogo() {
                    break;
                }
                p.nova_mao();
            }
        }
    }

    #[test]
    fn bot_paga_com_mao_forte_e_larga_lixo_caro() {
        let base = Contexto { mao: [C { naipe: 0, valor: 14 }, C { naipe: 1, valor: 14 }], mesa: cs("Ad 7c 2s"), oponentes: 1, pote: 100, falta: 50, fichas: 900, maior: 50, min_para: 100, estilo: Estilo { solto: 0.5, agressivo: 0.0 } };
        assert_ne!(decidir(&base, 5), Acao::Desistir);
        let lixo = Contexto { mao: [C { naipe: 0, valor: 2 }, C { naipe: 1, valor: 7 }], mesa: cs("Ad Kc Qs Jd 9h"), falta: 900, fichas: 900, ..base };
        assert_eq!(decidir(&lixo, 5), Acao::Desistir);
    }
}


