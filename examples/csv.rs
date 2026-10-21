//! Visualizador de CSV só leitura, para comparar com a mesma tabela em Iced 0.13.
//! `cargo run --release --example csv -- arquivo.csv` (sem arquivo: 200 000 linhas sintéticas em memória).
//! `-- --gerar caminho.csv [linhas]` grava o CSV sintético; `-- --capturar out.png` só gera o PNG;
//! `-- --medir` mede quadros de rolagem e de redimensionar sem janela, nas escalas 1 e 2.
use pelicano::column;
use pelicano::*;
use pelicano::futures;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---------- núcleo: igual, linha a linha, no csv-iced ----------
// A carga segue a do FeatherSheet (`feathersheet-io/src/csv_colunas.rs`): o arquivo passa em pedaços de 64 KiB
// por um divisor de estados (numa thread), que entrega blocos de 65 536 linhas; outra etapa monta cada bloco
// por colunas, em paralelo, com um dicionário de textos por coluna. O arquivo nunca fica inteiro na memória.

const ALTURA_LINHA: f32 = 24.0;
const ALTURA_CABECALHO: f32 = 28.0;
const MARGEM: usize = 5;
const TAMANHO_TEXTO: f32 = 13.0;
const LARGURA_CHAR: f32 = 7.0;
const LINHAS_PADRAO: usize = 200_000;
const CHUNK_SIZE: usize = 65_536;
const CHUNK_SHIFT: u32 = 16;

/// Uma coluna de um bloco: os textos um atrás do outro e onde cada um termina. Com `ids`, os textos são um
/// dicionário (cada valor diferente uma vez) e `ids[linha]` aponta para ele; sem, há um texto por linha.
struct Coluna {
    texto: Vec<u8>,
    fins: Vec<u32>,
    ids: Option<Box<[u32]>>,
}

impl Coluna {
    fn get(&self, r: usize) -> &[u8] {
        let k = self.ids.as_ref().map_or(r, |ids| ids[r] as usize);
        &self.texto[if k == 0 { 0 } else { self.fins[k - 1] as usize }..self.fins[k] as usize]
    }
}

struct Tabela {
    nome: String,
    /// Blocos de [`CHUNK_SIZE`] linhas do arquivo; a linha 0 do arquivo é o cabeçalho.
    blocos: Vec<Vec<Coluna>>,
    linhas_arquivo: usize,
    larguras: Vec<f32>,
    /// `pre[c]`: a soma das larguras antes da coluna `c` (tem `ncols + 1` entradas).
    pre: Vec<f32>,
    largura_num: f32,
    analise: Duration,
}

impl std::fmt::Debug for Tabela {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tabela({}, {} linhas)", self.nome, self.linhas_arquivo)
    }
}

impl Tabela {
    /// Linhas de dados (sem o cabeçalho).
    fn n(&self) -> usize {
        self.linhas_arquivo.saturating_sub(1)
    }
    /// O texto da célula `c` da linha `r` do arquivo (0 = cabeçalho). Só copia se precisar.
    fn campo(&self, r: usize, c: usize) -> Cow<'_, str> {
        let Some(col) = self.blocos[r >> CHUNK_SHIFT].get(c) else { return Cow::Borrowed("") };
        let b = col.get(r & (CHUNK_SIZE - 1));
        if b.contains(&b'\n') || b.contains(&b'\r') {
            return Cow::Owned(String::from_utf8_lossy(b).replace(['\r', '\n'], " "));
        }
        String::from_utf8_lossy(b)
    }
    fn largura_total(&self) -> f32 {
        self.largura_num + self.pre[self.larguras.len()]
    }
    fn altura_total(&self) -> f32 {
        ALTURA_CABECALHO + self.n() as f32 * ALTURA_LINHA
    }
    /// As linhas a montar para a rolagem `y` numa vista de altura `h`, com folga dos dois lados.
    fn faixa(&self, y: f32, h: f32) -> (usize, usize) {
        let n = self.n();
        let ini = ((y / ALTURA_LINHA) as usize).saturating_sub(MARGEM).min(n);
        let fim = (((y + h) / ALTURA_LINHA).ceil() as usize + MARGEM).min(n);
        (ini, fim.max(ini))
    }
    /// As colunas que caem na faixa `x..x + w` (mais uma de folga de cada lado).
    fn colunas(&self, x: f32, w: f32) -> (usize, usize) {
        let x = x - self.largura_num;
        let nc = self.larguras.len();
        let ini = (0..nc).find(|&c| self.pre[c + 1] > x).unwrap_or(nc).saturating_sub(1);
        let fim = (ini..nc).find(|&c| self.pre[c] >= x + w).unwrap_or(nc);
        (ini, (fim + 1).min(nc))
    }
}

/// Gerador congruente: o mesmo arquivo nas duas versões.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a>(&mut self, v: &[&'a str]) -> &'a str {
        v[self.next() as usize % v.len()]
    }
}

fn campo(out: &mut String, s: &str) {
    if s.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

fn gerar(linhas: usize) -> String {
    const NOMES: &[&str] = &["Ana", "Bruno", "Carla", "Diego", "Elisa", "Fábio", "Gabriela", "Heitor", "Isabela", "João", "Lívia", "Marcos", "Natália", "Otávio", "Paula", "Rafael", "Sofia", "Tiago", "Vitória", "Yuri"];
    const SOBRENOMES: &[&str] = &["Silva", "Santos", "Oliveira", "Souza", "Rodrigues", "Ferreira", "Alves", "Pereira", "Lima", "Gomes", "Costa", "Ribeiro", "Martins", "Carvalho", "Almeida"];
    const CIDADES: &[(&str, &str)] = &[("São Paulo", "SP"), ("Rio de Janeiro", "RJ"), ("Belo Horizonte", "MG"), ("Curitiba", "PR"), ("Porto Alegre", "RS"), ("Salvador", "BA"), ("Recife", "PE"), ("Fortaleza", "CE"), ("Manaus", "AM"), ("Goiânia", "GO"), ("Florianópolis", "SC"), ("Belém", "PA")];
    const CATEGORIAS: &[&str] = &["Eletrônicos", "Livros", "Casa e Jardim", "Brinquedos", "Esporte", "Moda", "Alimentos", "Ferramentas"];
    const STATUS: &[&str] = &["pago", "pendente", "enviado", "entregue", "cancelado", "devolvido"];
    const PALAVRAS: &[&str] = &["entrega", "rápida", "cliente", "pediu", "embalagem", "presente", "nota fiscal", "reenviar", "conferir", "endereço", "portaria", "horário comercial", "ligar antes", "frágil"];
    let mut g = Lcg(0x2545_F491_4F6C_DD1D);
    let mut s = String::with_capacity(linhas * 160);
    s.push_str("id,nome,cidade,estado,data,valor,quantidade,categoria,email,telefone,status,observação\r\n");
    for id in 1..=linhas {
        let (nome, sobrenome) = (g.pick(NOMES), g.pick(SOBRENOMES));
        let (cidade, estado) = CIDADES[g.next() as usize % CIDADES.len()];
        let obs = match g.next() % 7 {
            0 => String::new(),
            1 => format!("Cliente disse \"{}\", ok", g.pick(PALAVRAS)),
            n => (0..n * 3).map(|_| g.pick(PALAVRAS)).collect::<Vec<_>>().join(", "),
        };
        let cols = [
            id.to_string(),
            format!("{nome} {sobrenome}"),
            cidade.to_string(),
            estado.to_string(),
            format!("2024-{:02}-{:02}", g.next() % 12 + 1, g.next() % 28 + 1),
            format!("{}.{:02}", g.next() % 10_000, g.next() % 100),
            (g.next() % 50 + 1).to_string(),
            g.pick(CATEGORIAS).to_string(),
            format!("{}.{}{}@exemplo.com.br", nome.to_lowercase(), sobrenome.to_lowercase(), g.next() % 1000),
            format!("({:02}) 9{:04}-{:04}", g.next() % 89 + 11, g.next() % 10_000, g.next() % 10_000),
            g.pick(STATUS).to_string(),
            obs,
        ];
        for (i, c) in cols.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            campo(&mut s, c);
        }
        s.push_str("\r\n");
    }
    s
}

/// As linhas de um bloco, como o divisor as entrega: o conteúdo dos campos (já sem aspas) um atrás do
/// outro, onde cada campo termina e onde cada linha começa (em campos).
struct BlocoLinhas {
    indice: usize,
    texto: Vec<u8>,
    fins: Vec<u32>,
    /// Primeiro campo de cada linha; tem uma entrada a mais que as linhas.
    linhas: Vec<u32>,
}

impl BlocoLinhas {
    fn novo(indice: usize) -> Self {
        BlocoLinhas { indice, texto: Vec::new(), fins: Vec::new(), linhas: vec![0] }
    }

    fn quantas_linhas(&self) -> usize {
        self.linhas.len() - 1
    }

    /// O campo `c` da linha `r` do bloco (vazio se a linha for mais curta).
    fn campo(&self, r: usize, c: usize) -> &[u8] {
        let p = self.linhas[r] as usize;
        if p + c >= self.linhas[r + 1] as usize {
            return b"";
        }
        let i = p + c;
        &self.texto[if i == 0 { 0 } else { self.fins[i - 1] as usize }..self.fins[i] as usize]
    }
}

/// O divisor do FeatherSheet como máquina de estados, alimentado aos pedaços: o estado das aspas
/// atravessa os pedaços e as linhas físicas. Aspas, `""`, quebra de linha dentro de aspas, CR, LF, CRLF e BOM.
// shortcut: o texto de um bloco é indexado em u32: 65 536 linhas com mais de 4 GiB juntas estouram; aí, blocos menores.
struct Divisor {
    delim: u8,
    entre_aspas: bool,
    /// Aspa vista dentro de aspas: a próxima decide se é `""` ou o fim.
    aspa_pendente: bool,
    ignorar_lf: bool,
    inicio_do_arquivo: bool,
    inicio_campo: usize,
    bloco: BlocoLinhas,
}

impl Divisor {
    fn novo(delim: u8) -> Self {
        Divisor { delim, entre_aspas: false, aspa_pendente: false, ignorar_lf: false, inicio_do_arquivo: true, inicio_campo: 0, bloco: BlocoLinhas::novo(0) }
    }

    fn campo_vazio(&self) -> bool {
        self.bloco.texto.len() == self.inicio_campo
    }

    fn fim_campo(&mut self) {
        self.bloco.fins.push(self.bloco.texto.len() as u32);
        self.inicio_campo = self.bloco.texto.len();
    }

    fn fim_linha(&mut self, entregar: &mut impl FnMut(BlocoLinhas)) {
        self.bloco.linhas.push(self.bloco.fins.len() as u32);
        if self.bloco.quantas_linhas() == CHUNK_SIZE {
            let proximo = BlocoLinhas::novo(self.bloco.indice + 1);
            entregar(std::mem::replace(&mut self.bloco, proximo));
            self.inicio_campo = 0;
        }
    }

    fn alimentar(&mut self, mut buf: &[u8], entregar: &mut impl FnMut(BlocoLinhas)) {
        if self.inicio_do_arquivo && !buf.is_empty() {
            // O BOM só conta no começo do arquivo; o primeiro pedaço tem sempre mais de 3 bytes (64 KiB).
            buf = buf.strip_prefix(b"\xEF\xBB\xBF".as_slice()).unwrap_or(buf);
            self.inicio_do_arquivo = false;
        }
        let d = self.delim;
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];
            if std::mem::take(&mut self.ignorar_lf) && b == b'\n' {
                i += 1;
                continue;
            }
            if std::mem::take(&mut self.aspa_pendente) {
                if b == b'"' {
                    self.bloco.texto.push(b'"');
                    i += 1;
                    continue;
                }
                self.entre_aspas = false;
            }
            if self.entre_aspas {
                match buf[i..].iter().position(|&x| x == b'"') {
                    Some(p) => {
                        self.bloco.texto.extend_from_slice(&buf[i..i + p]);
                        self.aspa_pendente = true;
                        i += p + 1;
                    }
                    None => {
                        self.bloco.texto.extend_from_slice(&buf[i..]);
                        i = buf.len();
                    }
                }
                continue;
            }
            if b == b'"' && self.campo_vazio() {
                self.entre_aspas = true;
                i += 1;
            } else if b == d {
                self.fim_campo();
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                self.ignorar_lf = b == b'\r';
                self.fim_campo();
                self.fim_linha(entregar);
                i += 1;
            } else {
                // Um trecho comum de uma vez, até o próximo separador ou fim de linha (aspa no meio é conteúdo).
                let p = buf[i..].iter().position(|&x| x == d || x == b'\n' || x == b'\r').unwrap_or(buf.len() - i);
                self.bloco.texto.extend_from_slice(&buf[i..i + p]);
                i += p;
            }
        }
    }

    /// Fim do arquivo: a última linha sem quebra no fim também conta.
    fn terminar(mut self, entregar: &mut impl FnMut(BlocoLinhas)) {
        self.aspa_pendente = false;
        let inicio = *self.bloco.linhas.last().unwrap() as usize;
        if !self.campo_vazio() || self.bloco.fins.len() > inicio {
            self.fim_campo();
            self.fim_linha(entregar);
        }
        if self.bloco.quantas_linhas() > 0 {
            entregar(self.bloco);
        }
    }
}

/// Hash de multiplicar e girar, para o dicionário de textos de cada coluna.
#[derive(Default)]
struct HashRapido(u64);

impl Hasher for HashRapido {
    fn write(&mut self, bytes: &[u8]) {
        const K: u64 = 0x51_7c_c1_b7_27_22_0a_95;
        let mut pedacos = bytes.chunks_exact(8);
        for p in &mut pedacos {
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(p.try_into().unwrap())).wrapping_mul(K);
        }
        for &b in pedacos.remainder() {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(K);
        }
    }

    fn write_u8(&mut self, b: u8) {
        self.write(&[b]);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Monta uma coluna de um bloco, campo a campo (na ordem das linhas). Com dicionário, cada texto diferente
/// fica uma vez só; se a amostra das primeiras linhas for quase toda diferente (ids, nomes de faixa), o
/// dicionário só gastaria mais: guarda um texto por linha.
struct Construtor<'a> {
    dict: Option<HashMap<&'a [u8], u32, BuildHasherDefault<HashRapido>>>,
    texto: Vec<u8>,
    fins: Vec<u32>,
    ids: Vec<u32>,
}

impl<'a> Construtor<'a> {
    fn novo(b: &'a BlocoLinhas, c: usize) -> Self {
        let amostra = b.quantas_linhas().min(1024);
        let diferentes: std::collections::HashSet<&[u8], BuildHasherDefault<HashRapido>> = (0..amostra).map(|r| b.campo(r, c)).collect();
        let dict = (diferentes.len() * 2 <= amostra).then(HashMap::default);
        Construtor { dict, texto: Vec::new(), fins: Vec::new(), ids: Vec::new() }
    }

    fn campo(&mut self, f: &'a [u8]) {
        let (texto, fins) = (&mut self.texto, &mut self.fins);
        let mut guardar = || {
            texto.extend_from_slice(f);
            fins.push(texto.len() as u32);
            fins.len() as u32 - 1
        };
        match &mut self.dict {
            Some(d) => self.ids.push(*d.entry(f).or_insert_with(guardar)),
            None => {
                guardar();
            }
        }
    }

    fn terminar(mut self) -> Coluna {
        (self.texto.shrink_to_fit(), self.fins.shrink_to_fit());
        Coluna { texto: self.texto, fins: self.fins, ids: self.dict.map(|_| self.ids.into_boxed_slice()) }
    }
}

/// As colunas `de..ate` do bloco, percorrendo o texto na ordem em que está guardado (linha a linha):
/// coluna a coluna, cada campo lido fica longe do anterior na memória e o processador espera a RAM.
fn montar_grupo(b: &BlocoLinhas, de: usize, ate: usize) -> Vec<Coluna> {
    let mut k: Vec<Construtor> = (de..ate).map(|c| Construtor::novo(b, c)).collect();
    for r in 0..b.quantas_linhas() {
        for (c, k) in (de..ate).zip(&mut k) {
            k.campo(b.campo(r, c));
        }
    }
    k.into_iter().map(Construtor::terminar).collect()
}

/// As colunas do bloco em `nucleos` threads, cada uma com a sua faixa de colunas.
fn montar_colunas(b: &BlocoLinhas, nucleos: usize) -> Vec<Coluna> {
    let colunas = (0..b.quantas_linhas()).map(|r| (b.linhas[r + 1] - b.linhas[r]) as usize).max().unwrap_or(0);
    let por_thread = colunas.div_ceil(nucleos.clamp(1, colunas.max(1))).max(1);
    std::thread::scope(|escopo| {
        let tarefas: Vec<_> = (0..colunas).step_by(por_thread).map(|de| escopo.spawn(move || montar_grupo(b, de, (de + por_thread).min(colunas)))).collect();
        tarefas.into_iter().flat_map(|t| t.join().expect("montar colunas")).collect()
    })
}

/// Lê tudo de `leitor` (dividir numa thread, montar as colunas nesta) e chama `progresso(bytes lidos)`.
fn ler(nome: String, leitor: impl BufRead + Send, progresso: &(dyn Fn(u64) + Sync)) -> Result<Tabela, String> {
    let t0 = Instant::now();
    let nucleos = std::thread::available_parallelism().map_or(1, |n| n.get());
    let (para_montar, a_montar) = std::sync::mpsc::sync_channel::<BlocoLinhas>(2);
    let (blocos, linhas) = std::thread::scope(|escopo| {
        let divisor = escopo.spawn(move || -> std::io::Result<()> {
            let mut leitor = leitor;
            let mut divisor = Divisor::novo(b',');
            let mut entregar = |bloco: BlocoLinhas| {
                let _ = para_montar.send(bloco);
            };
            let mut lidos = 0u64;
            loop {
                let buf = leitor.fill_buf()?;
                if buf.is_empty() {
                    break;
                }
                let n = buf.len();
                divisor.alimentar(buf, &mut entregar);
                leitor.consume(n);
                lidos += n as u64;
                progresso(lidos);
            }
            divisor.terminar(&mut entregar);
            Ok(())
        });
        let (mut blocos, mut linhas) = (Vec::new(), 0);
        for bloco in a_montar {
            linhas += bloco.quantas_linhas();
            blocos.push(montar_colunas(&bloco, nucleos));
        }
        divisor.join().map_err(|_| "falha ao dividir o CSV".to_string())?.map_err(|e| format!("{nome}: {e}"))?;
        Ok::<_, String>((blocos, linhas))
    })?;
    let mut t = Tabela { nome, blocos, linhas_arquivo: linhas, larguras: Vec::new(), pre: vec![0.0], largura_num: 0.0, analise: t0.elapsed() };
    let amostra = linhas.min(201);
    let ncols = t.blocos.first().map_or(0, Vec::len);
    t.larguras = (0..ncols).map(|c| largura_de((0..amostra).map(|r| t.campo(r, c).chars().count()).max().unwrap_or(0))).collect();
    t.pre = std::iter::once(0.0)
        .chain(t.larguras.iter().scan(0.0, |s, w| {
            *s += w;
            Some(*s)
        }))
        .collect();
    t.largura_num = largura_de(t.n().to_string().len());
    Ok(t)
}

fn largura_de(chars: usize) -> f32 {
    (chars as f32 * LARGURA_CHAR + 16.0).clamp(60.0, 320.0)
}

/// De onde vem a tabela: um arquivo ou o CSV sintético.
#[derive(Clone)]
struct Origem {
    nome: String,
    arquivo: Option<std::path::PathBuf>,
}

/// `--gerar caminho [linhas]` grava e sai (`Ok(None)`); senão o arquivo do argumento ou o sintético.
fn origem(args: &[String]) -> Result<Option<Origem>, String> {
    if let Some(i) = args.iter().position(|a| a == "--gerar") {
        let caminho = args.get(i + 1).ok_or("uso: --gerar caminho.csv [linhas]")?;
        let n = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(LINHAS_PADRAO);
        std::fs::write(caminho, gerar(n)).map_err(|e| format!("{caminho}: {e}"))?;
        println!("{caminho}: {n} linhas");
        return Ok(None);
    }
    Ok(Some(match args.get(1).filter(|a| !a.starts_with("--")) {
        Some(caminho) => {
            let p = std::path::PathBuf::from(caminho);
            Origem { nome: p.file_name().map_or(caminho.clone(), |n| n.to_string_lossy().into_owned()), arquivo: Some(p) }
        }
        None => Origem { nome: format!("sintético ({LINHAS_PADRAO} linhas)"), arquivo: None },
    }))
}

/// Carrega na thread de quem chama. `progresso(lidos, total)`.
fn carregar(o: &Origem, progresso: &(dyn Fn(u64, u64) + Sync)) -> Result<Tabela, String> {
    let t = match &o.arquivo {
        Some(p) => {
            let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let total = f.metadata().map_or(0, |m| m.len());
            ler(o.nome.clone(), BufReader::with_capacity(64 * 1024, f), &|n| progresso(n, total))?
        }
        None => {
            let texto = gerar(LINHAS_PADRAO).into_bytes();
            let total = texto.len() as u64;
            ler(o.nome.clone(), BufReader::with_capacity(64 * 1024, std::io::Cursor::new(texto)), &|n| progresso(n, total))?
        }
    };
    eprintln!("{}: {} linhas × {} colunas, carga {:.1} ms", t.nome, t.n(), t.larguras.len(), t.analise.as_secs_f64() * 1000.0);
    Ok(t)
}

#[derive(Debug, Clone)]
enum Carga {
    Progresso(u64, u64),
    Pronta(Result<Arc<Tabela>, String>),
}

/// Carrega numa thread à parte; o progresso (no máximo a cada 50 ms) e o fim chegam pelo fluxo.
fn carregar_em_fundo(o: Origem) -> futures::channel::mpsc::UnboundedReceiver<Carga> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        let t0 = Instant::now();
        let ultimo = AtomicU64::new(0);
        let r = carregar(&o, &|n, total| {
            let ms = t0.elapsed().as_millis() as u64;
            if ms >= ultimo.load(Ordering::Relaxed) + 50 {
                ultimo.store(ms, Ordering::Relaxed);
                let _ = tx.unbounded_send(Carga::Progresso(n, total));
            }
        });
        let _ = tx.unbounded_send(Carga::Pronta(r.map(Arc::new)));
    });
    rx
}

/// Memória (VmRSS e o pico, VmHWM) e CPU do processo, lidas do /proc a cada segundo. CPU em % de um núcleo (como no `top`).
#[derive(Default)]
struct Recursos {
    rss_mib: f32,
    pico_mib: f32,
    cpu: f32,
    antes: Option<(u64, Instant)>,
}

impl Recursos {
    fn amostrar(&mut self) {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            let kb = |chave: &str| s.lines().find_map(|l| l.strip_prefix(chave).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f32>().ok()));
            self.rss_mib = kb("VmRSS:").unwrap_or(0.0) / 1024.0;
            self.pico_mib = kb("VmHWM:").unwrap_or(0.0) / 1024.0;
        }
        // shortcut: CLK_TCK fixo em 100 (o do Linux comum), para não puxar a libc; outro valor erra a escala da CPU.
        let ticks = std::fs::read_to_string("/proc/self/stat").ok().and_then(|s| {
            let f: Vec<u64> = s[s.rfind(')')? + 2..].split(' ').map(|x| x.parse().unwrap_or(0)).collect();
            Some(f.get(11)? + f.get(12)?)
        });
        let agora = Instant::now();
        if let (Some(t), Some((t0, i0))) = (ticks, self.antes) {
            self.cpu = t.saturating_sub(t0) as f32 / 100.0 / (agora - i0).as_secs_f32().max(1e-3) * 100.0;
        }
        self.antes = ticks.map(|t| (t, agora));
    }
}

// ---------- interface ----------

const FUNDO: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
fn cor(hex: &str) -> Color {
    Color::hex(hex).unwrap_or(FUNDO)
}

#[derive(Clone, Copy)]
enum Tecla {
    Cima,
    Baixo,
    Inicio,
    Fim,
}

#[derive(Clone)]
enum Msg {
    Rolou(f32, f32),
    Janela(f32, f32),
    Selecionar(usize),
    Tique,
    Tecla(Tecla),
    Carga(Carga),
    Nada,
}

struct Visor {
    t: Option<Arc<Tabela>>,
    /// Bytes lidos e o total, enquanto carrega; ou o erro da carga.
    carga: (u64, u64),
    erro: Option<String>,
    x: f32,
    y: f32,
    /// O tamanho da janela: vista folgada da tabela (as barras de cima e de baixo só sobram).
    vista: (f32, f32),
    sel: Option<usize>,
    fps: u32,
    rec: Recursos,
    // O `view` só tem `&self`: os contadores de quadro ficam em `Cell`.
    quadros: Cell<u32>,
    ultimo: Cell<Option<Instant>>,
    intervalo: Cell<f32>,
}

impl Visor {
    fn new(t: Option<Arc<Tabela>>) -> Self {
        let mut rec = Recursos::default();
        rec.amostrar();
        Visor { t, carga: (0, 0), erro: None, x: 0.0, y: 0.0, vista: (1280.0, 800.0), sel: None, fps: 0, rec, quadros: Cell::new(0), ultimo: Cell::new(None), intervalo: Cell::new(0.0) }
    }

    /// Enquanto carrega: bytes lidos do total, numa barra; ou o erro.
    fn tela_carga(&self, texto: Color, mudo: Color) -> Element<'_, Msg> {
        let (lidos, total) = self.carga;
        let mib = |b: u64| b as f64 / 1048576.0;
        let msg = match &self.erro {
            Some(e) => e.clone(),
            None => format!("carregando… {:.0} / {:.0} MiB", mib(lidos), mib(total)),
        };
        let fracao = if total > 0 { lidos as f32 / total as f32 } else { 0.0 };
        let recursos = format!("RAM {:.0} MiB · pico {:.0} MiB · CPU {:.0}% de 1 núcleo ({:.1}% da máquina)", self.rec.rss_mib, self.rec.pico_mib, self.rec.cpu, self.rec.cpu / nucleos());
        container(column![text(msg).size(14).color(texto), progress_bar(0.0..=1.0, fracao).width(360.0), text(recursos).size(12).color(mudo)].spacing(10)).center().width(Length::FILL).height(Length::FILL).into()
    }
}

fn celula<'a>(s: impl Into<Cow<'a, str>>, w: f32, h: f32, fonte: Font, c: Color) -> Element<'a, Msg> {
    container(text(s).size(TAMANHO_TEXTO).font(fonte).color(c).no_wrap()).width(w).height(h).padding([0, 8]).align_y(Align::Center).clip(true).into()
}

impl App for Visor {
    type Msg = Msg;

    fn theme(&self) -> Theme {
        Theme::light()
    }

    fn update(&mut self, m: Msg) -> Task<Msg> {
        match m {
            Msg::Rolou(x, y) => (self.x, self.y) = (x, y),
            Msg::Janela(w, h) => self.vista = (w, h),
            Msg::Selecionar(i) => self.sel = Some(i),
            Msg::Tique => {
                self.fps = self.quadros.replace(0);
                self.rec.amostrar();
            }
            Msg::Carga(Carga::Progresso(lidos, total)) => self.carga = (lidos, total),
            Msg::Carga(Carga::Pronta(r)) => {
                match r {
                    Ok(t) => self.t = Some(t),
                    Err(e) => self.erro = Some(e),
                }
                self.rec.amostrar();
            }
            Msg::Tecla(k) => {
                let Some(t) = &self.t else { return Task::none() };
                let pagina = (self.vista.1 - 120.0).max(ALTURA_LINHA);
                let max = (t.altura_total() - (self.vista.1 - 70.0)).max(0.0);
                self.y = match k {
                    Tecla::Cima => self.y - pagina,
                    Tecla::Baixo => self.y + pagina,
                    Tecla::Inicio => 0.0,
                    Tecla::Fim => max,
                }
                .clamp(0.0, max);
                return Task::scroll_to("tabela", Point::new(self.x, self.y));
            }
            Msg::Nada => {}
        }
        Task::none()
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        match ev {
            Event::Resized(s) => Some(Msg::Janela(s.width, s.height)),
            Event::Key { key: Key::Named(k), pressed: true, .. } => Some(Msg::Tecla(match k {
                NamedKey::PageUp => Tecla::Cima,
                NamedKey::PageDown => Tecla::Baixo,
                NamedKey::Home => Tecla::Inicio,
                NamedKey::End => Tecla::Fim,
                _ => return None,
            })),
            _ => None,
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        Subscription::every(Duration::from_secs(1)).map(|_| Msg::Tique)
    }

    fn view(&self) -> Element<'_, Msg> {
        let agora = Instant::now();
        if let Some(antes) = self.ultimo.replace(Some(agora)) {
            self.intervalo.set((agora - antes).as_secs_f32() * 1000.0);
        }
        self.quadros.set(self.quadros.get() + 1);

        let (texto, mudo) = (cor("#1f2328"), cor("#59636e"));
        let Some(t) = self.t.as_deref() else { return self.tela_carga(texto, mudo) };
        let (ini, fim) = t.faixa(self.y, self.vista.1);
        let (c0, c1) = t.colunas(self.x, self.vista.0);
        let total_w = t.largura_total();
        let (esq, dir) = (t.pre[c0], t.pre[t.larguras.len()] - t.pre[c1]);

        let topo = container(
            row![
                text(t.nome.as_str()).size(14).font(Font::BOLD).color(texto),
                text(format!("{} linhas × {} colunas", t.n(), t.larguras.len())).size(13).color(mudo),
                text(format!("carga: {:.1} ms", t.analise.as_secs_f64() * 1000.0)).size(13).color(mudo),
                horizontal_space(),
                text(format!("RAM {:.0} MiB (pico {:.0}) · CPU {:.0}% de 1 núcleo ({:.1}% da máquina) · {} views/s · Δ {:.1} ms", self.rec.rss_mib, self.rec.pico_mib, self.rec.cpu, self.rec.cpu / nucleos(), self.fps, self.intervalo.get())).size(13).font(Font::MONO).color(texto),
            ]
            .spacing(18)
            .align(Align::Center),
        )
        .padding([8, 12])
        .width(Length::FILL)
        .style(|_| Style::default().background(cor("#f6f8fa")));

        // Uma linha: número, espaço no lugar das colunas à esquerda, as colunas à mostra, espaço das da direita.
        let faixa = |num: Element<'static, Msg>, r: usize, h: f32, fonte: Font| -> Vec<Element<'_, Msg>> {
            let cels = (c0..c1).map(move |c| celula(t.campo(r, c), t.larguras[c], h, fonte, texto));
            std::iter::once(num).chain([Space::new(esq, h).into()]).chain(cels).chain([Space::new(dir, h).into()]).collect()
        };
        let linhas = (ini..fim).map(|i| {
            let fundo = if self.sel == Some(i) { cor("#cfe2ff") } else if i % 2 == 1 { cor("#f3f5f8") } else { FUNDO };
            let num = celula((i + 1).to_string(), t.largura_num, ALTURA_LINHA, Font::DEFAULT, mudo);
            let linha = container(row(faixa(num, i + 1, ALTURA_LINHA, Font::DEFAULT)))
                .width(total_w)
                .height(ALTURA_LINHA)
                .style(move |_| Style::default().background(fundo));
            mouse_area(linha).on_press(Msg::Selecionar(i)).into()
        });
        let corpo = column(
            std::iter::once(Space::new(total_w, ALTURA_CABECALHO + ini as f32 * ALTURA_LINHA).into())
                .chain(linhas)
                .chain([Space::new(total_w, (t.n() - fim) as f32 * ALTURA_LINHA).into()])
                .collect::<Vec<Element<Msg>>>(),
        );

        let num = celula("#", t.largura_num, ALTURA_CABECALHO, Font::BOLD, mudo);
        let cab = container(row(faixa(num, 0, ALTURA_CABECALHO, Font::BOLD)))
            .width(total_w)
            .height(ALTURA_CABECALHO)
            .style(|_| Style::default().background(cor("#e8ecf1")));
        // O cabeçalho desce junto com a rolagem vertical: fica sempre no topo e rola na horizontal com as células.
        let fixo = column![Space::new(total_w, self.y), mouse_area(cab).on_press(Msg::Nada)];

        let tabela = scrollable(stack![corpo, fixo]).direction(Direction::Both).id("tabela").on_scroll(|v| Msg::Rolou(v.offset.x, v.offset.y));

        let rodape = container(
            text(format!(
                "linhas {}–{} e colunas {}–{} montadas · selecionada: {}",
                ini + 1,
                fim,
                c0 + 1,
                c1,
                self.sel.map_or("nenhuma".to_string(), |s| (s + 1).to_string())
            ))
            .size(12)
            .color(mudo),
        )
        .padding([4, 12])
        .width(Length::FILL)
        .style(|_| Style::default().background(cor("#f6f8fa")));

        column![topo, horizontal_rule(1.0), tabela, horizontal_rule(1.0), rodape].into()
    }
}

fn mediana(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// `--medir`: rola a tabela com a roda (eventos de verdade, pelo `dispatch`) na vertical e na horizontal,
/// e redimensiona, sem janela. Cada quadro é view + layout + pintura inteira (a janela redesenha só o que mudou).
fn medir(t: Tabela) {
    let mut app = Visor::new(Some(Arc::new(t)));
    let tema = app.theme();
    for escala in [1.0, 2.0] {
        let (mut r, mut st, mut clip) = (Renderer::new(), States::default(), MemoryClipboard::default());
        let mut tempos: [Vec<f64>; 3] = Default::default();
        for passo in 0..260 {
            // 200 passos de roda (3 linhas cada), 30 de roda na horizontal, 30 de redimensionar.
            let tam = if passo < 230 { Size::new(1280.0, 800.0) } else { Size::new(1280.0 + (passo - 230) as f32 * 9.0, 800.0 + (passo - 230) as f32 * 6.0) };
            if let Some(m) = app.on_event(&Event::Resized(tam)) {
                let _ = app.update(m);
            }
            let t0 = Instant::now();
            let el = app.view();
            let no = layout(&el, tam, &mut r, &st, &tema);
            r.begin(tam, escala, tema.background);
            draw(&el, &no, &mut r, &st, &tema, None, false);
            let fase = if passo < 200 { 0 } else if passo < 230 { 1 } else { 2 };
            // Os primeiros quadros de cada fase aquecem os caches de texto.
            if !matches!(passo, 0..=2 | 200..=202 | 230..=232) {
                tempos[fase].push(t0.elapsed().as_secs_f64() * 1000.0);
            }
            let (mut msgs, mut janela) = (Vec::new(), Vec::new());
            let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: Some(Point::new(640.0, 400.0)), messages: &mut msgs, window: &mut janela, redraw: false, clipboard: &mut clip };
            let (dx, dy) = if passo < 200 { (0.0, -3.0 * ALTURA_LINHA) } else { (-120.0, 0.0) };
            if passo < 230 {
                dispatch(&el, &no, &Event::Wheel { dx, dy, modifiers: Modifiers::default() }, &mut cx);
            }
            drop(el);
            for m in msgs {
                let _ = app.update(m);
            }
        }
        let [v, h, d] = tempos;
        println!(
            "escala {escala}: rolar vertical mediana {:.2} ms (máx {:.2}) · horizontal {:.2} ms · redimensionar {:.2} ms; parou na linha {} coluna {}",
            mediana(v.clone()),
            v.iter().cloned().fold(0.0, f64::max),
            mediana(h),
            mediana(d),
            (app.y / ALTURA_LINHA) as usize + 1,
            app.t.as_ref().map_or(0, |t| t.colunas(app.x, 0.0).0 + 2)
        );
        (app.x, app.y) = (0.0, 0.0);
    }
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let o = match origem(&args) {
        Ok(Some(o)) => o,
        Ok(None) => return Ok(()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let medir_ = args.iter().any(|a| a == "--medir");
    let capturar = args.iter().position(|a| a == "--capturar");
    if medir_ || capturar.is_some() {
        let t = carregar(&o, &|_, _| {}).map_err(Error::Window)?;
        if medir_ {
            medir(t);
            return Ok(());
        }
        let mut app = Visor::new(Some(Arc::new(t)));
        app.sel = Some(3);
        let png = capture(&app.view(), Size::new(1280.0, 800.0), 1.0, &app.theme(), &mut Renderer::new(), None);
        let destino = capturar.and_then(|i| args.get(i + 1)).map_or("csv.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    // `--cache`: liga o cache de textos entre quadros (para comparar RAM e CPU).
    let cache = args.iter().any(|a| a == "--cache");
    let settings = Settings { title: format!("CSV (pelicano{}) — {}", if cache { ", cache" } else { "" }, o.nome), size: Size::new(1280.0, 800.0), text_cache: cache, ..Default::default() };
    run(settings, || (Visor::new(None), Task::run(carregar_em_fundo(o), Msg::Carga)))
}

/// Os núcleos da máquina: o monitor do sistema divide a CPU por eles.
fn nucleos() -> f32 {
    std::thread::available_parallelism().map_or(1, |n| n.get()) as f32
}
