//! A tela do FeatherWorks Launcher refeita no pelicano, só a interface: os botões não fazem
//! nada, só os da janela (arrastar, redimensionar, minimizar, maximizar, fechar).
//! `cargo run --example launcher`; `-- --capturar l.png` só gera o PNG.
use pelicano::column;
use pelicano::*;
use std::cell::RefCell;
use std::collections::HashMap;

// As medidas e cores do launcher original (src/main.rs e o tema escuro do feather-ui).
const MARGEM: f32 = 16.0;
const ALTURA_CABECALHO: f32 = 98.0;
const ALTURA_RODAPE: f32 = 44.0;
const RAIO_JANELA: f32 = 12.0;
const RAIO_VIDRO: f32 = 14.0;
const COL_VERSAO: f32 = 150.0;
const COL_ESTADO: f32 = 150.0;
const COL_DATA: f32 = 170.0;
const COL_ACOES: f32 = 230.0;
const LARGURA_BOTAO: f32 = COL_ACOES - 8.0 - 36.0;
const COR_LAUNCHER: &str = "#b8336a";

fn hex(s: &str) -> Color {
    Color::hex(s).unwrap_or(Color::BLACK)
}

fn texto() -> Color {
    hex("#e8eaed")
}
fn mudo() -> Color {
    hex("#9aa0a6")
}
fn borda() -> Color {
    hex("#3d4147")
}
fn borda_suave() -> Color {
    hex("#33363b")
}
fn painel() -> Color {
    hex("#2a2d32")
}
fn acento() -> Color {
    hex("#4fb3a9")
}
fn verde() -> Color {
    hex("#81c995")
}
fn laranja() -> Color {
    hex("#f6ae6b")
}
fn azul() -> Color {
    hex("#8ab4f8")
}

/// A cor do app clareada 25% para o tema escuro, como o `tom_app` do launcher.
fn tom_app(cor: &str) -> Color {
    let c = hex(cor);
    c.mix(Color::WHITE, 0.25)
}

#[derive(Debug, Clone)]
enum Msg {
    Nada,
    Arrastar,
    Maximizar,
    Minimizar,
    Fechar,
    Puxar(ResizeDirection),
    Amostrar,
}

#[derive(Clone, Copy)]
enum Estado {
    Atualizar { de: &'static str, para: &'static str },
    Instalado(&'static str),
    Baixando { pct: f32, total_mb: u32 },
    NaoInstalado { versao: &'static str, mb: u32 },
    EmBreve,
}

struct AppInfo {
    sufixo: &'static str,
    descricao: &'static str,
    etiquetas: &'static [&'static str],
    cor: &'static str,
    estado: Estado,
    data: Option<&'static str>,
}

const APPS: [AppInfo; 6] = [
    AppInfo { sufixo: "Sheet", descricao: "Planilhas e dados", etiquetas: &["Planilhas", "Tabela dinâmica", "OpenQuery", "Macros (JS)", "Gráficos"], cor: "#287846", estado: Estado::Atualizar { de: "0.1.3", para: "0.1.4" }, data: Some("19/10/2026") },
    AppInfo { sufixo: "Slide", descricao: "Apresentações", etiquetas: &["Slides", "Animações", "Temas", "Exportação"], cor: "#a87f0c", estado: Estado::Instalado("0.1.0"), data: Some("19/10/2026") },
    AppInfo { sufixo: "Docs", descricao: "Documentos de texto", etiquetas: &["Documentos", "Markdown", "LaTeX", "Whiteboard", "DOCX"], cor: "#236ac7", estado: Estado::Baixando { pct: 45.0, total_mb: 38 }, data: Some("18/10/2026") },
    AppInfo { sufixo: "PDF", descricao: "Leitura e organização de PDF", etiquetas: &["Editar", "Anotações", "Formulários", "OCR"], cor: "#c5352b", estado: Estado::NaoInstalado { versao: "0.0.9", mb: 22 }, data: Some("17/10/2026") },
    AppInfo { sufixo: "CAD", descricao: "Desenho técnico 2D e 3D", etiquetas: &["CAD 2D", "DXF", "Vetores", "MCP (IA)"], cor: "#0f7184", estado: Estado::EmBreve, data: None },
    AppInfo { sufixo: "Code", descricao: "Editor de código", etiquetas: &["IDE", "Ferramentas", "Integração", "Extensões"], cor: "#7048b8", estado: Estado::EmBreve, data: None },
];

// ---------- Ícones (Tabler) e a pena ----------

const ICONE_CONFIG: &str = r#"<path d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 0 0 1.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 0 0-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 0 0-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 0 0-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 0 0-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 0 0 1.066-2.573c-.94-1.543.826-3.31 2.37-2.37 1 .608 2.296.07 2.572-1.065z"/><path d="M9 12a3 3 0 1 0 6 0a3 3 0 0 0-6 0"/>"#;
const ICONE_REFRESH: &str = r#"<path d="M20 11a8.1 8.1 0 0 0-15.5-2m-.5-4v4h4"/><path d="M4 13a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4"/>"#;
const ICONE_LISTA: &str = r#"<path d="M9 6h11M9 12h11M9 18h11M5 6v.01M5 12v.01M5 18v.01"/>"#;
const ICONE_GRADE: &str = r#"<path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"/>"#;
const ICONE_BAIXAR: &str = r#"<path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2M7 11l5 5 5-5M12 4v12"/>"#;
const ICONE_MAIS: &str = r#"<circle cx="5" cy="12" r="1"/><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/>"#;
const ICONE_BUSCA: &str = r#"<circle cx="10" cy="10" r="7"/><path d="M21 21l-6-6"/>"#;
const ICONE_CADEADO: &str = r#"<path d="M5 13a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2zM11 16a1 1 0 1 0 2 0 1 1 0 0 0-2 0M8 11V7a4 4 0 1 1 8 0v4"/>"#;
const ICONE_RELOGIO: &str = r#"<path d="M3 12a9 9 0 1 0 18 0 9 9 0 0 0-18 0M12 7v5l3 3"/>"#;
const ICONE_PACOTE: &str = r#"<path d="M12 3l8 4.5v9L12 21l-8-4.5v-9L12 3M12 12l8-4.5M12 12v9M12 12L4 7.5"/>"#;
const ICONE_GITHUB: &str = r#"<path d="M9 19c-4.3 1.4-4.3-2.5-6-3m12 5v-3.5c0-1 .1-1.4-.5-2 2.8-.3 5.5-1.4 5.5-6a4.6 4.6 0 0 0-1.3-3.2 4.2 4.2 0 0 0-.1-3.2s-1.1-.3-3.5 1.3a12.3 12.3 0 0 0-6.2 0C6.5 2.8 5.4 3.1 5.4 3.1a4.2 4.2 0 0 0-.1 3.2A4.6 4.6 0 0 0 4 9.5c0 4.6 2.7 5.7 5.5 6-.6.6-.6 1.2-.5 2V21"/>"#;
const ICONE_PLAY: &str = r#"<path d="M7 4v16l13-8z"/>"#;
const ICONE_PAUSA: &str = r#"<path d="M6 5h4v14H6zM14 5h4v14h-4z"/>"#;
const ICONE_X: &str = r#"<path d="M18 6L6 18M6 6l12 12"/>"#;
const ICONE_MINIMIZAR: &str = r#"<path d="M5 12h14"/>"#;
const ICONE_MAXIMIZAR: &str = r#"<rect x="5" y="5" width="14" height="14" rx="1"/>"#;

thread_local! {
    // Cada SVG é analisado uma vez; o mesmo `Svg` também reaproveita o cache de rasterização.
    static SVGS: RefCell<HashMap<String, Svg>> = RefCell::new(HashMap::new());
}

fn svg_cache(chave: &str, fonte: impl FnOnce() -> String) -> Svg {
    SVGS.with(|c| c.borrow_mut().entry(chave.to_string()).or_insert_with(|| Svg::from_bytes(fonte().as_bytes()).expect("SVG embutido válido")).clone())
}

fn traco(caminho: &'static str, tamanho: f32, cor: Color) -> SvgView {
    let s = svg_cache(caminho, || format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{caminho}</svg>"##));
    svg(s).width(tamanho).height(tamanho).color(cor)
}

fn cheio(caminho: &'static str, tamanho: f32, cor: Color) -> SvgView {
    let s = svg_cache(&format!("cheio{caminho}"), || format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#000" stroke="#000" stroke-width="2" stroke-linejoin="round">{caminho}</svg>"##));
    svg(s).width(tamanho).height(tamanho).color(cor)
}

/// A pena da marca na cor do app, sem o quadrado de fundo (como o `pena_na_cor` do launcher).
fn pena(cor: &'static str, tamanho: f32) -> SvgView {
    let s = svg_cache(&format!("pena{cor}"), || {
        let clara = hex(cor).mix(Color::WHITE, 0.4);
        let b = |v: f32| (v * 255.0).round() as u8;
        let mut p = include_str!("assets/pena.svg").replace("#287846", cor).replace("#72aa86", &format!("#{:02x}{:02x}{:02x}", b(clara.r), b(clara.g), b(clara.b)));
        if let Some(i) = p.find("<rect id=\"fundo-arredondado\"") {
            let fim = p[i..].find("/>").map_or(i, |f| i + f + 2);
            p.replace_range(i..fim, "");
        }
        p
    });
    svg(s).width(tamanho).height(tamanho)
}

// ---------- Estilos do launcher ----------

/// Vidro: desfoca a foto de fundo de verdade (o original recortava a foto à mão nessas medidas).
fn vidro(alfa: f32, raio: f32) -> Style {
    let base = Color::rgb8(17, 21, 31);
    let claro = Color { r: base.r + 0.06, g: base.g + 0.06, b: base.b + 0.08, a: alfa };
    Style::default()
        .background(Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, claro), (1.0, base.alpha(alfa))] })
        .border(Border::rounded(raio).color(Color::WHITE.alpha(0.12), 1.0))
        .shadow(Shadow { color: Color::BLACK.alpha(0.45), offset: Point::new(0.0, 8.0), blur: 28.0 })
        .text_color(texto())
        .backdrop_blur(20.0)
}

fn destaque(_: &Theme, s: Status) -> Style {
    let a = hex(COR_LAUNCHER);
    let (topo, baixo) = match s {
        Status::Hovered => (a.mix(Color::WHITE, 0.12), a),
        Status::Pressed => (a.mix(Color::BLACK, 0.15), a.mix(Color::BLACK, 0.15)),
        Status::Disabled => (a.alpha(0.4), a.alpha(0.4)),
        _ => (a, a.mix(Color::BLACK, 0.14)),
    };
    Style::default()
        .background(Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, topo), (1.0, baixo)] })
        .border(Border::rounded(8.0).color(Color::WHITE.alpha(0.25), 1.0))
        .shadow(Shadow { color: a.alpha(if s == Status::Hovered { 0.55 } else { 0.35 }), offset: Point::new(0.0, 2.0), blur: 14.0 })
        .text_color(Color::WHITE)
}

fn botao_vidro(_: &Theme, s: Status) -> Style {
    let fundo = match s {
        Status::Hovered => 0.12,
        Status::Pressed => 0.16,
        Status::Disabled => 0.02,
        _ => 0.05,
    };
    Style::default().background(Color::WHITE.alpha(fundo)).border(Border::rounded(8.0).color(Color::WHITE.alpha(0.14), 1.0)).text_color(if s == Status::Disabled { mudo() } else { texto() })
}

fn pilula(ativo: bool) -> impl Fn(&Theme, Status) -> Style {
    move |_, s| {
        let sobre = matches!(s, Status::Hovered | Status::Pressed);
        let bg = if ativo { acento().alpha(0.16) } else { painel().alpha(if sobre { 0.75 } else { 0.5 }) };
        Style::default().background(bg).text_color(texto()).border(Border::rounded(10.0).color(if ativo { acento() } else { borda() }, if ativo { 1.5 } else { 1.0 }))
    }
}

fn controle(fechar: bool) -> impl Fn(&Theme, Status) -> Style {
    move |_, s| {
        let sobre = matches!(s, Status::Hovered | Status::Pressed);
        let bg = sobre.then(|| if fechar { Color::rgb8(0xe8, 0x11, 0x23) } else { texto().alpha(0.1) });
        Style { background: bg.map(Into::into), border: Border::rounded(8.0), ..Default::default() }
    }
}

fn caixa(cor: Color, raio: f32, contorno: Option<Color>) -> impl Fn(&Theme) -> Style {
    move |_| {
        let b = Border::rounded(raio);
        Style::default().background(cor).border(match contorno {
            Some(c) => b.color(c, 1.0),
            None => b,
        })
    }
}

fn ponto<'a>(cor: Color) -> Element<'a, Msg> {
    container(Space::new(8.0, 8.0)).style(caixa(cor, 4.0, None)).into()
}

// ---------- A tela ----------

struct Launcher {
    fundo: Image,
    busca: String,
    /// RAM e CPU medidas de verdade, a cada segundo.
    recursos: String,
    cpu_antes: Option<(u64, u64)>,
}

impl Launcher {
    fn barra(&self) -> Element<'_, Msg> {
        let nome = row![
            pena(COR_LAUNCHER, 28.0),
            text("FeatherWorks").size(16).font(Font::SEMIBOLD),
            text("Launcher").size(15).color(mudo()),
            horizontal_space(),
            button(traco(ICONE_CONFIG, 18.0, texto())).on_press(Msg::Nada).padding(7.0).style(controle(false)),
        ]
        .spacing(10)
        .align(Align::Center)
        .padding([0u16, 14])
        .width(Length::FILL)
        .height(50.0);
        let janela = |icone, m, fechar| button(container(traco(icone, 15.0, mudo())).center()).on_press(m).width(40.0).height(32.0).padding(0.0).style(controle(fechar));
        row![
            mouse_area(nome).on_press(Msg::Arrastar).on_double_click(Msg::Maximizar),
            row![janela(ICONE_MINIMIZAR, Msg::Minimizar, false), janela(ICONE_MAXIMIZAR, Msg::Maximizar, false), janela(ICONE_X, Msg::Fechar, true)].spacing(4).padding(Padding { right: 8.0, ..Padding::ZERO }),
        ]
        .align(Align::Center)
        .into()
    }

    fn faixa(&self) -> Element<'_, Msg> {
        let aba = |rotulo: &'static str, n: usize, ativo: bool, alerta: bool| {
            let cor_n = if alerta { Color::WHITE } else { texto() };
            let contagem = container(text(n.to_string()).size(11).font(Font::BOLD).color(cor_n)).padding([1u16, 7]).style(caixa(if alerta { hex("#b45309") } else { texto().alpha(0.12) }, 9.0, None));
            button(row![text(rotulo).size(13), contagem].spacing(8).align(Align::Center)).on_press(Msg::Nada).padding([7u16, 14]).style(pilula(ativo))
        };
        let modo = |icone, ativo: bool, dica: &'static str| tooltip(button(traco(icone, 16.0, texto())).on_press(Msg::Nada).padding([6u16, 8]).style(pilula(ativo)), dica, TooltipPosition::Bottom);
        let busca = container(
            row![traco(ICONE_BUSCA, 15.0, mudo()), text_input("Buscar aplicativo…", &self.busca).size(13).padding(0.0).style(|t, _| Style::default().text_color(t.text))].spacing(8).align(Align::Center),
        )
        .padding([0u16, 12])
        .width(240.0)
        .height(34.0)
        .align_y(Align::Center)
        .style(caixa(painel().alpha(0.6), 8.0, Some(borda())));
        row![
            aba("Todos", 6, true, false),
            aba("Instalados", 2, false, false),
            aba("Atualizações", 1, false, true),
            aba("Não instalados", 4, false, false),
            horizontal_space(),
            row![modo(ICONE_LISTA, true, "Lista"), modo(ICONE_GRADE, false, "Grade")].spacing(2).padding(Padding { right: 8.0, ..Padding::ZERO }),
            busca,
            button(row![traco(ICONE_REFRESH, 15.0, texto()), text("Verificar").size(13).font(Font::SEMIBOLD)].spacing(6).align(Align::Center)).on_press(Msg::Nada).padding([8u16, 14]).style(pilula(false)),
            button(text("Atualizar tudo (1)").size(13).font(Font::SEMIBOLD)).on_press(Msg::Nada).padding([8u16, 14]).style(destaque),
        ]
        .spacing(8)
        .align(Align::Center)
        .padding(Padding { top: 4.0, right: 16.0, bottom: 12.0, left: 16.0 })
        .width(Length::FILL)
        .into()
    }

    fn nome(app: &AppInfo) -> Element<'static, Msg> {
        let em_breve = matches!(app.estado, Estado::EmBreve);
        let chip = |s: String| -> Element<'static, Msg> { container(text(s).size(11).color(texto())).padding([3u16, 9]).style(caixa(texto().alpha(0.06), 6.0, Some(borda()))).into() };
        let mostradas = if app.etiquetas.len() > 4 { 3 } else { app.etiquetas.len() };
        let mut descricao = row![text(app.descricao).size(13).color(mudo())].spacing(8).align(Align::Center).wrap();
        for e in &app.etiquetas[..mostradas] {
            descricao = descricao.push(chip(e.to_string()));
        }
        if app.etiquetas.len() > mostradas {
            descricao = descricao.push(chip(format!("+{}", app.etiquetas.len() - mostradas)));
        }
        column![
            row![text("Feather").size(17).font(Font::BOLD).color(if em_breve { mudo() } else { texto() }), text(app.sufixo).size(17).font(Font::BOLD).color(tom_app(app.cor))],
            descricao,
        ]
        .spacing(5)
        .width(Length::FILL)
        .into()
    }

    fn versao(app: &AppInfo) -> Element<'static, Msg> {
        let novidades = || button(text("Ver novidades ›").size(12).color(azul())).on_press(Msg::Nada).padding(0.0).style(|_, _| Style::default());
        let v = |s: &'static str| text(s).size(14);
        let col = match app.estado {
            Estado::Atualizar { de, para } => column![row![v(de).color(mudo()), v("→").color(mudo()), v(para).color(laranja())].spacing(6), novidades()],
            Estado::Instalado(x) => column![v(x), novidades()],
            Estado::Baixando { .. } => column![v("0.1.2"), novidades()],
            Estado::NaoInstalado { versao, .. } => column![v(versao), novidades()],
            Estado::EmBreve => column![v("—").color(mudo()), text("Em desenvolvimento").size(12).color(mudo())],
        };
        col.spacing(4).width(COL_VERSAO).into()
    }

    fn estado(app: &AppInfo) -> Element<'static, Msg> {
        let (rotulo, cor) = match app.estado {
            Estado::Atualizar { .. } => ("Atualização disponível", laranja()),
            Estado::Instalado(_) => ("Instalado", verde()),
            Estado::Baixando { .. } => ("Baixando…", azul()),
            Estado::NaoInstalado { .. } => ("Não instalado", mudo()),
            Estado::EmBreve => {
                return container(row![traco(ICONE_RELOGIO, 15.0, texto()), text("Em breve").size(13).font(Font::SEMIBOLD)].spacing(8).align(Align::Center)).padding([6u16, 14]).style(caixa(texto().alpha(0.06), 16.0, Some(borda()))).into();
            }
        };
        container(row![ponto(cor), text(rotulo).size(13).font(Font::SEMIBOLD).color(cor)].spacing(8).align(Align::Center)).padding([6u16, 14]).style(caixa(cor.alpha(0.18), 16.0, Some(cor.alpha(0.35)))).into()
    }

    fn data(app: &AppInfo) -> Element<'static, Msg> {
        match app.data {
            Some(d) if !matches!(app.estado, Estado::EmBreve) => text(d).size(13).into(),
            _ => text("Ainda sem versão publicada").size(12).color(mudo()).into(),
        }
    }

    fn acoes(app: &AppInfo) -> Element<'static, Msg> {
        let quadrado = |d: Element<'static, Msg>| button(container(d).center()).on_press(Msg::Nada).width(36.0).height(36.0).padding(0.0).style(botao_vidro);
        let primario = |d: Element<'static, Msg>, w: f32| button(container(d).center()).on_press(Msg::Nada).width(w).height(36.0).padding(0.0).style(destaque);
        let mais = || quadrado(traco(ICONE_MAIS, 16.0, texto()).into());
        let vao = || Space::new(36.0, 36.0);
        let linha = match app.estado {
            Estado::Atualizar { .. } => row![quadrado(cheio(ICONE_PLAY, 14.0, texto()).into()), primario(text("Atualizar").size(13).font(Font::SEMIBOLD).into(), LARGURA_BOTAO - 44.0), mais()],
            Estado::Instalado(_) => row![primario(row![cheio(ICONE_PLAY, 15.0, Color::WHITE), text("Abrir").size(15).font(Font::SEMIBOLD)].spacing(8).align(Align::Center).into(), LARGURA_BOTAO), mais()],
            Estado::Baixando { pct, total_mb } => row![
                column![text(format!("{pct:.0}% de {total_mb} MB")).size(12).color(mudo()), progress_bar(0.0..=100.0, pct).height(6.0).color(hex("#287846"))].spacing(6).width(Length::FILL),
                quadrado(cheio(ICONE_PAUSA, 14.0, texto()).into()),
                quadrado(traco(ICONE_X, 16.0, texto()).into()),
            ],
            Estado::NaoInstalado { mb, .. } => row![primario(row![traco(ICONE_BAIXAR, 16.0, Color::WHITE), text(format!("Instalar · {mb} MB")).size(14).font(Font::SEMIBOLD)].spacing(8).align(Align::Center).into(), LARGURA_BOTAO), vao()],
            Estado::EmBreve => row![
                button(container(row![traco(ICONE_CADEADO, 15.0, mudo()), text("Indisponível").size(13).color(mudo())].spacing(8).align(Align::Center)).center()).width(LARGURA_BOTAO).height(36.0).padding(0.0).style(botao_vidro),
                vao(),
            ],
        };
        container(linha.spacing(8).align(Align::Center)).width(COL_ACOES).align_x(Align::End).into()
    }

    fn lista(&self) -> Element<'_, Msg> {
        let cab = |s: &'static str| text(s).size(11).font(Font::BOLD).color(mudo());
        let cabecalho = row![
            cab("APLICATIVO").width(Length::FILL),
            cab("VERSÃO").width(COL_VERSAO),
            cab("ESTADO").width(COL_ESTADO),
            cab("ÚLTIMA ATUALIZAÇÃO").width(COL_DATA),
            container(cab("AÇÕES")).width(COL_ACOES).align_x(Align::End),
        ]
        .spacing(16)
        .padding([12u16, 20]);
        let mut col = column![cabecalho];
        for app in &APPS {
            let fina = container(Space::new(Length::FILL, 1.0)).width(Length::FILL).style(caixa(borda_suave(), 0.0, None));
            let linha = row![
                row![pena(app.cor, 48.0), Self::nome(app)].spacing(16).align(Align::Center).width(Length::FILL),
                Self::versao(app),
                container(Self::estado(app)).width(COL_ESTADO),
                container(Self::data(app)).width(COL_DATA),
                Self::acoes(app),
            ]
            .spacing(16)
            .padding([14u16, 20])
            .align(Align::Center);
            col = col.push(fina).push(linha);
        }
        let cartao = container(col).width(Length::FILL).style(|_| vidro(0.94, 12.0).backdrop_blur(0.0));
        scrollable(container(cartao).padding([8u16, 12])).into()
    }

    fn rodape(&self) -> Element<'_, Msg> {
        let m = |s: &'static str| text(s).size(12).color(mudo());
        let sep = || container(Space::new(1.0, 14.0)).style(caixa(borda(), 0.0, None));
        container(
            row![
                row![ponto(hex("#287846")), m("Instalando para Linux"), m("~/.local")].spacing(8).align(Align::Center),
                sep(),
                row![traco(ICONE_PACOTE, 14.0, mudo()), m("Cada pacote é conferido pelo SHA256 da release")].spacing(8).align(Align::Center),
                horizontal_space(),
                text(&self.recursos).size(12).color(mudo()),
                sep(),
                row![traco(ICONE_GITHUB, 14.0, mudo()), m("github.com/feather-works")].spacing(8).align(Align::Center),
                sep(),
                m("Launcher 0.1.3"),
            ]
            .spacing(14)
            .align(Align::Center),
        )
        .padding([0u16, 20])
        .height(ALTURA_RODAPE)
        .width(Length::FILL)
        .align_y(Align::Center)
        .style(|_| vidro(0.6, RAIO_VIDRO))
        .into()
    }

    /// Bordas da direita, de baixo e o canto: puxar redimensiona (sem moldura do sistema).
    fn alcas(&self) -> Element<'_, Msg> {
        let alca = |w: Length, h: Length, d: ResizeDirection, i: CursorIcon| mouse_area(Space::new(w, h)).on_press(Msg::Puxar(d)).cursor_icon(i);
        stack![
            container(alca(Length::Fixed(6.0), Length::FILL, ResizeDirection::East, CursorIcon::EResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End),
            container(alca(Length::FILL, Length::Fixed(6.0), ResizeDirection::South, CursorIcon::SResize)).width(Length::FILL).height(Length::FILL).align_y(Align::End),
            container(alca(Length::Fixed(16.0), Length::Fixed(16.0), ResizeDirection::SouthEast, CursorIcon::SeResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End),
        ]
        .into()
    }
}

impl App for Launcher {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Nada => Task::none(),
            Msg::Amostrar => {
                let agora = tempos_de_cpu();
                let cpu = match (agora, self.cpu_antes) {
                    (Some((p, t)), Some((p0, t0))) => format!(" · CPU {:.1}%", p.saturating_sub(p0) as f32 / t.saturating_sub(t0).max(1) as f32 * 100.0).replace('.', ","),
                    _ => String::new(),
                };
                self.cpu_antes = agora;
                self.recursos = format!("RAM {} MB{cpu}", rss_kb().unwrap_or(0) / 1024);
                Task::none()
            }
            Msg::Arrastar => Task::window(WindowCmd::Drag),
            Msg::Maximizar => Task::window(WindowCmd::ToggleMaximize),
            Msg::Minimizar => Task::window(WindowCmd::Minimize),
            Msg::Fechar => Task::window(WindowCmd::Close),
            Msg::Puxar(d) => Task::window(WindowCmd::DragResize(d)),
        }
    }

    fn theme(&self) -> Theme {
        Theme { background: hex("#181a1d"), surface: painel(), text: texto(), muted: mudo(), primary: acento(), border: borda(), ..Theme::dark() }
    }

    fn view(&self) -> Element<'_, Msg> {
        let cabecalho = container(column![self.barra(), self.faixa()]).height(ALTURA_CABECALHO).width(Length::FILL).style(|_| vidro(0.6, RAIO_VIDRO));
        let corpo = column![cabecalho, self.lista(), self.rodape()].spacing(12).padding(MARGEM).width(Length::FILL).height(Length::FILL);
        let fundo = image(self.fundo.clone()).content_fit(ContentFit::Cover).width(Length::FILL).height(Length::FILL);
        // A foto e tudo por cima ficam dentro dos cantos da janela.
        let janela = container(stack![fundo, corpo, self.alcas()])
            .width(Length::FILL)
            .height(Length::FILL)
            .clip(true)
            .style(|_| Style::default().border(Border::rounded(RAIO_JANELA).color(borda(), 1.0)).text_color(texto()));
        janela.into()
    }
}

fn fundo() -> Image {
    let img = ::image::load_from_memory(include_bytes!("assets/fundo.jpg")).expect("fundo.jpg embutido").to_rgba8();
    let (w, h) = img.dimensions();
    Image::from_rgba(w, h, img.into_raw()).expect("tamanho confere")
}

fn main() -> Result<(), Error> {
    let app = || Launcher { fundo: fundo(), busca: String::new(), recursos: String::new(), cpu_antes: None };
    let args: Vec<String> = std::env::args().collect();
    // `--medir`: tempo de um quadro inteiro (layout + desenho), mudando a largura a cada vez como num arrasto.
    if args.iter().any(|a| a == "--medir") {
        let (a, mut r) = (app(), Renderer::new());
        for i in 0..8 {
            let t = std::time::Instant::now();
            let (size, tema, st) = (Size::new(1440.0 + if args.iter().any(|a| a == "--fixo") { 0.0 } else { i as f32 * 7.0 }, 860.0), a.theme(), States::default());
            let el = a.view();
            r.begin(size, 1.0, Color::TRANSPARENT);
            let n = layout(&el, size, &mut r, &st, &tema);
            draw(&el, &n, &mut r, &st, &tema, None, false);
            println!("quadro {i}: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
        // O que cada movimento do mouse custa mesmo sem redesenhar: montar a view e o layout.
        let (size, tema, st) = (Size::new(1440.0, 860.0), a.theme(), States::default());
        let t = std::time::Instant::now();
        for _ in 0..100 {
            let el = a.view();
            layout(&el, size, &mut r, &st, &tema);
        }
        println!("view + layout: {:.2} ms", t.elapsed().as_secs_f64() * 10.0);
        let cpu = || tempos_de_cpu().map_or(0, |c| c.0);
        // Mouse andando sem mudar o hover: o evento (view + layout) e o quadro gravado e comparado, sem pintar.
        let t = std::time::Instant::now();
        for i in 0..100 {
            let el = a.view();
            layout(&el, size, &mut r, &st, &tema);
            let el = a.view();
            let n = layout(&el, size, &mut r, &st, &tema);
            r.record();
            draw(&el, &n, &mut r, &st, &tema, Some(Point::new(5.0 + (i % 2) as f32, 5.0)), false);
            let _ = r.changes();
        }
        println!("mouse sem mudança: {:.2} ms", t.elapsed().as_secs_f64() * 10.0);
        // Mouse varrendo a janela na diagonal, 4 px por evento: o pipeline todo (gravar, comparar, pintar regiões).
        let (t, c0, mut pintados) = (std::time::Instant::now(), cpu(), 0);
        for i in 0..300 {
            let p = Some(Point::new(i as f32 * 4.8, i as f32 * 2.8));
            let el = a.view();
            let n = layout(&el, size, &mut r, &st, &tema);
            r.record();
            draw(&el, &n, &mut r, &st, &tema, p, false);
            if let Changes::Regions(rs) = r.changes() {
                for x in rs {
                    pintados += 1;
                    if std::env::var_os("REGIOES").is_some() { println!("  {i} {x:?}"); }
                    r.begin_parcial(size, 1.0, Color::TRANSPARENT, x);
                    draw(&el, &n, &mut r, &st, &tema, p, false);
                    r.end();
                }
            }
        }
        println!("varrer com o mouse: {:.2} ms por evento, CPU {:.2} ms, {pintados} regiões em 300", t.elapsed().as_secs_f64() * 1000.0 / 300.0, (cpu() - c0) as f64 / 30.0);
        // Um hover numa linha da lista: só a faixa dela é redesenhada. Tempo e CPU (todas as threads).
        let (t, c0) = (std::time::Instant::now(), cpu());
        for _ in 0..100 {
            let el = a.view();
            r.begin_parcial(size, 1.0, Color::TRANSPARENT, Rect { x: 0.0, y: 200.0, width: 1440.0, height: 90.0 });
            let n = layout(&el, size, &mut r, &st, &tema);
            draw(&el, &n, &mut r, &st, &tema, None, false);
            r.end();
        }
        println!("hover parcial: {:.2} ms, CPU {} ms", t.elapsed().as_secs_f64() * 10.0, (cpu() - c0) / 10 * 10 / 10);
        // Quadro cheio: tempo e CPU (todas as threads).
        let (t, c0) = (std::time::Instant::now(), cpu());
        for _ in 0..100 {
            let el = a.view();
            r.begin(size, 1.0, Color::TRANSPARENT);
            let n = layout(&el, size, &mut r, &st, &tema);
            draw(&el, &n, &mut r, &st, &tema, None, false);
        }
        println!("quadro cheio: {:.2} ms, CPU {} ms", t.elapsed().as_secs_f64() * 10.0, (cpu() - c0) / 10);
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let a = app();
        let png = capture(&a.view(), Size::new(1440.0, 860.0), 1.0, &Theme { background: Color::TRANSPARENT, ..a.theme() }, &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("launcher.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    let settings = Settings {
        title: "FeatherWorks Launcher".into(),
        size: Size::new(1440.0, 860.0),
        min_size: Some(Size::new(960.0, 520.0)),
        decorations: false,
        transparent: true,
        app_id: Some("featherlauncher".into()),
        debug: true,
        ..Default::default()
    };
    // shortcut: o relógio ocupa uma thread do pool dormindo; trocar por um timer do runtime quando existir.
    let relogio = futures::stream::unfold((), |_| async {
        std::thread::sleep(std::time::Duration::from_secs(1));
        Some(((), ()))
    });
    run(settings, move || (app(), Task::batch([Task::done(Msg::Amostrar), Task::run(relogio, |_| Msg::Amostrar)])))
}

/// RSS do processo em KiB (Linux: `/proc/self/status`).
fn rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status").ok()?.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse().ok()
}

/// (CPU do processo, CPU da máquina), em ticks do `/proc`.
// shortcut: só Linux, como no launcher original.
fn tempos_de_cpu() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let campos: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let processo = campos.get(11)?.parse::<u64>().ok()? + campos.get(12)?.parse::<u64>().ok()?;
    let total = std::fs::read_to_string("/proc/stat").ok()?.lines().next()?.split_whitespace().skip(1).filter_map(|v| v.parse::<u64>().ok()).sum();
    Some((processo, total))
}
