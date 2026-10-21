//! Buraco (você contra o bot) com o baralho pixelado do pack: dois baralhos sem coringa, os 2 são
//! curingas, jogos só de sequência do mesmo naipe, canastra com 7 cartas, morto e batida.
//! Vence a partida quem chega a 1000 pontos.
//! `cargo run --release --example buraco`; `-- --capturar x.png` só gera o PNG.
//! shortcut: a mão acaba quando o monte acaba (o morto que sobrou não vira monte); entra se o pack pedir.
use pelicano::{column, row};
use pelicano::*;
use std::collections::HashMap;
use std::time::Duration;

use crate::cartas::*;
use crate::estilo::*;

const CW: f32 = 80.0;
const CH: f32 = 112.0;
/// Quanto aparece de cada carta num jogo baixado.
const PASSO_JOGO: f32 = 20.0;
const META: i32 = 1000;
const NOMES_NAIPES: [&str; 4] = ["COPAS", "ESPADAS", "OUROS", "PAUS"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Carta {
    naipe: u8,
    valor: u8,
    /// 0..104: os dois baralhos têm cartas iguais, o id separa.
    id: u8,
}

fn pontos(c: Carta) -> i32 {
    match c.valor {
        1 => 15,
        2 => 10,
        3..=7 => 5,
        _ => 10,
    }
}

fn xorshift(mut s: u64) -> u64 {
    s ^= s << 13;
    s ^= s >> 7;
    s ^ (s << 17)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Jogo {
    /// Em ordem, com o curinga no lugar que ele ocupa.
    cartas: Vec<Carta>,
    suja: bool,
}

impl Jogo {
    fn canastra(&self) -> bool {
        self.cartas.len() >= 7
    }
    fn bonus(&self) -> i32 {
        match (self.canastra(), self.suja) {
            (false, _) => 0,
            (true, false) => 200,
            (true, true) => 100,
        }
    }
}

/// Arruma `cartas` numa sequência do mesmo naipe (3 ou mais, ás baixo ou alto, no máximo um
/// curinga). Um 2 do naipe no lugar dele conta como carta natural.
fn montar(cartas: &[Carta]) -> Option<Jogo> {
    if cartas.len() < 3 {
        return None;
    }
    let naturais: Vec<Carta> = cartas.iter().filter(|c| c.valor != 2).copied().collect();
    let naipe = naturais.first()?.naipe;
    if naturais.iter().any(|c| c.naipe != naipe) {
        return None;
    }
    let dois: Vec<Carta> = cartas.iter().filter(|c| c.valor == 2).copied().collect();
    // Primeiro com um 2 do naipe como natural (jogo limpo), depois todos os 2 como curinga.
    let opcoes = dois.iter().position(|c| c.naipe == naipe).map(Some).into_iter().chain([None]);
    for natural in opcoes {
        let (nat, cur): (Vec<_>, Vec<_>) = dois.iter().enumerate().partition(|(i, _)| Some(*i) == natural);
        let cur: Vec<Carta> = cur.into_iter().map(|(_, c)| *c).collect();
        if cur.len() > 1 {
            continue;
        }
        let nat: Vec<Carta> = naturais.iter().copied().chain(nat.into_iter().map(|(_, c)| *c)).collect();
        for as_alto in [false, true] {
            let mut v: Vec<(i32, Carta)> = nat.iter().map(|c| (if c.valor == 1 && as_alto { 14 } else { c.valor as i32 }, *c)).collect();
            v.sort_by_key(|x| x.0);
            if v.windows(2).any(|w| w[0].0 == w[1].0) {
                continue;
            }
            let (min, max) = (v[0].0, v[v.len() - 1].0);
            let buracos = (max - min + 1) as usize - v.len();
            if buracos > cur.len() {
                continue;
            }
            let mut seq = Vec::new();
            let mut k = 0;
            for p in min..=max {
                if v[k].0 == p {
                    seq.push(v[k].1);
                    k += 1;
                } else {
                    seq.push(cur[0]);
                }
            }
            // Curinga sem buraco para tapar vai na ponta.
            if let (0, Some(&w)) = (buracos, cur.first()) {
                if max < 14 {
                    seq.push(w);
                } else if min > 1 {
                    seq.insert(0, w);
                } else {
                    continue;
                }
            }
            return Some(Jogo { cartas: seq, suja: !cur.is_empty() });
        }
    }
    None
}

/// Por que `cartas` não formam uma sequência, dito ao jogador: naipes, repetida ou a carta que falta.
fn por_que_nao(cartas: &[Carta]) -> String {
    if cartas.len() < 3 {
        return "UM JOGO PRECISA DE 3 CARTAS OU MAIS".into();
    }
    let naturais: Vec<Carta> = cartas.iter().filter(|c| c.valor != 2).copied().collect();
    let Some(naipe) = naturais.first().map(|c| c.naipe) else { return "SÓ CURINGAS NÃO FORMAM JOGO".into() };
    if naturais.iter().any(|c| c.naipe != naipe) {
        return "AS CARTAS PRECISAM SER DO MESMO NAIPE".into();
    }
    let dois = cartas.iter().filter(|c| c.valor == 2).count();
    let dois_do_naipe = cartas.iter().any(|c| c.valor == 2 && c.naipe == naipe);
    // Cada leitura possível (o 2 do naipe natural ou curinga, ás baixo ou alto): as que faltam e os curingas.
    let mut leituras = Vec::new();
    for natural in [true, false].into_iter().filter(|n| !*n || dois_do_naipe) {
        for as_alto in [false, true] {
            let mut v: Vec<i32> = naturais.iter().map(|c| if c.valor == 1 && as_alto { 14 } else { c.valor as i32 }).chain(natural.then_some(2)).collect();
            v.sort();
            if v.windows(2).any(|w| w[0] == w[1]) {
                continue;
            }
            let faltam: Vec<i32> = (v[0]..=v[v.len() - 1]).filter(|x| !v.contains(x)).collect();
            leituras.push((faltam, dois - natural as usize));
        }
    }
    let Some((faltam, curingas)) = leituras.into_iter().min_by_key(|(f, c)| (f.len() as i32 - *c as i32, *c)) else { return "TEM CARTA REPETIDA".into() };
    if curingas > 1 {
        return "SÓ CABE UM CURINGA POR JOGO".into();
    }
    let nome = |v: i32| match v {
        1 | 14 => "ÁS".to_string(),
        11 => "VALETE".to_string(),
        12 => "DAMA".to_string(),
        13 => "REI".to_string(),
        n => n.to_string(),
    };
    let nomes: Vec<String> = faltam.iter().map(|&v| nome(v)).collect();
    let lista = match nomes.split_last() {
        Some((ultimo, antes)) if !antes.is_empty() => format!("{} E {ultimo}", antes.join(", ")),
        _ => nomes.join(""),
    };
    let naipe = NOMES_NAIPES[naipe as usize];
    match (faltam.len(), curingas) {
        (1, 0) => format!("FALTA O {lista} DE {naipe}"),
        (_, 0) => format!("FALTAM O {lista} DE {naipe}"),
        _ => format!("FALTAM O {lista} DE {naipe}: O CURINGA SÓ TAPA UM"),
    }
}

/// Uma dupla (ou um jogador, no jogo de dois): os jogos na mesa e o morto são dela.
#[derive(Debug, Clone, Default)]
pub(crate) struct Lado {
    jogos: Vec<Jogo>,
    pegou_morto: bool,
    bateu: bool,
}

impl Lado {
    fn tem_canastra(&self) -> bool {
        self.jogos.iter().any(Jogo::canastra)
    }
    /// Os pontos da mão que acabou; `na_mao` são as cartas que sobraram com os jogadores da dupla.
    fn pontos(&self, na_mao: i32) -> i32 {
        let jogos: i32 = self.jogos.iter().map(|j| j.cartas.iter().map(|&c| pontos(c)).sum::<i32>() + j.bonus()).sum();
        jogos - na_mao - if self.pegou_morto { 0 } else { 100 } + if self.bateu { 100 } else { 0 }
    }
}

fn ordenar(mao: &mut [Carta]) {
    mao.sort_by_key(|c| (c.naipe, if c.valor == 1 { 14 } else { c.valor }, c.id));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fase {
    Comprar,
    Jogar,
}

/// Uma mão do jogo, com 2 ou 4 jogadores: 0 é você; a dupla de cada um é `jogador % 2`.
#[derive(Debug, Clone)]
pub(crate) struct Mao {
    maos: Vec<Vec<Carta>>,
    lados: [Lado; 2],
    monte: Vec<Carta>,
    lixo: Vec<Carta>,
    mortos: Vec<Vec<Carta>>,
    vez: usize,
    fase: Fase,
    acabou: bool,
}

impl Mao {
    fn nova(semente: u64, comeca: usize, jogadores: usize) -> Mao {
        let mut cartas: Vec<Carta> = (0..104u8).map(|id| Carta { naipe: (id % 52) / 13, valor: id % 13 + 1, id }).collect();
        let mut s = semente | 1;
        for i in (1..cartas.len()).rev() {
            s = xorshift(s);
            cartas.swap(i, (s % (i as u64 + 1)) as usize);
        }
        let mut tira = |n: usize| cartas.split_off(cartas.len() - n);
        let mut maos: Vec<Vec<Carta>> = (0..jogadores).map(|_| tira(11)).collect();
        let mortos = vec![tira(11), tira(11)];
        let lixo = tira(1);
        for m in &mut maos {
            ordenar(m);
        }
        Mao { maos, lados: Default::default(), monte: cartas, lixo, mortos, vez: comeca, fase: Fase::Comprar, acabou: false }
    }

    fn mao(&mut self) -> &mut Vec<Carta> {
        &mut self.maos[self.vez]
    }

    fn lado(&mut self) -> &mut Lado {
        &mut self.lados[self.vez % 2]
    }

    /// As cartas que sobraram na mão dos jogadores da dupla `d`, em pontos.
    fn na_mao(&self, d: usize) -> i32 {
        self.maos.iter().skip(d).step_by(2).flatten().map(|&c| pontos(c)).sum()
    }

    fn comprar(&mut self) -> Result<(), String> {
        if self.fase != Fase::Comprar {
            return Err("VOCÊ JÁ COMPROU".to_string());
        }
        let c = self.monte.pop().ok_or_else(|| "O MONTE ACABOU".to_string())?;
        self.mao().push(c);
        ordenar(self.mao());
        self.fase = Fase::Jogar;
        Ok(())
    }

    fn pegar_lixo(&mut self) -> Result<(), String> {
        if self.fase != Fase::Comprar {
            return Err("VOCÊ JÁ COMPROU".to_string());
        }
        if self.lixo.is_empty() {
            return Err("O LIXO ESTÁ VAZIO".to_string());
        }
        let lixo = std::mem::take(&mut self.lixo);
        self.mao().extend(lixo);
        ordenar(self.mao());
        self.fase = Fase::Jogar;
        Ok(())
    }

    /// Tira da mão as cartas `ids` e confere se o que sobra pode ficar: depois do morto, só se
    /// esvazia a mão (ou fica com uma carta para descartar) tendo canastra.
    fn tirar(&self, ids: &[u8]) -> Result<(Vec<Carta>, Vec<Carta>), String> {
        if self.fase != Fase::Jogar {
            return Err("COMPRE PRIMEIRO".to_string());
        }
        let (tiradas, resto): (Vec<Carta>, Vec<Carta>) = self.maos[self.vez].iter().partition(|c| ids.contains(&c.id));
        if tiradas.len() != ids.len() || ids.is_empty() {
            return Err("ESCOLHA CARTAS DA SUA MÃO".to_string());
        }
        Ok((tiradas, resto))
    }

    fn pode_ficar_com(&self, resto: usize, jogos: &[Jogo]) -> Result<(), String> {
        let lado = &self.lados[self.vez % 2];
        if lado.pegou_morto && resto < 2 && !jogos.iter().any(Jogo::canastra) {
            return Err("PRECISA DE CANASTRA PARA BATER".to_string());
        }
        Ok(())
    }

    fn baixar(&mut self, ids: &[u8]) -> Result<(), String> {
        let (tiradas, resto) = self.tirar(ids)?;
        let jogo = montar(&tiradas).ok_or_else(|| por_que_nao(&tiradas))?;
        let jogos: Vec<Jogo> = self.lados[self.vez % 2].jogos.iter().cloned().chain([jogo.clone()]).collect();
        self.pode_ficar_com(resto.len(), &jogos)?;
        *self.mao() = resto;
        self.lado().jogos.push(jogo);
        self.depois_de_baixar();
        Ok(())
    }

    fn adicionar(&mut self, j: usize, ids: &[u8]) -> Result<(), String> {
        let (tiradas, resto) = self.tirar(ids)?;
        let atual = self.lados[self.vez % 2].jogos.get(j).ok_or_else(|| "ESSE JOGO NÃO É SEU".to_string())?;
        let juntas: Vec<Carta> = atual.cartas.iter().copied().chain(tiradas).collect();
        let novo = montar(&juntas).ok_or_else(|| por_que_nao(&juntas))?;
        let mut jogos = self.lados[self.vez % 2].jogos.clone();
        jogos[j] = novo.clone();
        self.pode_ficar_com(resto.len(), &jogos)?;
        *self.mao() = resto;
        self.lado().jogos[j] = novo;
        self.depois_de_baixar();
        Ok(())
    }

    /// Mão vazia no meio da vez: o primeiro da dupla a esvaziar pega o morto; depois dele, é batida.
    fn depois_de_baixar(&mut self) {
        if !self.maos[self.vez].is_empty() {
            return;
        }
        if self.lado().pegou_morto {
            self.lado().bateu = true;
            self.acabou = true;
        } else {
            self.pegar_morto();
        }
    }

    fn pegar_morto(&mut self) {
        if let Some(mut m) = self.mortos.pop() {
            ordenar(&mut m);
            *self.mao() = m;
        }
        self.lado().pegou_morto = true;
    }

    fn descartar(&mut self, id: u8) -> Result<(), String> {
        let (tiradas, resto) = self.tirar(&[id])?;
        if resto.is_empty() && self.lados[self.vez % 2].pegou_morto && !self.lados[self.vez % 2].tem_canastra() {
            return Err("PRECISA DE CANASTRA PARA BATER".to_string());
        }
        *self.mao() = resto;
        self.lixo.extend(tiradas);
        if self.maos[self.vez].is_empty() {
            if self.lado().pegou_morto {
                self.lado().bateu = true;
                self.acabou = true;
                return Ok(());
            }
            self.pegar_morto();
        }
        self.vez = (self.vez + 1) % self.maos.len();
        self.fase = Fase::Comprar;
        if self.monte.is_empty() {
            self.acabou = true;
        }
        Ok(())
    }

    /// Onde cada carta está agora (para animar as que mudaram de lugar).
    fn zonas(&self) -> HashMap<u8, Zona> {
        let mut z = HashMap::new();
        for (i, m) in self.maos.iter().enumerate() {
            z.extend(m.iter().map(|c| (c.id, Zona::Mao(i))));
        }
        for (d, l) in self.lados.iter().enumerate() {
            z.extend(l.jogos.iter().flat_map(|j| &j.cartas).map(|c| (c.id, Zona::Jogo(d))));
        }
        z.extend(self.lixo.iter().map(|c| (c.id, Zona::Lixo)));
        z
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zona {
    /// Na mão do jogador.
    Mao(usize),
    /// Num jogo da dupla.
    Jogo(usize),
    Lixo,
}

// O bot: heurística simples, joga uma ação por vez para dar para acompanhar.

/// Quantas cartas do mesmo naipe a mão tem perto de `c` (até 2 de distância).
fn vizinhos(mao: &[Carta], c: Carta) -> usize {
    mao.iter().filter(|x| x.id != c.id && x.naipe == c.naipe && x.valor != 2 && (x.valor as i32 - c.valor as i32).abs() <= 2 && x.valor != c.valor).count()
}

/// A maior sequência natural (sem curinga) que dá para baixar com a mão.
fn melhor_sequencia(mao: &[Carta]) -> Option<Vec<u8>> {
    let mut melhor: Option<Vec<u8>> = None;
    for naipe in 0..4 {
        let do_naipe: Vec<Carta> = mao.iter().filter(|c| c.naipe == naipe).copied().collect();
        for as_alto in [false, true] {
            let valor = |c: &Carta| if c.valor == 1 && as_alto { 14 } else { c.valor };
            let mut vals: Vec<(u8, Carta)> = do_naipe.iter().map(|c| (valor(c), *c)).collect();
            vals.sort_by_key(|x| x.0);
            vals.dedup_by_key(|x| x.0);
            let mut i = 0;
            while i < vals.len() {
                let mut j = i;
                while j + 1 < vals.len() && vals[j + 1].0 == vals[j].0 + 1 {
                    j += 1;
                }
                let run: Vec<Carta> = vals[i..=j].iter().map(|x| x.1).collect();
                if run.len() >= 3 && melhor.as_ref().is_none_or(|m| run.len() > m.len()) && montar(&run).is_some_and(|m| !m.suja) {
                    melhor = Some(run.iter().map(|c| c.id).collect());
                }
                i = j + 1;
            }
        }
    }
    melhor
}

fn lixo_serve(m: &Mao, jogador: usize) -> bool {
    let Some(&topo) = m.lixo.last() else { return false };
    let mao = &m.maos[jogador];
    let num_jogo = m.lados[jogador % 2].jogos.iter().any(|j| montar(&j.cartas.iter().copied().chain([topo]).collect::<Vec<_>>()).is_some_and(|n| n.suja == j.suja));
    let com_a_mao = mao.iter().enumerate().any(|(i, &a)| mao[i + 1..].iter().any(|&b| montar(&[a, b, topo]).is_some_and(|j| !j.suja)));
    num_jogo || com_a_mao
}

/// Uma ação do lado da vez; devolve o que ele fez, para mostrar na mesa.
fn passo_bot(m: &mut Mao) -> String {
    let eu = m.vez;
    if m.fase == Fase::Comprar {
        if lixo_serve(m, eu) {
            let n = m.lixo.len();
            if m.pegar_lixo().is_ok() {
                return format!("PEGOU O LIXO ({n} {})", if n == 1 { "CARTA" } else { "CARTAS" });
            }
        }
        return match m.comprar() {
            Ok(()) => "COMPROU DO MONTE".into(),
            Err(_) => {
                m.acabou = true;
                "O MONTE ACABOU".into()
            }
        };
    }
    let mao = m.maos[eu].clone();
    // Completar um jogo com carta natural, sem sujar o que está limpo.
    for c in mao.iter().filter(|c| c.valor != 2) {
        for (j, jogo) in m.lados[eu % 2].jogos.clone().iter().enumerate() {
            if montar(&jogo.cartas.iter().copied().chain([*c]).collect::<Vec<_>>()).is_some_and(|n| n.suja == jogo.suja) && m.adicionar(j, &[c.id]).is_ok() {
                return "COMPLETOU UM JOGO".into();
            }
        }
    }
    if let Some(ids) = melhor_sequencia(&mao) {
        if m.baixar(&ids).is_ok() {
            return format!("BAIXOU UMA SEQUÊNCIA DE {}", NOMES_NAIPES[mao.iter().find(|c| c.id == ids[0]).map_or(0, |c| c.naipe as usize)]);
        }
    }
    // O curinga só fecha canastra ou completa um jogo de três.
    if let Some(w) = mao.iter().find(|c| c.valor == 2) {
        for (j, jogo) in m.lados[eu % 2].jogos.clone().iter().enumerate() {
            if jogo.cartas.len() == 6 && !jogo.suja && m.adicionar(j, &[w.id]).is_ok() {
                return "FECHOU UMA CANASTRA COM CURINGA".into();
            }
        }
        if mao.len() > 4 {
            for (i, &a) in mao.iter().enumerate().filter(|(_, c)| c.valor != 2) {
                for &b in mao[i + 1..].iter().filter(|c| c.valor != 2 && c.naipe == a.naipe) {
                    if montar(&[a, b, *w]).is_some() && m.baixar(&[a.id, b.id, w.id]).is_ok() {
                        return "BAIXOU UM JOGO COM CURINGA".into();
                    }
                }
            }
        }
    }
    // Descarta a carta mais solta, sem dar de presente o que encaixa nos jogos do outro.
    let outro = &m.lados[1 - eu % 2].jogos;
    let encaixa = |c: Carta| outro.iter().any(|j| montar(&j.cartas.iter().copied().chain([c]).collect::<Vec<_>>()).is_some());
    let nota = |c: Carta| vizinhos(&mao, c) as i32 * 10 + if c.valor == 2 { 60 } else { 0 } + if encaixa(c) { 35 } else { 0 } - pontos(c) / 5;
    let mut ordem = mao.clone();
    ordem.sort_by_key(|&c| nota(c));
    for c in ordem {
        if m.descartar(c.id).is_ok() {
            return "DESCARTOU".into();
        }
    }
    // Sem descarte possível (não acontece nas regras acima): encerra a mão para não travar.
    m.acabou = true;
    "PAROU".into()
}

#[derive(Debug, Clone)]
pub(crate) enum Msg {
    Monte,
    Lixo,
    Selecionar(u8),
    /// Apertou uma carta da mão: começa a arrastar (ou é só um clique, se soltar sem mexer).
    Pegar(u8),
    Mouse(Point),
    Soltar,
    /// O mouse entrou (`true`) ou saiu de um lugar onde dá para soltar cartas.
    Sobre(Alvo, bool),
    /// Começa uma partida nova com 2 ou 4 jogadores.
    Jogadores(usize),
    Baixar,
    Adicionar(usize),
    Descartar,
    Bot(u32),
    Assentou(u32),
    ProximaMao,
    NovaPartida,
}

/// Onde as cartas arrastadas podem cair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Alvo {
    Mesa,
    Jogo(usize),
    Lixo,
}

struct Arraste {
    ids: Vec<u8>,
    inicio: Point,
    mouse: Point,
}

impl Arraste {
    /// Quase sem mexer é um clique, não um arraste.
    fn movido(&self) -> bool {
        (self.mouse.x - self.inicio.x).abs() + (self.mouse.y - self.inicio.y).abs() >= 6.0
    }
}

pub(crate) struct Buraco {
    mouse: Point,
    arraste: Option<Arraste>,
    /// Debaixo do mouse: a área dos seus jogos, um jogo seu, o lixo.
    sobre: (bool, Option<usize>, bool),
    mao: Mao,
    placar: [i32; 2],
    /// Os pontos da última mão, para o resumo entre mãos.
    resultado: Option<[i32; 2]>,
    selecionadas: Vec<u8>,
    aviso: Option<String>,
    relato: String,
    /// Cartas que acabaram de chegar a um lugar e de onde vêm (deslocamento vertical da entrada).
    novas: Vec<(u8, f32)>,
    animacao: u32,
    /// Muda a cada mão: a resposta atrasada do bot da mão anterior é ignorada.
    rodada: u32,
    maos: u32,
    /// 2 (você contra o bot) ou 4 (em duplas: você e a BIA contra o TONHO e a DUDA).
    jogadores: usize,
}

fn nomes(jogadores: usize) -> &'static [&'static str] {
    if jogadores == 4 { &["VOCÊ", "TONHO", "BIA", "DUDA"] } else { &["VOCÊ", "BOT"] }
}

fn semente() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64)
}

fn esperar(ms: u64, m: Msg) -> Task<Msg> {
    Task::perform(async move { std::thread::sleep(Duration::from_millis(ms)) }, move |_| m.clone())
}

/// A carta com a sombra dura; `nova` faz ela entrar deslizando, `subida` é a da mão escolhida.
fn carta<'a>(c: Carta, nova: Option<f32>, subida: f32) -> Element<'a, Msg> {
    let face = container(svg(svg_carta(c.naipe, c.valor)).width(CW).height(CH)).style(|_| Style::default().background(Color::WHITE).border(Border::rounded(5.0)).shadow(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(PX, PX), blur: 0.0 }));
    let a = animate(face).offset(0.0, subida).transition(Transition::new(220).easing(Easing::EaseOut));
    match nova {
        // Uma camada a mais muda a chave do widget: a animação de entrada roda de novo.
        Some(dy) => container(a.enter(Keyframe::default().offset(0.0, dy).opacity(0.0))).into(),
        None => a.into(),
    }
}

fn verso<'a, M: 'a>() -> Element<'a, M> {
    container(svg(svg_verso()).width(CW).height(CH)).style(|_| Style::default().background(Color::WHITE).border(Border::rounded(5.0)).shadow(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(PX, PX), blur: 0.0 })).into()
}

fn no_lugar<'a, M: 'a>(el: impl Into<Element<'a, M>>, x: f32, y: f32) -> Element<'a, M> {
    container(el).padding(Padding { top: y, right: 0.0, bottom: 0.0, left: x }).into()
}

/// Cartas em leque, uma por cima da outra, em `largura` no máximo.
fn leque<'a, M: 'a>(cartas: Vec<Element<'a, M>>, passo: f32, largura: f32, altura: f32) -> Element<'a, M> {
    let n = cartas.len();
    let passo = if n > 1 { passo.min((largura - CW) / (n - 1) as f32) } else { passo };
    let w = CW + passo * n.saturating_sub(1) as f32;
    let filhos = std::iter::once(Space::new(w, altura).into()).chain(cartas.into_iter().enumerate().map(|(i, c)| no_lugar(c, (i as f32 * passo).round(), altura - CH)));
    stack(filhos).into()
}

impl Buraco {
    fn novo(placar: [i32; 2], maos: u32, rodada: u32, jogadores: usize) -> Self {
        let comeca = maos as usize % jogadores;
        let relato = format!("{} COMEÇA", nomes(jogadores)[comeca]);
        Buraco { mouse: Point::ORIGIN, arraste: None, sobre: (false, None, false), mao: Mao::nova(semente(), comeca, jogadores), placar, resultado: None, selecionadas: Vec::new(), aviso: None, relato, novas: Vec::new(), animacao: 0, rodada, maos, jogadores }
    }

    fn quatro(&self) -> bool {
        self.jogadores == 4
    }

    fn nome(&self, jogador: usize) -> &'static str {
        nomes(self.jogadores)[jogador]
    }

    /// Como chamar cada dupla no placar.
    fn dupla(&self, d: usize) -> &'static str {
        if self.quatro() { ["NÓS", "ELES"][d] } else { ["VOCÊ", "BOT"][d] }
    }

    /// Faz `f` na mão, marca para animar as cartas que mudaram de lugar e cuida do fim da mão.
    fn agir(&mut self, f: impl FnOnce(&mut Mao) -> Result<(), String>) -> Task<Msg> {
        let antes = self.mao.zonas();
        if let Err(e) = f(&mut self.mao) {
            self.aviso = Some(e);
            return Task::none();
        }
        self.aviso = None;
        self.marcar(antes)
    }

    fn marcar(&mut self, antes: HashMap<u8, Zona>) -> Task<Msg> {
        let depois = self.mao.zonas();
        self.novas = depois
            .iter()
            .filter(|(id, z)| antes.get(id) != Some(z))
            .map(|(&id, &z)| {
                let dy = match (antes.get(&id), z) {
                    (_, Zona::Mao(_)) => -60.0,
                    (_, Zona::Jogo(0)) => 50.0,
                    (_, Zona::Jogo(_)) => -50.0,
                    (Some(Zona::Mao(0)), Zona::Lixo) => 70.0,
                    (_, Zona::Lixo) => -70.0,
                };
                (id, dy)
            })
            .collect();
        self.selecionadas.retain(|id| self.mao.maos[0].iter().any(|c| c.id == *id));
        self.animacao += 1;
        let assentar = esperar(450, Msg::Assentou(self.animacao));
        if self.mao.acabou {
            self.fechar_mao();
            return assentar;
        }
        if self.mao.vez != 0 {
            return Task::batch([assentar, esperar(650, Msg::Bot(self.rodada))]);
        }
        assentar
    }

    fn fechar_mao(&mut self) {
        let r = [self.mao.lados[0].pontos(self.mao.na_mao(0)), self.mao.lados[1].pontos(self.mao.na_mao(1))];
        self.placar[0] += r[0];
        self.placar[1] += r[1];
        self.resultado = Some(r);
        self.rodada += 1;
    }

    fn nova(&self, id: u8) -> Option<f32> {
        self.novas.iter().find(|(x, _)| *x == id).map(|(_, dy)| *dy)
    }

    fn arrastando(&self, id: u8) -> bool {
        self.arraste.as_ref().is_some_and(|a| a.movido() && a.ids.contains(&id))
    }

    /// O alvo aceso: onde as cartas cairiam se soltasse agora.
    fn alvo(&self) -> Option<Alvo> {
        self.arraste.as_ref().filter(|a| a.movido())?;
        match self.sobre {
            (_, _, true) => Some(Alvo::Lixo),
            (_, Some(j), _) => Some(Alvo::Jogo(j)),
            (true, _, _) => Some(Alvo::Mesa),
            _ => None,
        }
    }

    fn vencedor(&self) -> Option<usize> {
        (self.resultado.is_some() && self.placar.iter().any(|&p| p >= META)).then(|| if self.placar[0] >= self.placar[1] { 0 } else { 1 })
    }

    fn jogos(&self, lado: usize) -> Element<'_, Msg> {
        let jogos = self.mao.lados[lado].jogos.iter().enumerate().map(|(j, jogo)| {
            let cartas = jogo.cartas.iter().map(|&c| carta(c, self.nova(c.id), 0.0)).collect();
            let largura = CW + PASSO_JOGO * (jogo.cartas.len() - 1) as f32;
            let fileira = leque(cartas, PASSO_JOGO, largura, CH);
            let rotulo = if jogo.canastra() { texto(if jogo.suja { "CANASTRA SUJA" } else { "CANASTRA LIMPA" }, 12.0, OURO) } else { texto(format!("{} CARTAS", jogo.cartas.len()), 12.0, MENTA.alpha(0.7)) };
            let canastra = jogo.canastra();
            let aceso = lado == 0 && self.alvo() == Some(Alvo::Jogo(j));
            let caixa = container(column![fileira, rotulo].spacing(6)).padding(6.0).style(move |_| match (aceso, canastra) {
                (true, _) => Style::default().background(OURO.alpha(0.25)).border(Border::rounded(0.0).color(OURO, PX)),
                (_, true) => Style::default().background(OURO.alpha(0.12)).border(Border::rounded(0.0).color(OURO, PX)),
                _ => Style::default(),
            });
            if lado == 0 {
                mouse_area(caixa).on_press(Msg::Adicionar(j)).on_enter(Msg::Sobre(Alvo::Jogo(j), true)).on_exit(Msg::Sobre(Alvo::Jogo(j), false)).cursor_icon(CursorIcon::Pointer).into()
            } else {
                caixa.into()
            }
        });
        let vazio = self.mao.lados[lado].jogos.is_empty();
        let conteudo: Element<'_, Msg> = if vazio {
            let vazio = match (lado, self.quatro()) {
                (0, false) => "SEUS JOGOS APARECEM AQUI",
                (0, true) => "OS JOGOS DA SUA DUPLA APARECEM AQUI",
                (_, false) => "JOGOS DO BOT",
                (_, true) => "JOGOS DOS ADVERSÁRIOS",
            };
            container(texto(vazio, 14.0, MENTA.alpha(0.35))).center().into()
        } else {
            scrollable(row(jogos).spacing(14).wrap()).height(Length::FILL).into()
        };
        let aceso = lado == 0 && self.alvo() == Some(Alvo::Mesa);
        let area = container(conteudo).width(Length::FILL).height(Length::FILL).padding(8.0).style(move |_| {
            if aceso { Style::default().background(OURO.alpha(0.12)).border(Border::rounded(0.0).color(OURO, PX)) } else { Style::default().background(FELTRO_ESCURO.alpha(0.5)).border(Border::rounded(0.0).color(FELTRO_CLARO.alpha(0.5), PX / 2.0)) }
        });
        if lado == 0 {
            mouse_area(area).on_enter(Msg::Sobre(Alvo::Mesa, true)).on_exit(Msg::Sobre(Alvo::Mesa, false)).into()
        } else {
            area.into()
        }
    }

    fn centro(&self) -> Element<'_, Msg> {
        let m = &self.mao;
        let sua_compra = m.vez == 0 && m.fase == Fase::Comprar && !m.acabou;
        let rotulo = |s: String| texto(s, 12.0, MENTA);
        // O monte com espessura e a contagem.
        let pilha = (0..m.monte.len().min(4)).map(|i| no_lugar(verso::<Msg>(), 6.0 - i as f32 * 2.0, 6.0 - i as f32 * 2.0)).collect::<Vec<_>>();
        let monte = column![mouse_area(stack(std::iter::once(Space::new(CW + 6.0, CH + 6.0).into()).chain(pilha))).on_press(Msg::Monte).cursor_icon(CursorIcon::Pointer), rotulo(format!("MONTE {}", m.monte.len()))].spacing(6).align(Align::Center);
        let mostrar = m.lixo.len().saturating_sub(12);
        let cartas_lixo: Vec<Element<'_, Msg>> = m.lixo[mostrar..].iter().map(|&c| carta(c, self.nova(c.id), 0.0)).collect();
        let lixo_el: Element<'_, Msg> = if cartas_lixo.is_empty() { container(Space::new(CW, CH)).style(|_| Style::default().border(Border::rounded(5.0).color(MENTA.alpha(0.3), PX / 2.0))).into() } else { leque(cartas_lixo, 22.0, 340.0, CH) };
        let aceso = self.alvo() == Some(Alvo::Lixo);
        let lixo_el = container(lixo_el).padding(PX).style(move |_| if aceso { Style::default().background(OURO.alpha(0.25)).border(Border::rounded(0.0).color(OURO, PX)) } else { Style::default() });
        let lixo = column![mouse_area(lixo_el).on_press(Msg::Lixo).on_enter(Msg::Sobre(Alvo::Lixo, true)).on_exit(Msg::Sobre(Alvo::Lixo, false)).cursor_icon(CursorIcon::Pointer), rotulo(format!("LIXO {}", m.lixo.len()))].spacing(6).align(Align::Center);
        // Os mortos cruzados, como na mesa de verdade.
        let mortos: Vec<Element<'_, Msg>> = (0..m.mortos.len()).map(|i| if i == 0 { no_lugar(verso::<Msg>(), 16.0, 0.0) } else { no_lugar(animate(verso::<Msg>()).rotate(90.0), 16.0, 0.0) }).collect();
        let mortos = column![stack(std::iter::once(Space::new(CH, CH).into()).chain(mortos)), rotulo(format!("MORTOS {}", m.mortos.len()))].spacing(6).align(Align::Center);
        // O que está acontecendo: de quem é a vez, o que fazer, e o último lance do bot.
        let (vez, cor) = match (m.acabou, m.vez, m.fase) {
            (true, _, _) => ("FIM DA MÃO".to_string(), OURO),
            (_, 0, Fase::Comprar) => ("COMPRE DO MONTE OU PEGUE O LIXO".into(), OURO),
            (_, 0, _) => ("BAIXE, COMPLETE OU DESCARTE".into(), OURO),
            (_, v, _) => (format!("VEZ DE {}", self.nome(v)), OURO),
        };
        let vez_el: Element<'_, Msg> = if m.vez != 0 && !m.acabou {
            row![texto(vez, 18.0, cor), animate(texto("...", 18.0, cor)).opacity(0.1).repeat(Repeat::Alternate).transition(Transition::new(420))].into()
        } else if sua_compra {
            animate(texto(vez, 18.0, cor)).enter(Keyframe::default().opacity(0.55)).repeat(Repeat::Alternate).transition(Transition::new(700)).into()
        } else {
            texto(vez, 18.0, cor).into()
        };
        let aviso: Element<'_, Msg> = match &self.aviso {
            Some(a) => animate(texto(a.clone(), 14.0, ROSA)).enter(Keyframe::default().offset(6.0, 0.0)).transition(Transition::new(260).easing(Easing::EaseOut)).into(),
            None => texto(self.relato.clone(), 14.0, MENTA.alpha(0.8)).into(),
        };
        let status = container(column![vez_el, aviso].spacing(10)).width(Length::FILL).padding(16.0).style(|_| Style::default().background(FUNDO.alpha(0.55)).border(Border::rounded(0.0).color(Color::BLACK.alpha(0.4), PX)));
        row![monte, lixo, mortos, status].spacing(28).align(Align::Center).into()
    }

    fn mao_jogador(&self) -> Element<'_, Msg> {
        let cartas = self.mao.maos[0]
            .iter()
            .filter(|c| !self.arrastando(c.id))
            .map(|&c| {
                let sel = self.selecionadas.contains(&c.id);
                mouse_area(carta(c, self.nova(c.id), if sel { -18.0 } else { 0.0 })).on_press(Msg::Pegar(c.id)).cursor_icon(CursorIcon::Pointer).into()
            })
            .collect();
        leque(cartas, 52.0, 900.0, CH + 22.0)
    }

    /// Os outros jogadores, na ordem em que sentam na mesa: as costas das cartas e quem é quem.
    fn outros(&self) -> Element<'_, Msg> {
        let m = &self.mao;
        let assentos: &[usize] = if self.quatro() { &[3, 2, 1] } else { &[1] };
        let jogadores = assentos.iter().map(|&p| {
            let n = m.maos[p].len();
            let costas = leque((0..n).map(|_| verso()).collect(), 22.0, 200.0, CH);
            let papel = if p % 2 == 0 { "PARCEIRA" } else { "ADVERSÁRIO" };
            let info = column![texto(self.nome(p), 16.0, OURO), texto(format!("{n} CARTAS"), 12.0, MENTA), texto(papel, 12.0, MENTA.alpha(0.6))]
                .spacing(6)
                .push_maybe(m.lados[p % 2].pegou_morto.then(|| texto("PEGOU O MORTO", 12.0, ROSA)));
            let vez = m.vez == p && !m.acabou;
            container(row![costas, info].spacing(12).align(Align::Center)).padding(6.0).style(move |_| {
                let cor = if vez { OURO } else { Color::TRANSPARENT };
                Style::default().background(if vez { OURO.alpha(0.12) } else { Color::TRANSPARENT }).border(Border::rounded(0.0).color(cor, PX))
            })
        });
        container(row(jogadores.map(Element::from)).spacing(14).align(Align::Center)).width(Length::FILL).align_x(Align::Center).into()
    }

    fn controles(&self) -> Element<'_, Msg> {
        let m = &self.mao;
        let jogando = m.vez == 0 && m.fase == Fase::Jogar && !m.acabou;
        let n = self.selecionadas.len();
        let morto = match (m.lados[0].pegou_morto, self.quatro()) {
            (false, _) => "MORTO AINDA NA MESA",
            (true, false) => "VOCÊ PEGOU O MORTO",
            (true, true) => "SUA DUPLA PEGOU O MORTO",
        };
        column![
            botao("BAIXAR JOGO", 16.0, (jogando && n >= 3).then_some(Msg::Baixar), false),
            botao("DESCARTAR", 16.0, (jogando && n == 1).then_some(Msg::Descartar), true),
            texto(if jogando { "ARRASTE PARA A MESA, UM JOGO SEU OU O LIXO" } else { morto }, 12.0, MENTA.alpha(0.75)),
        ]
        .spacing(12)
        .width(210.0)
        .into()
    }

    fn letreiro(&self) -> Option<Element<'_, Msg>> {
        let r = self.resultado?;
        let sinal = |p: i32| if p >= 0 { format!("+{p}") } else { p.to_string() };
        let (titulo, cor, botao_el) = match self.vencedor() {
            Some(0) => (if self.quatro() { "SUA DUPLA VENCEU!" } else { "VOCÊ VENCEU!" }, OURO, botao("JOGAR DE NOVO", 18.0, Some(Msg::NovaPartida), true)),
            Some(_) => (if self.quatro() { "OS ADVERSÁRIOS VENCERAM" } else { "O BOT VENCEU" }, ROSA, botao("JOGAR DE NOVO", 18.0, Some(Msg::NovaPartida), true)),
            None => ("FIM DA MÃO", OURO, botao("PRÓXIMA MÃO", 18.0, Some(Msg::ProximaMao), true)),
        };
        let bateu = match (self.mao.lados[0].bateu, self.mao.lados[1].bateu, self.quatro()) {
            (true, _, false) => "VOCÊ BATEU",
            (true, _, true) => "SUA DUPLA BATEU",
            (_, true, false) => "O BOT BATEU",
            (_, true, true) => "OS ADVERSÁRIOS BATERAM",
            _ => "O MONTE ACABOU",
        };
        let linhas = column![
            texto(bateu, 16.0, MENTA),
            texto(format!("NESTA MÃO  ·  {} {}  ·  {} {}", self.dupla(0), sinal(r[0]), self.dupla(1), sinal(r[1])), 18.0, Color::WHITE),
            texto(format!("PLACAR  ·  {} {}  ·  {} {}  ·  META {META}", self.dupla(0), self.placar[0], self.dupla(1), self.placar[1]), 18.0, OURO),
        ]
        .spacing(10)
        .align(Align::Center);
        let cartao = container(column![texto(titulo, 40.0, cor), linhas, Space::new(0.0, 6.0), container(botao_el).width(260.0)].spacing(18).align(Align::Center)).padding(36.0).style(|_| placa().shadow(sombra_dura(8.0)));
        let entra = Transition::new(320).delay(500).easing(Easing::EaseOut);
        let cortina = container(animate(cartao).enter(Keyframe::default().scale(0.6).opacity(0.0)).transition(entra)).center().style(|_| Style::default().background(Color::BLACK.alpha(0.55)));
        Some(opaque(animate(cortina).enter(Keyframe::default().opacity(0.0)).transition(entra)).into())
    }
}

impl App for Buraco {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let minha = self.mao.vez == 0 && !self.mao.acabou;
        match msg {
            Msg::Monte if minha => return self.agir(Mao::comprar),
            Msg::Lixo if minha => return self.agir(Mao::pegar_lixo),
            Msg::Selecionar(id) => {
                if let Some(i) = self.selecionadas.iter().position(|x| *x == id) {
                    self.selecionadas.remove(i);
                } else {
                    self.selecionadas.push(id);
                }
                self.aviso = None;
            }
            Msg::Pegar(id) => {
                // Pegou uma escolhida: vão todas as escolhidas; senão, só ela. A apertada vem primeiro (o clique).
                let mut ids = vec![id];
                if self.selecionadas.contains(&id) {
                    ids.extend(self.selecionadas.iter().filter(|&&x| x != id));
                }
                self.arraste = Some(Arraste { ids, inicio: self.mouse, mouse: self.mouse });
            }
            Msg::Mouse(p) => {
                self.mouse = p;
                if let Some(a) = &mut self.arraste {
                    a.mouse = p;
                }
            }
            Msg::Sobre(alvo, dentro) => match alvo {
                Alvo::Mesa => self.sobre.0 = dentro,
                Alvo::Jogo(j) if dentro => self.sobre.1 = Some(j),
                Alvo::Jogo(j) => {
                    if self.sobre.1 == Some(j) {
                        self.sobre.1 = None;
                    }
                }
                Alvo::Lixo => self.sobre.2 = dentro,
            },
            Msg::Soltar => {
                let alvo = self.alvo();
                let Some(a) = self.arraste.take() else { return Task::none() };
                if !a.movido() {
                    return self.update(Msg::Selecionar(a.ids[0]));
                }
                if !minha {
                    return Task::none();
                }
                return match alvo {
                    Some(Alvo::Mesa) => self.agir(|m| m.baixar(&a.ids)),
                    Some(Alvo::Jogo(j)) => self.agir(|m| m.adicionar(j, &a.ids)),
                    Some(Alvo::Lixo) if a.ids.len() == 1 => self.agir(|m| m.descartar(a.ids[0])),
                    Some(Alvo::Lixo) => {
                        self.aviso = Some("DESCARTE UMA CARTA SÓ".into());
                        Task::none()
                    }
                    None => Task::none(),
                };
            }
            Msg::Baixar if minha => {
                let ids = self.selecionadas.clone();
                return self.agir(|m| m.baixar(&ids));
            }
            Msg::Adicionar(j) if minha && !self.selecionadas.is_empty() => {
                let ids = self.selecionadas.clone();
                return self.agir(|m| m.adicionar(j, &ids));
            }
            Msg::Descartar if minha && self.selecionadas.len() == 1 => {
                let id = self.selecionadas[0];
                return self.agir(|m| m.descartar(id));
            }
            Msg::Bot(r) if r == self.rodada && self.mao.vez != 0 && !self.mao.acabou => {
                let antes = self.mao.zonas();
                let quem = self.nome(self.mao.vez);
                self.relato = format!("{quem}: {}", passo_bot(&mut self.mao));
                return self.marcar(antes);
            }
            Msg::Assentou(a) if a == self.animacao => self.novas.clear(),
            Msg::ProximaMao => {
                *self = Buraco::novo(self.placar, self.maos + 1, self.rodada + 1, self.jogadores);
                if self.mao.vez != 0 {
                    return esperar(900, Msg::Bot(self.rodada));
                }
            }
            Msg::NovaPartida => *self = Buraco::novo([0, 0], 0, self.rodada + 1, self.jogadores),
            Msg::Jogadores(n) => *self = Buraco::novo([0, 0], 0, self.rodada + 1, n),
            _ => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let titulo = container(text("BURACO").font(LETREIRO).size(20.0).color(OURO)).padding([10u16, 20]).style(|_| placa());
        let placar = container(texto(format!("{} {}  ·  {} {}  ·  META {META}", self.dupla(0), self.placar[0], self.dupla(1), self.placar[1]), 16.0, OURO)).padding([10u16, 16]).style(|_| Style::default().background(PAINEL).border(Border::rounded(0.0).color(Color::BLACK, PX)).shadow(sombra_dura(PX)));
        let topo = row![Space::new(150.0, 0.0), horizontal_space(), titulo, horizontal_space(), row![container(botao(if self.quatro() { "JOGAR A 2" } else { "JOGAR EM DUPLAS" }, 12.0, Some(Msg::Jogadores(if self.quatro() { 2 } else { 4 })), false)).width(170.0), container(placar).width(Length::Shrink)].spacing(14).align(Align::Center)].align(Align::Center);
        let mesa = column![self.outros(), self.jogos(1), self.centro(), self.jogos(0), row![self.mao_jogador(), horizontal_space(), self.controles()].spacing(20).align(Align::End)].spacing(12);
        let feltro = container(mesa).padding(18.0).width(Length::FILL).height(Length::FILL).style(|_| Style::default().background(FELTRO).border(Border::rounded(0.0).color(MADEIRA_ESCURA, PX)));
        let moldura = container(feltro).padding(10.0).width(Length::FILL).height(Length::FILL).style(|_| Style::default().background(MADEIRA).border(Border::rounded(0.0).color(MADEIRA_ESCURA, PX)).shadow(sombra_dura(8.0)));
        let tela = container(column![topo, moldura].spacing(16)).padding(Padding { top: 14.0, right: 24.0, bottom: 24.0, left: 24.0 }).width(Length::FILL).height(Length::FILL).style(|_| Style::default().background(FUNDO));
        let mut camadas = vec![tela.into()];
        if let Some(l) = self.letreiro() {
            camadas.push(l);
        }
        // As cartas arrastadas seguem o mouse, por cima da mesa.
        if let Some(a) = self.arraste.as_ref().filter(|a| a.movido()) {
            let cartas = a.ids.iter().filter_map(|id| self.mao.maos[0].iter().find(|c| c.id == *id)).enumerate().map(|(k, &c)| no_lugar(carta(c, None, 0.0), a.mouse.x - CW / 2.0 + k as f32 * 22.0, a.mouse.y - CH / 2.0));
            // O stack mede pelo primeiro filho: um vazio do tamanho da tela para nenhuma carta ser cortada.
            camadas.push(stack(std::iter::once(Space::new(Length::FILL, Length::FILL).into()).chain(cartas)).into());
        }
        mouse_area(stack(camadas)).on_move(Msg::Mouse).on_release(Msg::Soltar).into()
    }
}

/// O jogo pronto para abrir (no exemplo sozinho ou no menu do pack).
pub(crate) fn iniciar() -> (Buraco, Task<Msg>) {
    (Buraco::novo([0, 0], 0, 0, 4), Task::none())
}

/// O ícone do jogo no menu: um leque de cartas com um curinga.
#[allow(dead_code)] // só o menu do pack usa
pub(crate) fn icone<'a, M: 'a>() -> Element<'a, M> {
    let carta_ = |naipe: u8, valor: u8| container(svg(svg_carta(naipe, valor)).width(56.0).height(78.0)).style(|_| Style::default().background(Color::WHITE).border(Border::rounded(4.0)).shadow(Shadow { color: Color::BLACK.alpha(0.35), offset: Point::new(3.0, 3.0), blur: 0.0 }));
    let leque = [(0u8, 5u8, -18.0, 22.0, 18.0), (0, 6, -6.0, 46.0, 10.0), (0, 7, 6.0, 70.0, 10.0), (1, 2, 18.0, 94.0, 18.0)];
    let filhos = std::iter::once(Space::new(170.0, 110.0).into()).chain(leque.into_iter().map(|(n, v, g, x, y)| no_lugar(animate(carta_(n, v)).rotate(g), x, y)));
    stack(filhos).into()
}

pub(crate) fn principal() -> Result<(), Error> {
    carregar_fontes();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        // Meio de mão: todos jogam pelo bot até as duplas terem jogos na mesa e ser sua vez.
        let n = if args.iter().any(|a| a == "--dois") { 2 } else { 4 };
        let mut b = Buraco::novo([340, 215], 1, 0, n);
        b.mao = Mao::nova(11, 0, n);
        let mut passos = 0;
        while passos < 400 && !b.mao.acabou && (b.mao.lados[0].jogos.len() < 3 || b.mao.lados[1].jogos.len() < 2 || b.mao.vez != 0 || b.mao.fase != Fase::Jogar) {
            let quem = b.nome(b.mao.vez);
            b.relato = format!("{quem}: {}", passo_bot(&mut b.mao));
            passos += 1;
        }
        b.selecionadas = b.mao.maos[0].iter().take(2).map(|c| c.id).collect();
        let png = capture(&b.view(), Size::new(1240.0, 940.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("buraco.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Buraco".into(), size: Size::new(1240.0, 940.0), min_size: Some(Size::new(1180.0, 880.0)), ..Default::default() }, iniciar)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn c(naipe: u8, valor: u8) -> Carta {
        Carta { naipe, valor, id: naipe * 13 + valor - 1 }
    }
    /// A mesma carta do segundo baralho.
    fn c2(naipe: u8, valor: u8) -> Carta {
        Carta { id: c(naipe, valor).id + 52, ..c(naipe, valor) }
    }
    fn valores(j: &Jogo) -> Vec<u8> {
        j.cartas.iter().map(|c| c.valor).collect()
    }

    #[test]
    fn aviso_diz_o_que_falta() {
        assert_eq!(por_que_nao(&[c(0, 1), c(0, 2), c(0, 3), c(0, 5)]), "FALTA O 4 DE COPAS");
        assert_eq!(por_que_nao(&[c(0, 4), c(0, 7), c(0, 9)]), "FALTAM O 5, 6 E 8 DE COPAS");
        assert_eq!(por_que_nao(&[c(0, 4), c(0, 7), c(1, 2)]), "FALTAM O 5 E 6 DE COPAS: O CURINGA SÓ TAPA UM");
        assert_eq!(por_que_nao(&[c(0, 4), c(1, 5), c(0, 6)]), "AS CARTAS PRECISAM SER DO MESMO NAIPE");
        assert_eq!(por_que_nao(&[c(0, 4), c(0, 5)]), "UM JOGO PRECISA DE 3 CARTAS OU MAIS");
    }

    #[test]
    fn sequencias() {
        assert_eq!(montar(&[c(0, 5), c(0, 3), c(0, 4)]).map(|j| (valores(&j), j.suja)), Some((vec![3, 4, 5], false)));
        assert!(montar(&[c(0, 5), c(1, 6), c(0, 7)]).is_none(), "naipes diferentes");
        assert!(montar(&[c(0, 5), c(0, 7)]).is_none(), "menos de três");
        // Ás baixo e alto; 2 do naipe no lugar dele é natural.
        assert_eq!(montar(&[c(0, 1), c(0, 2), c(0, 3)]).map(|j| j.suja), Some(false));
        assert_eq!(montar(&[c(0, 12), c(0, 13), c(0, 1)]).map(|j| valores(&j)), Some(vec![12, 13, 1]));
        // Curinga tapa o buraco ou vai na ponta.
        assert_eq!(montar(&[c(0, 5), c(1, 2), c(0, 7)]).map(|j| (valores(&j), j.suja)), Some((vec![5, 2, 7], true)));
        assert_eq!(montar(&[c(0, 12), c(0, 13), c(0, 1), c(2, 2)]).map(|j| valores(&j)), Some(vec![2, 12, 13, 1]));
        assert!(montar(&[c(0, 5), c(1, 2), c(2, 2), c(0, 8)]).is_none(), "dois curingas");
        assert!(montar(&[c(0, 5), c(0, 5), c(0, 6)]).is_none() && montar(&[c(0, 5), c2(0, 5), c(0, 6)]).is_none(), "carta repetida");
        assert!(montar(&[c(1, 2), c(2, 2), c(3, 2)]).is_none(), "só curingas");
    }

    #[test]
    fn pontuacao() {
        let limpa = montar(&(3..=9).map(|v| c(0, v)).collect::<Vec<_>>()).unwrap();
        let suja = montar(&[c(1, 3), c(1, 4), c(0, 2), c(1, 6), c(1, 7), c(1, 8), c(1, 9)]).unwrap();
        assert_eq!((limpa.bonus(), suja.bonus()), (200, 100));
        let lado = Lado { jogos: vec![limpa], pegou_morto: true, bateu: false };
        // 3..7 = 5 pontos cada (25), 8 e 9 = 10 cada (20), canastra limpa 200, ás na mão -15.
        assert_eq!(lado.pontos(15), 25 + 20 + 200 - 15);
        let sem_morto = Lado { pegou_morto: false, ..lado.clone() };
        assert_eq!(sem_morto.pontos(15), lado.pontos(15) - 100);
        // Em duplas, a mão dos dois parceiros conta contra a dupla.
        let mut m = Mao::nova(3, 0, 4);
        m.maos = vec![vec![c(2, 1)], vec![c(0, 3)], vec![c(1, 13)], vec![]];
        assert_eq!((m.na_mao(0), m.na_mao(1)), (25, 5));
    }

    #[test]
    fn arrastar_baixa_e_descarta() {
        let mut b = Buraco::novo([0, 0], 0, 0, 4);
        b.mao.fase = Fase::Jogar;
        b.mao.maos[0] = vec![c(0, 3), c(0, 4), c(0, 5), c(3, 9), c(3, 11)];
        let arrastar = |b: &mut Buraco, id: u8, alvo: Alvo| {
            let _ = b.update(Msg::Mouse(Point::new(100.0, 800.0)));
            let _ = b.update(Msg::Pegar(id));
            let _ = b.update(Msg::Mouse(Point::new(300.0, 500.0)));
            let _ = b.update(Msg::Sobre(alvo, true));
            let _ = b.update(Msg::Soltar);
            let _ = b.update(Msg::Sobre(alvo, false));
        };
        // Clique sem mexer escolhe; arrastar uma escolhida leva todas.
        for v in [3, 4, 5] {
            let _ = b.update(Msg::Pegar(c(0, v).id));
            let _ = b.update(Msg::Soltar);
        }
        assert_eq!(b.selecionadas.len(), 3);
        arrastar(&mut b, c(0, 4).id, Alvo::Mesa);
        assert_eq!(b.mao.lados[0].jogos.len(), 1);
        arrastar(&mut b, c(3, 9).id, Alvo::Lixo);
        assert_eq!(b.mao.lixo.last().map(|c| c.id), Some(c(3, 9).id));
        assert_eq!(b.mao.vez, 1);
    }

    #[test]
    fn morto_e_batida() {
        let mut m = Mao::nova(5, 0, 2);
        m.fase = Fase::Jogar;
        m.maos[0] = vec![c(0, 3), c(0, 4), c(0, 5), c(3, 9)];
        m.baixar(&[c(0, 3).id, c(0, 4).id, c(0, 5).id]).unwrap();
        // O descarte que esvazia a mão pega o morto e passa a vez.
        m.descartar(c(3, 9).id).unwrap();
        assert!(m.lados[0].pegou_morto);
        assert_eq!(m.maos[0].len(), 11);
        assert_eq!(m.vez, 1);
        // Depois do morto, sem canastra, não dá para ficar com menos de duas cartas.
        m.vez = 0;
        m.fase = Fase::Jogar;
        m.maos[0] = vec![c(0, 6), c(1, 9)];
        assert_eq!(m.adicionar(0, &[c(0, 6).id]), Err("PRECISA DE CANASTRA PARA BATER".to_string()));
        // Com canastra, bate.
        m.lados[0].jogos = vec![montar(&(3..=9).map(|v| c(0, v)).collect::<Vec<_>>()).unwrap()];
        m.maos[0] = vec![c(0, 10), c(1, 9)];
        m.adicionar(0, &[c(0, 10).id]).unwrap();
        m.descartar(c(1, 9).id).unwrap();
        assert!(m.acabou && m.lados[0].bateu);
    }

    #[test]
    fn bot_joga_uma_vez_valida() {
        for n in [2, 4] {
            let mut m = Mao::nova(9, 1, n);
            let total = |m: &Mao| m.monte.len() + m.lixo.len() + m.mortos.iter().map(Vec::len).sum::<usize>() + m.maos.iter().map(Vec::len).sum::<usize>() + m.lados.iter().map(|l| l.jogos.iter().map(|j| j.cartas.len()).sum::<usize>()).sum::<usize>();
            while m.vez != 0 && !m.acabou {
                passo_bot(&mut m);
                assert_eq!(total(&m), 104, "nenhuma carta some nem aparece");
            }
            assert_eq!((m.vez, m.fase), (0, Fase::Comprar));
            assert!(m.lados.iter().flat_map(|l| &l.jogos).all(|j| montar(&j.cartas).is_some()));
        }
    }

    #[test]
    fn mao_inteira_bot_contra_bot() {
        for (semente, n) in (1..8).flat_map(|s| [(s, 2), (s, 4)]) {
            let mut m = Mao::nova(semente, 0, n);
            let mut passos = 0;
            while !m.acabou {
                passo_bot(&mut m);
                passos += 1;
                assert!(passos < 5000, "a mão {semente} não acabou");
            }
            let _ = (m.lados[0].pontos(m.na_mao(0)), m.lados[1].pontos(m.na_mao(1)));
        }
    }

    #[test]
    fn parceiros_dividem_os_jogos() {
        let mut m = Mao::nova(4, 0, 4);
        m.fase = Fase::Jogar;
        m.maos[0] = vec![c(0, 3), c(0, 4), c(0, 5), c(3, 9), c(3, 11)];
        m.baixar(&[c(0, 3).id, c(0, 4).id, c(0, 5).id]).unwrap();
        m.descartar(c(3, 9).id).unwrap();
        assert_eq!(m.vez, 1);
        // A parceira (jogador 2) completa o jogo que você baixou; o adversário não pode.
        m.vez = 2;
        m.fase = Fase::Jogar;
        m.maos[2].push(c(0, 6));
        m.adicionar(0, &[c(0, 6).id]).unwrap();
        assert_eq!(m.lados[0].jogos[0].cartas.len(), 4);
        m.descartar(m.maos[2][0].id).unwrap();
        assert_eq!(m.vez, 3);
        m.fase = Fase::Jogar;
        m.maos[3].push(c2(0, 7));
        assert!(m.adicionar(0, &[c2(0, 7).id]).is_err());
    }
}
