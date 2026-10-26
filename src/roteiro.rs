//! Demonstração gravada: `PELICANO_ROTEIRO=demo.txt` faz a janela principal seguir um roteiro
//! com um cursor desenhado (vai até o botão, clica, digita), e `PELICANO_GRAVAR=demo.mp4` grava
//! os quadros pelo ffmpeg. Um passo por linha, `#` comenta:
//!
//! ```text
//! esperar 500
//! mover "Salvar" 800      # até o widget com esse rótulo (leitor de tela), em 800 ms
//! mover 320 180           # ou até x y lógicos (700 ms)
//! clicar
//! duplo                   # duplo clique
//! apertar                 # aperta o botão (arrastar: apertar, mover, soltar)
//! soltar
//! digitar Olá mundo
//! tecla Ctrl+D            # Enter, Tab, Escape, Backspace, Delete, setas, Home, End, PageUp…
//! rolar -120
//! sair                    # fecha o app (senão a gravação para no fim e a janela continua)
//! ```
use crate::{Color, Easing, Event, Key, Modifiers, MouseButton, NamedKey, Path, PathBuilder, Point, Renderer, Stroke};
use std::collections::VecDeque;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Alvo {
    Ponto(Point),
    Rotulo(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Passo {
    Mover(Alvo, Duration),
    Apertar,
    Soltar,
    Esperar(Duration),
    Tecla(Key, Option<String>, Modifiers),
    Rolar(f32),
    Sair,
}

/// Quanto o roteiro espera um rótulo aparecer (a janela ainda abrindo, uma tela carregando).
const PACIENCIA: Duration = Duration::from_secs(5);

pub(crate) fn ler(texto: &str) -> Result<Vec<Passo>, String> {
    let mut passos = Vec::new();
    for (n, linha) in texto.lines().enumerate() {
        // No `digitar` o `#` é texto.
        let comenta = if linha.trim_start().starts_with("digitar ") { None } else { linha.find(" #") };
        let linha = match comenta.or(linha.trim_start().starts_with('#').then_some(0)) {
            Some(i) => &linha[..i],
            None => linha,
        }
        .trim();
        if linha.is_empty() {
            continue;
        }
        let (cmd, resto) = linha.split_once(' ').unwrap_or((linha, ""));
        let resto = resto.trim();
        let erro = |o: &str| format!("roteiro, linha {}: {o}: {linha}", n + 1);
        let ms = |s: &str| s.parse::<u64>().map(Duration::from_millis).map_err(|_| erro("esperava milissegundos"));
        match cmd {
            "esperar" => passos.push(Passo::Esperar(ms(resto)?)),
            "mover" => {
                let (alvo, tempo) = if let Some(r) = resto.strip_prefix('"') {
                    let (rotulo, depois) = r.split_once('"').ok_or_else(|| erro("falta fechar as aspas"))?;
                    (Alvo::Rotulo(rotulo.to_owned()), depois.trim())
                } else {
                    let mut p = resto.split_whitespace();
                    let mut num = || p.next().and_then(|s| s.parse::<f32>().ok()).ok_or_else(|| erro("esperava x y ou \"rótulo\""));
                    let (x, y) = (num()?, num()?);
                    (Alvo::Ponto(Point::new(x, y)), resto.split_whitespace().nth(2).unwrap_or(""))
                };
                let tempo = if tempo.is_empty() { Duration::from_millis(700) } else { ms(tempo)? };
                passos.push(Passo::Mover(alvo, tempo));
            }
            "clicar" => passos.extend([Passo::Apertar, Passo::Esperar(Duration::from_millis(90)), Passo::Soltar, Passo::Esperar(Duration::from_millis(150))]),
            "duplo" => passos.extend([
                Passo::Apertar, Passo::Esperar(Duration::from_millis(40)), Passo::Soltar, Passo::Esperar(Duration::from_millis(60)),
                Passo::Apertar, Passo::Esperar(Duration::from_millis(40)), Passo::Soltar, Passo::Esperar(Duration::from_millis(150)),
            ]),
            "apertar" => passos.extend([Passo::Apertar, Passo::Esperar(Duration::from_millis(90))]),
            "soltar" => passos.extend([Passo::Soltar, Passo::Esperar(Duration::from_millis(150))]),
            "digitar" => {
                for c in resto.chars() {
                    let s = c.to_string();
                    passos.push(Passo::Tecla(Key::Character(s.as_str().into()), Some(s), Modifiers::empty()));
                    passos.push(Passo::Esperar(Duration::from_millis(60)));
                }
            }
            "tecla" => {
                let (mods, nome) = resto.rsplit_once('+').unwrap_or(("", resto));
                let mut m = Modifiers::empty();
                for p in mods.split('+').filter(|p| !p.is_empty()) {
                    m |= match p.to_lowercase().as_str() {
                        "ctrl" => Modifiers::CONTROL,
                        "shift" => Modifiers::SHIFT,
                        "alt" => Modifiers::ALT,
                        "super" => Modifiers::SUPER,
                        _ => return Err(erro("modificador desconhecido")),
                    };
                }
                let nomeada = match nome {
                    "Enter" => Some(NamedKey::Enter),
                    "Tab" => Some(NamedKey::Tab),
                    "Escape" => Some(NamedKey::Escape),
                    "Backspace" => Some(NamedKey::Backspace),
                    "Delete" => Some(NamedKey::Delete),
                    "Space" => Some(NamedKey::Space),
                    "ArrowUp" => Some(NamedKey::ArrowUp),
                    "ArrowDown" => Some(NamedKey::ArrowDown),
                    "ArrowLeft" => Some(NamedKey::ArrowLeft),
                    "ArrowRight" => Some(NamedKey::ArrowRight),
                    "Home" => Some(NamedKey::Home),
                    "End" => Some(NamedKey::End),
                    "PageUp" => Some(NamedKey::PageUp),
                    "PageDown" => Some(NamedKey::PageDown),
                    "F11" => Some(NamedKey::F11),
                    _ => None,
                };
                let (key, texto) = match nomeada {
                    Some(k) => (Key::Named(k), matches!(k, NamedKey::Enter | NamedKey::Space | NamedKey::Tab).then(|| match k {
                        NamedKey::Enter => "\r".to_owned(),
                        NamedKey::Tab => "\t".to_owned(),
                        _ => " ".to_owned(),
                    })),
                    None if nome.chars().count() == 1 => {
                        // Como o winit: com Shift a letra vem maiúscula, com Ctrl/Alt sozinhos, minúscula.
                        let s = if m.contains(Modifiers::SHIFT) { nome.to_uppercase() } else if m.is_empty() { nome.to_owned() } else { nome.to_lowercase() };
                        (Key::Character(s.as_str().into()), m.difference(Modifiers::SHIFT).is_empty().then_some(s))
                    }
                    None => return Err(erro("tecla desconhecida")),
                };
                passos.push(Passo::Tecla(key, texto, m));
                passos.push(Passo::Esperar(Duration::from_millis(120)));
            }
            "rolar" => passos.push(Passo::Rolar(resto.parse().map_err(|_| erro("esperava pixels"))?)),
            "sair" => passos.push(Passo::Sair),
            _ => return Err(erro("comando desconhecido")),
        }
    }
    Ok(passos)
}

pub(crate) struct Roteiro {
    passos: VecDeque<Passo>,
    /// O passo em andamento, quando começou e de onde o cursor saiu.
    atual: Option<(Passo, Instant, Point)>,
    pub(crate) cursor: Point,
    apertado: bool,
    clique: Option<Instant>,
}

/// O que um quadro do roteiro pede ao laço.
#[derive(Default)]
pub(crate) struct Saida {
    pub(crate) eventos: Vec<Event>,
    pub(crate) sair: bool,
    pub(crate) acabou: bool,
}

impl Roteiro {
    pub(crate) fn new(passos: Vec<Passo>) -> Self {
        Roteiro { passos: passos.into(), atual: None, cursor: Point::new(40.0, 40.0), apertado: false, clique: None }
    }

    /// Passos novos no fim (outro pedido chegou com este ainda andando).
    pub(crate) fn acrescentar(&mut self, passos: Vec<Passo>) {
        self.passos.extend(passos);
    }

    /// Avança até `agora`. `achar` dá o meio do widget com o rótulo (lógico), se ele está na tela.
    pub(crate) fn tick(&mut self, agora: Instant, achar: impl Fn(&str) -> Option<Point>) -> Saida {
        let mut s = Saida::default();
        loop {
            let Some((passo, inicio, de)) = self.atual.clone().or_else(|| self.passos.pop_front().map(|p| (p, agora, self.cursor))) else {
                s.acabou = true;
                return s;
            };
            let passou = agora.saturating_duration_since(inicio);
            let feito = match &passo {
                Passo::Esperar(d) => passou >= *d,
                Passo::Mover(alvo, d) => {
                    let ponto = match alvo {
                        Alvo::Ponto(p) => Some(*p),
                        Alvo::Rotulo(r) => achar(r),
                    };
                    match ponto {
                        Some(p) => {
                            let t = Easing::EaseInOut.apply(if d.is_zero() { 1.0 } else { passou.as_secs_f32() / d.as_secs_f32() });
                            self.cursor = Point::new(de.x + (p.x - de.x) * t, de.y + (p.y - de.y) * t);
                            s.eventos.push(Event::CursorMoved(self.cursor));
                            t >= 1.0
                        }
                        None if passou < PACIENCIA => false,
                        None => {
                            eprintln!("[roteiro] não achei {alvo:?} na tela; seguindo");
                            true
                        }
                    }
                }
                Passo::Apertar => {
                    self.apertado = true;
                    self.clique = Some(agora);
                    s.eventos.push(Event::MousePressed(MouseButton::Left));
                    true
                }
                Passo::Soltar => {
                    self.apertado = false;
                    s.eventos.push(Event::MouseReleased(MouseButton::Left));
                    true
                }
                Passo::Tecla(key, texto, m) => {
                    s.eventos.push(Event::Key { key: key.clone(), text: texto.clone(), modifiers: *m, pressed: true });
                    s.eventos.push(Event::Key { key: key.clone(), text: None, modifiers: *m, pressed: false });
                    true
                }
                Passo::Rolar(dy) => {
                    s.eventos.push(Event::Wheel { dx: 0.0, dy: *dy, modifiers: Modifiers::empty() });
                    true
                }
                Passo::Sair => {
                    s.sair = true;
                    true
                }
            };
            if !feito {
                self.atual = Some((passo, inicio, de));
                return s;
            }
            self.atual = None;
            // O mover espera o quadro seguinte para o widget ver o cursor antes do clique.
            if matches!(passo, Passo::Mover(..)) || s.sair {
                return s;
            }
        }
    }

    /// A seta por cima do quadro, e a onda do clique.
    pub(crate) fn desenhar(&self, r: &mut Renderer, agora: Instant) {
        let p = self.cursor;
        if let Some(t) = self.clique.map(|c| agora.saturating_duration_since(c).as_secs_f32() / 0.45).filter(|t| *t < 1.0) {
            if let Some(c) = Path::circle(p, 6.0 + 22.0 * t) {
                r.stroke_path(&c, &Stroke::new(3.0), Color::rgba(0.85, 0.47, 0.34, 0.8 * (1.0 - t)));
            }
        }
        let k = if self.apertado { 0.85 } else { 1.0 };
        let pt = |x: f32, y: f32| Point::new(p.x + x * k, p.y + y * k);
        let seta = Path::new(|b: &mut PathBuilder| {
            b.move_to(pt(0.0, 0.0)).line_to(pt(0.0, 17.0)).line_to(pt(4.5, 13.0)).line_to(pt(7.5, 20.0)).line_to(pt(10.5, 18.8)).line_to(pt(7.6, 12.0)).line_to(pt(13.0, 12.0)).close();
        });
        let laranja = Color::rgb8(0xD9, 0x77, 0x57);
        // Os cantos redondos vêm dos traços com junta redonda: o branco por fora, o laranja por cima.
        let redondo = |w: f32| Stroke::new(w).join(crate::LineJoin::Round);
        if let Some(seta) = seta {
            r.stroke_path(&seta, &redondo(4.0), Color::WHITE);
            r.fill_path(&seta, laranja, crate::FillRule::NonZero);
            r.stroke_path(&seta, &redondo(1.6), laranja);
        }
        let (nome, tam) = ("Claude", 11.0);
        let t = r.measure(nome, tam, crate::Font::SEMIBOLD, None);
        let caixa = crate::Rect::new(pt(12.0, 20.0), crate::Size::new(t.width + 12.0, t.height + 4.0));
        r.fill_quad(caixa, Some(&laranja.into()), crate::Border::rounded(caixa.height / 2.0).color(Color::WHITE, 1.5), None);
        r.draw_text(nome, tam, crate::Font::SEMIBOLD, Color::WHITE, Point::new(caixa.x + 6.0, caixa.y + 2.0), None);
    }
}

/// O ffmpeg recebendo os quadros crus pela entrada; o relógio de parede dá o tempo de cada um,
/// então quadro que demorou não acelera o vídeo.
pub(crate) struct Gravador {
    ffmpeg: Child,
    tamanho: (u32, u32),
}

impl Gravador {
    pub(crate) fn new(destino: &str, w: u32, h: u32) -> Result<Self, String> {
        let ffmpeg = Command::new("ffmpeg")
            .args(["-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{w}x{h}"), "-use_wallclock_as_timestamps", "1", "-i", "-"])
            .args(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p", "-fps_mode", "cfr", "-r", "30", destino])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("gravar precisa do ffmpeg: {e}"))?;
        Ok(Gravador { ffmpeg, tamanho: (w, h) })
    }

    /// `rgba` pré-multiplicado de uma janela opaca: já é a cor.
    pub(crate) fn quadro(&mut self, w: u32, h: u32, rgba: &[u8]) {
        // shortcut: quadro de outro tamanho (janela redimensionada no meio) é pulado; recomeçar o ffmpeg se precisar.
        if (w, h) != self.tamanho {
            return;
        }
        if let Some(e) = self.ffmpeg.stdin.as_mut().and_then(|i| i.write_all(rgba).err()) {
            eprintln!("[gravar] ffmpeg parou: {e}");
            self.ffmpeg.stdin = None;
        }
    }

    pub(crate) fn fim(mut self) {
        drop(self.ffmpeg.stdin.take());
        let _ = self.ffmpeg.wait();
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_roteiro() {
        let p = ler("# demo\nmover \"Salvar\" 300\nmover 10 20\nclicar # aperta\ntecla Ctrl+d\nsair").unwrap();
        assert_eq!(p[0], Passo::Mover(Alvo::Rotulo("Salvar".into()), Duration::from_millis(300)));
        assert_eq!(p[1], Passo::Mover(Alvo::Ponto(Point::new(10.0, 20.0)), Duration::from_millis(700)));
        assert_eq!(p[2], Passo::Apertar);
        assert!(p.contains(&Passo::Tecla(Key::Character("d".into()), None, Modifiers::CONTROL)));
        assert_eq!(p.last(), Some(&Passo::Sair));
        assert!(ler("voar 1").is_err());
        assert!(ler("tecla Ctrl+Shift+z").unwrap().contains(&Passo::Tecla(Key::Character("Z".into()), None, Modifiers::CONTROL | Modifiers::SHIFT)));
        assert_eq!(ler("digitar #1").unwrap().len(), 4);
        let arrastar = ler("apertar\nmover 5 5\nsoltar").unwrap();
        assert_eq!((arrastar[0].clone(), arrastar.iter().filter(|p| **p == Passo::Soltar).count()), (Passo::Apertar, 1));
        assert_eq!(ler("duplo").unwrap().iter().filter(|p| **p == Passo::Apertar).count(), 2);
        assert!(crate::validar_roteiro("clicar\nvoar").unwrap_err().contains("linha 2"));
    }

    #[test]
    fn acrescentar_continua_do_mesmo_cursor() {
        let mut r = Roteiro::new(ler("mover 30 0 0").unwrap());
        let t0 = Instant::now();
        r.tick(t0, |_| None);
        r.acrescentar(ler("clicar").unwrap());
        let s = r.tick(t0 + Duration::from_millis(1), |_| None);
        assert_eq!((r.cursor.x, s.eventos.last()), (30.0, Some(&Event::MousePressed(MouseButton::Left))));
    }

    #[test]
    fn mover_anda_e_clicar_aperta_depois() {
        let mut r = Roteiro::new(ler("mover 100 0 100\nclicar").unwrap());
        let t0 = Instant::now();
        r.tick(t0, |_| None);
        let s = r.tick(t0 + Duration::from_millis(50), |_| None);
        assert!(r.cursor.x > 0.0 && r.cursor.x < 100.0 && s.eventos.len() == 1);
        r.tick(t0 + Duration::from_millis(100), |_| None);
        assert_eq!(r.cursor.x, 100.0);
        let s = r.tick(t0 + Duration::from_millis(110), |_| None);
        assert_eq!(s.eventos, vec![Event::MousePressed(MouseButton::Left)]);
    }
}
