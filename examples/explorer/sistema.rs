//! O que vem do sistema: pastas, datas, prévias e as medidas do /proc.
// shortcut: as medidas e as datas só funcionam no Linux; noutro sistema os gráficos ficam vazios.
use pelicano::Image;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Entrada {
    pub nome: String,
    pub caminho: PathBuf,
    pub pasta: bool,
    pub tamanho: u64,
    pub modificado: Option<SystemTime>,
}

/// O conteúdo de uma pasta sem os ocultos, pastas primeiro.
pub fn listar(dir: &Path) -> std::io::Result<Vec<Entrada>> {
    let mut v: Vec<Entrada> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().into_owned();
            if nome.starts_with('.') {
                return None;
            }
            let md = std::fs::metadata(e.path()).ok();
            Some(Entrada {
                pasta: md.as_ref().is_some_and(|m| m.is_dir()),
                tamanho: md.as_ref().map_or(0, |m| m.len()),
                modificado: md.and_then(|m| m.modified().ok()),
                caminho: e.path(),
                nome,
            })
        })
        .collect();
    v.sort_by(|a, b| b.pasta.cmp(&a.pasta).then_with(|| a.nome.to_lowercase().cmp(&b.nome.to_lowercase())));
    Ok(v)
}

pub fn tamanho_humano(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", U[i]) }
}

pub fn extensao(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// (nome do tipo, ícone)
pub fn tipo(e: &Entrada) -> (&'static str, &'static str) {
    if e.pasta {
        return ("Pasta", "pasta_cheia");
    }
    match extensao(&e.caminho).as_str() {
        "txt" | "log" => ("Texto", "texto"),
        "rs" => ("Rust", "codigo"),
        "sh" | "bash" | "zsh" | "fish" => ("Script", "terminal"),
        "md" => ("Markdown", "md"),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => ("Imagem", "foto"),
        "mp4" | "mkv" | "webm" | "avi" | "mov" => ("Vídeo", "video"),
        "mp3" | "flac" | "ogg" | "wav" | "opus" | "m4a" => ("Áudio", "musica"),
        "pdf" => ("PDF", "documento"),
        "doc" | "docx" | "odt" => ("Documento", "documento"),
        "zip" | "tar" | "gz" | "xz" | "zst" | "7z" | "rar" => ("Compactado", "arquivo"),
        "toml" | "json" | "yaml" | "yml" | "ini" | "conf" | "cfg" => ("Config", "codigo"),
        "py" | "js" | "ts" | "c" | "h" | "cpp" | "go" | "java" | "lua" => ("Código", "codigo"),
        "html" | "css" => ("Web", "codigo"),
        "desktop" => ("Atalho", "arquivo"),
        _ => ("Arquivo", "arquivo"),
    }
}

/// Segundos do fuso local (o `date +%z` uma vez só: a std não sabe o fuso).
fn fuso() -> i64 {
    static F: OnceLock<i64> = OnceLock::new();
    *F.get_or_init(|| {
        let s = std::process::Command::new("date").arg("+%z").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let n: i64 = s.get(1..).and_then(|v| v.parse().ok()).unwrap_or(0);
        let seg = n / 100 * 3600 + n % 100 * 60;
        if s.starts_with('-') { -seg } else { seg }
    })
}

/// `dd/mm/aaaa hh:mm` de segundos desde 1970 (algoritmo dos dias civis de H. Hinnant).
pub fn data_de(seg: i64) -> String {
    let (dias, resto) = (seg.div_euclid(86400), seg.rem_euclid(86400));
    let z = dias + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!("{d:02}/{m:02}/{a} {:02}:{:02}", resto / 3600, resto % 3600 / 60)
}

pub fn data(t: Option<SystemTime>) -> String {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or("-".into(), |d| data_de(d.as_secs() as i64 + fuso()))
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into())
}

/// Uma pasta do usuário: a do `user-dirs.dirs` ou o primeiro nome que existir.
pub fn pasta_xdg(chave: &str, nomes: &[&str]) -> Option<PathBuf> {
    let h = home();
    let conf = std::fs::read_to_string(h.join(".config/user-dirs.dirs")).unwrap_or_default();
    let prefixo = format!("XDG_{chave}_DIR=");
    let do_xdg = conf.lines().find_map(|l| l.strip_prefix(prefixo.as_str())).map(|v| PathBuf::from(v.trim_matches('"').replace("$HOME", &h.to_string_lossy())));
    do_xdg.into_iter().chain(nomes.iter().map(|n| h.join(n))).find(|p| p.is_dir() && *p != h)
}

/// Decodifica PNG (pelo pelicano) ou JPEG (pelo crate `image`), reduzido para caber no painel.
pub fn carregar_imagem(p: &Path) -> Option<Image> {
    let bytes = std::fs::read(p).ok().filter(|b| b.len() < 40 << 20)?;
    match extensao(p).as_str() {
        "png" => Image::from_png(&bytes),
        "jpg" | "jpeg" => {
            let img = ::image::load_from_memory(&bytes).ok()?.thumbnail(900, 600).to_rgba8();
            let (w, h) = img.dimensions();
            Image::from_rgba(w, h, img.into_raw())
        }
        _ => None,
    }
}

/// As primeiras linhas, se for texto.
pub fn primeiras_linhas(p: &Path, n: usize) -> Option<String> {
    use std::io::Read;
    let mut buf = vec![0; 4096];
    let lidos = std::fs::File::open(p).ok()?.read(&mut buf).ok()?;
    buf.truncate(lidos);
    if buf.contains(&0) {
        return None;
    }
    let s = String::from_utf8_lossy(&buf);
    Some(s.lines().take(n).map(|l| l.replace('\t', "    ")).collect::<Vec<_>>().join("\n"))
}

/// (arquivos, pastas, bytes dos arquivos de dentro, primeira imagem).
// shortcut: o tamanho soma só o primeiro nível; percorrer tudo numa Task se precisar do total.
pub fn resumo_pasta(p: &Path) -> (usize, usize, u64, Option<PathBuf>) {
    let v = listar(p).unwrap_or_default();
    let pastas = v.iter().filter(|e| e.pasta).count();
    let bytes = v.iter().filter(|e| !e.pasta).map(|e| e.tamanho).sum();
    let img = v.iter().find(|e| !e.pasta && matches!(extensao(&e.caminho).as_str(), "png" | "jpg" | "jpeg")).map(|e| e.caminho.clone());
    (v.len() - pastas, pastas, bytes, img)
}

fn ler(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

const N: usize = 60;

fn empurrar(v: &mut Vec<f32>, x: f32) {
    if v.len() >= N {
        v.remove(0);
    }
    v.push(x);
}

/// As séries dos gráficos, uma amostra por segundo.
#[derive(Default)]
pub struct Monitor {
    pub cpu: Vec<f32>,
    pub mem: Vec<f32>,
    pub disco: Vec<f32>,
    /// KB/s (recebido + enviado).
    pub rede: Vec<f32>,
    pub cpu_nome: String,
    pub cpu_ghz: f32,
    pub mem_usada: u64,
    pub mem_total: u64,
    pub disco_nome: String,
    pub rede_rx: f64,
    pub rede_tx: f64,
    cpu_antes: Option<(u64, u64)>,
    disco_dev: String,
    disco_antes: Option<(u64, Instant)>,
    rede_antes: Option<(u64, u64, Instant)>,
}

impl Monitor {
    pub fn new() -> Monitor {
        let info = ler("/proc/cpuinfo");
        let cpu_nome = info.lines().find_map(|l| l.strip_prefix("model name")).and_then(|l| l.split(':').nth(1)).map(|s| {
            // "AMD Ryzen 7 7700 8-Core Processor" → "AMD Ryzen 7 7700"
            s.split_whitespace().take_while(|w| !w.contains("-Core") && *w != "w/" && *w != "with" && *w != "Processor" && *w != "CPU" && !w.starts_with('@')).collect::<Vec<_>>().join(" ")
        });
        // O disco principal: o maior que não é loop, ram, zram nem óptico.
        let disco = std::fs::read_dir("/sys/block")
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !["loop", "ram", "zram", "sr", "dm-", "md"].iter().any(|p| n.starts_with(p)))
            .map(|n| (ler(&format!("/sys/block/{n}/size")).trim().parse::<u64>().unwrap_or(0) * 512, n))
            .max();
        let (disco_nome, disco_dev) = match disco {
            Some((bytes, dev)) => {
                let modelo = ler(&format!("/sys/block/{dev}/device/model")).trim().to_string();
                let gb = bytes as f64 / 1e9;
                let tam = if gb >= 1000.0 { format!("{:.0}TB", gb / 1000.0) } else { format!("{gb:.0}GB") };
                let modelo = if modelo.is_empty() { if dev.starts_with("nvme") { "NVMe".into() } else { dev.clone() } } else { modelo };
                (format!("{} {tam}", modelo.split_whitespace().next().unwrap_or("")), dev)
            }
            None => ("—".into(), String::new()),
        };
        let mut m = Monitor { cpu_nome: cpu_nome.unwrap_or_else(|| "CPU".into()), disco_nome, disco_dev, ..Default::default() };
        m.amostrar();
        m
    }

    pub fn amostrar(&mut self) {
        let agora = Instant::now();
        if let Some(l) = ler("/proc/stat").lines().next() {
            let c: Vec<u64> = l.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
            let (total, ocioso) = (c.iter().sum::<u64>(), c.get(3).copied().unwrap_or(0) + c.get(4).copied().unwrap_or(0));
            if let Some((t0, o0)) = self.cpu_antes {
                let dt = total.saturating_sub(t0).max(1) as f32;
                empurrar(&mut self.cpu, (100.0 * (1.0 - ocioso.saturating_sub(o0) as f32 / dt)).clamp(0.0, 100.0));
            }
            self.cpu_antes = Some((total, ocioso));
        }
        let mhz: Vec<f32> = ler("/proc/cpuinfo").lines().filter(|l| l.starts_with("cpu MHz")).filter_map(|l| l.split(':').nth(1)?.trim().parse().ok()).collect();
        if !mhz.is_empty() {
            self.cpu_ghz = mhz.iter().sum::<f32>() / mhz.len() as f32 / 1000.0;
        }
        let mem = ler("/proc/meminfo");
        let campo = |k: &str| mem.lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()).unwrap_or(0) * 1024;
        (self.mem_total, self.mem_usada) = (campo("MemTotal:"), campo("MemTotal:").saturating_sub(campo("MemAvailable:")));
        empurrar(&mut self.mem, self.mem_usada as f32 / self.mem_total.max(1) as f32 * 100.0);
        // Disco: o tempo em que ele esteve ocupado (io_ticks, ms) sobre o tempo que passou.
        let ticks = ler("/proc/diskstats").lines().find_map(|l| {
            let c: Vec<&str> = l.split_whitespace().collect();
            (c.get(2) == Some(&self.disco_dev.as_str())).then(|| c.get(12)?.parse::<u64>().ok()).flatten()
        });
        if let Some(t) = ticks {
            if let Some((t0, i0)) = self.disco_antes {
                let ms = agora.duration_since(i0).as_secs_f32() * 1000.0;
                empurrar(&mut self.disco, (t.saturating_sub(t0) as f32 / ms.max(1.0) * 100.0).clamp(0.0, 100.0));
            }
            self.disco_antes = Some((t, agora));
        }
        let (rx, tx) = ler("/proc/net/dev").lines().skip(2).filter(|l| !l.trim_start().starts_with("lo:")).fold((0u64, 0u64), |(rx, tx), l| {
            let c: Vec<u64> = l.split(':').nth(1).unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
            (rx + c.first().unwrap_or(&0), tx + c.get(8).unwrap_or(&0))
        });
        if let Some((r0, t0, i0)) = self.rede_antes {
            let s = agora.duration_since(i0).as_secs_f64().max(0.001);
            (self.rede_rx, self.rede_tx) = (rx.saturating_sub(r0) as f64 / s / 1024.0, tx.saturating_sub(t0) as f64 / s / 1024.0);
            empurrar(&mut self.rede, (self.rede_rx + self.rede_tx) as f32);
        }
        self.rede_antes = Some((rx, tx, agora));
    }
}

/// Os processos que mais usam memória: (pid, nome, bytes).
pub fn processos(n: usize) -> Vec<(u32, String, u64)> {
    let mut v: Vec<(u32, String, u64)> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let s = ler(&format!("/proc/{pid}/status"));
            let nome = s.lines().find_map(|l| l.strip_prefix("Name:"))?.trim().to_string();
            let rss = s.lines().find_map(|l| l.strip_prefix("VmRSS:"))?.split_whitespace().next()?.parse::<u64>().ok()? * 1024;
            Some((pid, nome, rss))
        })
        .collect();
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v.truncate(n);
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn datas_e_tamanhos() {
        assert_eq!(data_de(0), "01/01/1970 00:00");
        assert_eq!(data_de(1_700_000_000), "14/11/2023 22:13");
        assert_eq!(data_de(951_782_400), "29/02/2000 00:00");
        assert_eq!(tamanho_humano(512), "512 B");
        assert_eq!(tamanho_humano(3277), "3.2 KB");
    }
}
