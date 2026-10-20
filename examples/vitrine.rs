//! Vitrine do pelicano: tudo o que a biblioteca oferece numa janela só, uma seção por vez.
//! Serve de modelo para começar um app: estado, mensagens, `update`, `view`, eventos,
//! tarefas, assinaturas e várias janelas.
//!
//! `cargo run --example vitrine --features terminal,highlighter`
//! `... -- --capturar <pasta>` gera um PNG de cada seção, sem abrir janela.
use pelicano::column;
use pelicano::*;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Secao {
    Layout,
    Texto,
    Controles,
    Editor,
    Rolagem,
    Mouse,
    Imagens,
    Desenho,
    Lousa,
    Animacao,
    Janela,
    Tarefas,
    Eventos,
    Terminal,
}

/// Seção, título na lateral, nome do arquivo da captura e a frase de cima.
const SECOES: [(Secao, &str, &str, &str); 14] = [
    (Secao::Layout, "Layout", "layout", "row, column, stack, container, Space, réguas, Length, Align e responsive."),
    (Secao::Texto, "Texto e fontes", "texto", "Tamanhos, pesos, itálico, altura da linha, fontes carregadas, rich_text e markdown."),
    (Secao::Controles, "Controles", "controles", "Botões, campos, marcações, slider, listas, abas, datas, cores e dicas."),
    (Secao::Editor, "Editor", "editor", "text_editor com realce de código, números de linha, invisíveis e o Content."),
    (Secao::Rolagem, "Rolagem", "rolagem", "scrollable nas três direções, com id, scroll_to, snap_to e on_scroll."),
    (Secao::Mouse, "Mouse", "mouse", "mouse_area: cliques, duplo clique, entrar e sair, posição, roda e ícones."),
    (Secao::Imagens, "Imagens e ícones", "imagens", "image, svg, ContentFit, cantos arredondados e rótulos de acessibilidade."),
    (Secao::Desenho, "Desenho vetorial", "desenho", "canvas com Path, Stroke, Transform, degradês, texto em curvas, camadas e recorte."),
    (Secao::Lousa, "Canvas interativo", "lousa", "program(): um canvas com estado próprio que recebe os eventos. Desenhe com o mouse."),
    (Secao::Animacao, "Animações", "animacao", "Transições de estilo, curvas, animate com hover, entrada, saída e repetição."),
    (Secao::Janela, "Janela", "janela", "Comandos da janela, arrastar, segunda janela e a alça crua."),
    (Secao::Tarefas, "Tarefas", "tarefas", "Task (done, perform, run, batch, abortable), área de transferência e Subscription."),
    (Secao::Eventos, "Eventos", "eventos", "Arquivos soltos, modificadores, escala, toque e atalhos globais, num registro."),
    (Secao::Terminal, "Terminal", "terminal", "Um shell de verdade no widget terminal."),
];

const FRUTAS: [&str; 12] = ["Abacaxi", "Banana", "Caju", "Damasco", "Figo", "Goiaba", "Jabuticaba", "Kiwi", "Laranja", "Manga", "Pitanga", "Uva"];
const PAISES: [&str; 10] = ["Angola", "Brasil", "Cabo Verde", "Guiné-Bissau", "Guiné Equatorial", "Macau", "Moçambique", "Portugal", "São Tomé e Príncipe", "Timor-Leste"];
const CURVAS: [&str; 6] = ["Linear", "Ease", "EaseIn", "EaseOut", "EaseInOut", "Elástica"];
const TEMAS_CODIGO: [&str; 5] = ["Solarized", "Mocha", "Ocean", "Eighties", "GitHub"];
/// Fontes para o `load_font`; a primeira que existir no sistema é carregada.
const FONTES: [(&str, &str); 3] = [
    ("/usr/share/fonts/truetype/comfortaa/Comfortaa-Regular.ttf", "Comfortaa"),
    ("/usr/share/fonts/truetype/ebgaramond/EBGaramond12-Regular.ttf", "EB Garamond 12"),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf", "DejaVu Serif"),
];
const SEGUNDA: WindowId = WindowId(1);

const CODIGO: &str = r#"use std::collections::HashMap;

/// Conta as palavras de um texto.
fn contar(texto: &str) -> HashMap<&str, usize> {
    let mut mapa = HashMap::new();
    for palavra in texto.split_whitespace() {
        *mapa.entry(palavra).or_insert(0) += 1;
    }
    mapa
}

fn main() {
    let total = contar("o rato roeu a roupa do rei de roma");
    println!("{} palavras", total.len());
}
"#;

const NOTA: &str = "# Notas\n\nEste editor tem **realce de Markdown** leve.\n\n- listas\n- `código`\n- *ênfase*\n";

const MARKDOWN: &str = r#"### Markdown
Parágrafos com **negrito**, *itálico*, ~~tachado~~ e `código`, e um [link](https://exemplo.org).

| Widget | Tipo |
|---|---|
| `text` | conteúdo |
| `button` | controle |

- [x] lista de tarefas
- [ ] ainda falta
"#;

const ICONE_ESTRELA: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 2l3 6.9 7.5.7-5.7 5 1.7 7.4L12 18l-6.5 4 1.7-7.4-5.7-5 7.5-.7z"/></svg>"#;
const ICONE_CORACAO: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 21s-7.5-4.6-9.6-9.1C.9 8.6 3 5 6.4 5c2.1 0 3.5 1.2 4.3 2.4h2.6C14.1 6.2 15.5 5 17.6 5 21 5 23.1 8.6 21.6 11.9 19.5 16.4 12 21 12 21z"/></svg>"#;
const ICONE_LIXO: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M9 3h6l1 2h4v2H4V5h4zM6 9h12l-1 12H7z"/></svg>"#;
const PAISAGEM: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 80"><rect width="120" height="80" fill="#9ad0f5"/><circle cx="92" cy="22" r="10" fill="#ffd25a"/><path d="M0 80 40 34l22 24 16-14 42 36z" fill="#3f7d58"/></svg>"##;

#[derive(Debug, Clone)]
enum Msg {
    Ir(Secao),
    Escuro(bool),
    Destaque(Color),
    Log(String),
    // Layout
    Extra(bool),
    // Texto
    Link(String),
    // Controles
    Nome(String),
    Enviar,
    FocarNome,
    Senha(String),
    Marcar(bool),
    Ligar(bool),
    Tamanho(u8),
    Volume(f32),
    Quantidade(f64),
    Fruta(&'static str),
    Pais(&'static str),
    Aba(usize),
    Data(Date),
    Periodo(Date, Date),
    // Editor
    TemaCodigo(&'static str),
    Numeros(bool),
    Quebra(bool),
    Invisiveis(bool),
    Editou,
    Salvar,
    Desfazer,
    Refazer,
    AcharFn,
    TrocarMapa,
    FocarEditor,
    // Rolagem
    Rolou(Viewport),
    RolarPara(f32),
    RolarFracao(f32),
    // Mouse
    Posicao(Point),
    // Lousa
    Traco(usize),
    Limpar,
    // Animação
    Curva(&'static str),
    Duracao(f32),
    Mover,
    Lista,
    Girar,
    // Janela
    Janela(WindowCmd),
    Segunda(WindowCmd),
    Titulo,
    NoTopo(bool),
    AbrirSegunda,
    SegundaFechou,
    Alca,
    // Tarefas
    Contador(contador::Msg),
    Esperar,
    Baixar,
    Progresso(u32),
    Cancelar,
    Lote,
    Area(String),
    Copiar,
    Colar,
    Colado(Option<String>),
    Relogio(bool),
    Tique,
    Pulsos(bool),
    Pulso(u32),
    // Eventos
    Evento(String),
    Solto(std::path::PathBuf),
    Pairando(bool),
    LimparLog,
    Simular,
    // Terminal
    Term(TerminalEvent),
    RodarLs,
    CopiarTela,
}

struct Vitrine {
    secao: Secao,
    escuro: bool,
    destaque: Option<Color>,
    log: Vec<String>,
    fonte: Option<&'static str>,
    extra: bool,
    nome: String,
    senha: String,
    marcado: bool,
    ligado: bool,
    tamanho: Option<u8>,
    volume: f32,
    quantidade: f64,
    fruta: Option<&'static str>,
    pais: Option<&'static str>,
    aba: usize,
    data: Option<Date>,
    periodo: Option<(Date, Date)>,
    codigo: Content,
    nota: Content,
    tema_codigo: &'static str,
    numeros: bool,
    quebra: bool,
    invisiveis: bool,
    rolagem: f32,
    posicao: Point,
    tracos: usize,
    geracao: u64,
    curva: &'static str,
    duracao: f32,
    movido: bool,
    lista: bool,
    girado: bool,
    titulos: u32,
    no_topo: bool,
    segunda: bool,
    contador: contador::Contador,
    baixando: Option<Handle>,
    progresso: u32,
    area: String,
    relogio: bool,
    agora: String,
    pulsos: bool,
    pulso: u32,
    pairando: bool,
    term: Option<Terminal>,
    erro_term: Option<String>,
    foto: Option<Image>,
    pixels: Option<Image>,
    png: Option<Image>,
    estrela: Option<Svg>,
    coracao: Option<Svg>,
    lixo: Option<Svg>,
    paisagem: Option<Svg>,
}

impl Vitrine {
    fn new() -> Self {
        // `load_font` registra a fonte para o app inteiro; depois é só pedir pelo nome.
        let fonte = FONTES.iter().find_map(|(caminho, nome)| std::fs::read(caminho).ok().map(|b| (b, *nome))).map(|(bytes, nome)| {
            load_font(bytes);
            nome
        });
        Vitrine {
            secao: Secao::Layout,
            escuro: system_dark(),
            destaque: None,
            log: vec!["Bem-vindo à vitrine do pelicano.".into()],
            fonte,
            extra: true,
            nome: String::new(),
            senha: String::new(),
            marcado: true,
            ligado: false,
            tamanho: Some(2),
            volume: 40.0,
            quantidade: 3.0,
            fruta: None,
            pais: None,
            aba: 0,
            data: None,
            periodo: None,
            codigo: Content::with_text(CODIGO),
            nota: Content::with_text(NOTA),
            tema_codigo: TEMAS_CODIGO[0],
            numeros: true,
            quebra: false,
            invisiveis: false,
            rolagem: 0.0,
            posicao: Point::ORIGIN,
            tracos: 0,
            geracao: 0,
            curva: "EaseOut",
            duracao: 400.0,
            movido: false,
            lista: true,
            girado: false,
            titulos: 0,
            no_topo: false,
            segunda: false,
            contador: contador::Contador::default(),
            baixando: None,
            progresso: 0,
            area: "Texto para copiar".into(),
            relogio: false,
            agora: String::new(),
            pulsos: false,
            pulso: 0,
            pairando: false,
            term: None,
            erro_term: None,
            foto: foto(),
            pixels: pixel_art(),
            png: png_capturado(),
            estrela: Svg::from_bytes(ICONE_ESTRELA),
            coracao: Svg::from_bytes(ICONE_CORACAO),
            lixo: Svg::from_bytes(ICONE_LIXO),
            paisagem: Svg::from_bytes(PAISAGEM),
        }
    }

    fn registrar(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn abrir_terminal(&mut self) {
        if self.term.is_none() && self.erro_term.is_none() {
            match Terminal::new(TerminalSettings::default()) {
                Ok(t) => self.term = Some(t),
                Err(e) => self.erro_term = Some(e.to_string()),
            }
        }
    }

    fn highlight(&self) -> Highlight {
        let i = TEMAS_CODIGO.iter().position(|t| *t == self.tema_codigo).unwrap_or(0);
        Highlight::ALL[i]
    }

    fn easing(&self) -> Easing {
        match self.curva {
            "Linear" => Easing::Linear,
            "EaseIn" => Easing::EaseIn,
            "EaseOut" => Easing::EaseOut,
            "EaseInOut" => Easing::EaseInOut,
            "Elástica" => Easing::CubicBezier(0.3, 1.6, 0.5, 1.0),
            _ => Easing::Ease,
        }
    }
}

impl App for Vitrine {
    type Msg = Msg;

    fn theme(&self) -> Theme {
        let base = if self.escuro { Theme::dark() } else { Theme::light() };
        Theme { primary: self.destaque.unwrap_or(base.primary), ..base }
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Ir(s) => {
                self.secao = s;
                if s == Secao::Terminal {
                    self.abrir_terminal();
                }
            }
            Msg::Escuro(b) => self.escuro = b,
            Msg::Destaque(c) => self.destaque = Some(c),
            Msg::Log(s) => self.registrar(s),
            Msg::Extra(b) => self.extra = b,
            Msg::Link(url) => self.registrar(format!("Link clicado: {url}")),
            Msg::Nome(n) => self.nome = n,
            Msg::Enviar => {
                let n = self.nome.clone();
                self.registrar(format!("Enviado: \"{n}\""));
            }
            Msg::FocarNome => return Task::focus("nome"),
            Msg::Senha(s) => self.senha = s,
            Msg::Marcar(b) => self.marcado = b,
            Msg::Ligar(b) => self.ligado = b,
            Msg::Tamanho(t) => self.tamanho = Some(t),
            Msg::Volume(v) => self.volume = v,
            Msg::Quantidade(q) => self.quantidade = q,
            Msg::Fruta(f) => self.fruta = Some(f),
            Msg::Pais(p) => self.pais = Some(p),
            Msg::Aba(a) => self.aba = a,
            Msg::Data(d) => self.data = Some(d),
            Msg::Periodo(a, b) => self.periodo = Some((a, b)),
            Msg::TemaCodigo(t) => self.tema_codigo = t,
            Msg::Numeros(b) => self.numeros = b,
            Msg::Quebra(b) => self.quebra = b,
            Msg::Invisiveis(b) => self.invisiveis = b,
            Msg::Editou => {}
            Msg::Salvar => self.registrar(format!("Ctrl+S no editor: {} linhas", self.codigo.line_count())),
            Msg::Desfazer => self.codigo.undo(),
            Msg::Refazer => self.codigo.redo(),
            Msg::AcharFn => {
                if !self.codigo.find_next("fn ", true) {
                    self.registrar("Nenhum \"fn\" daqui para frente");
                }
                return Task::focus("editor");
            }
            Msg::TrocarMapa => {
                let n = self.codigo.replace_all("mapa", "contagem", true);
                self.registrar(format!("{n} trocas de \"mapa\" por \"contagem\""));
            }
            Msg::FocarEditor => {
                self.codigo.go_to_line(1);
                return Task::focus("editor");
            }
            Msg::Rolou(v) => self.rolagem = v.relative().y,
            Msg::RolarPara(y) => return Task::scroll_to("lista", Point::new(0.0, y)),
            Msg::RolarFracao(f) => return Task::snap_to("lista", Point::new(0.0, f)),
            Msg::Posicao(p) => self.posicao = p,
            Msg::Traco(n) => {
                self.tracos += 1;
                self.registrar(format!("Traço {} com {n} pontos", self.tracos));
            }
            Msg::Limpar => {
                self.geracao += 1;
                self.tracos = 0;
            }
            Msg::Curva(c) => self.curva = c,
            Msg::Duracao(d) => self.duracao = d,
            Msg::Mover => self.movido = !self.movido,
            Msg::Lista => self.lista = !self.lista,
            Msg::Girar => self.girado = !self.girado,
            Msg::Janela(cmd) => return Task::window(cmd),
            Msg::Segunda(cmd) => return Task::window_of(SEGUNDA, cmd),
            Msg::Titulo => {
                self.titulos += 1;
                return Task::window(WindowCmd::Title(format!("Vitrine — título nº {}", self.titulos)));
            }
            Msg::NoTopo(b) => {
                self.no_topo = b;
                return Task::window(WindowCmd::Level(if b { WindowLevel::AlwaysOnTop } else { WindowLevel::Normal }));
            }
            Msg::AbrirSegunda => {
                self.segunda = true;
                let s = Settings { title: "Segunda janela".into(), size: Size::new(420.0, 260.0), decorations: false, ..Default::default() };
                return Task::open_window(SEGUNDA, s);
            }
            Msg::SegundaFechou => self.segunda = false,
            Msg::Alca => return Task::window_handle(WindowId::MAIN, |h| Msg::Log(format!("Alça crua: {h:?}"))),
            Msg::Contador(m) => return self.contador.update(m).map(Msg::Contador),
            Msg::Esperar => {
                self.registrar("Esperando 1 s fora da thread da interface…");
                return Task::perform(tokio::time::sleep(Duration::from_secs(1)), |_| Msg::Log("Task::perform terminou".into()));
            }
            Msg::Baixar => {
                let fluxo = futures::stream::unfold(0u32, |n| async move {
                    if n > 100 {
                        return None;
                    }
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    Some((n, n + 2))
                });
                let (task, handle) = Task::run(fluxo, Msg::Progresso).abortable();
                self.baixando = Some(handle);
                self.progresso = 0;
                return task;
            }
            Msg::Progresso(n) => {
                self.progresso = n;
                if n >= 100 {
                    self.baixando = None;
                    self.registrar("Download simulado concluído");
                }
            }
            Msg::Cancelar => {
                if let Some(h) = self.baixando.take() {
                    h.abort();
                    self.registrar(format!("Cancelado em {}%", self.progresso));
                }
            }
            Msg::Lote => return Task::batch([Task::done(Msg::Log("Primeira do lote".into())), Task::done(Msg::Log("Segunda do lote".into()))]),
            Msg::Area(s) => self.area = s,
            Msg::Copiar => {
                self.registrar("Copiado para a área de transferência");
                return Task::write_clipboard(self.area.clone());
            }
            Msg::Colar => return Task::read_clipboard(Msg::Colado),
            Msg::Colado(t) => match t {
                Some(t) => self.area = t,
                None => self.registrar("A área de transferência está vazia"),
            },
            Msg::Relogio(b) => {
                self.relogio = b;
                self.agora = hora_utc();
            }
            Msg::Tique => self.agora = hora_utc(),
            Msg::Pulsos(b) => self.pulsos = b,
            Msg::Pulso(n) => self.pulso = n,
            Msg::Evento(s) => self.registrar(s),
            Msg::Solto(p) => {
                self.pairando = false;
                self.registrar(format!("Arquivo solto: {}", p.display()));
            }
            Msg::Pairando(b) => self.pairando = b,
            Msg::LimparLog => self.log.clear(),
            // Passa pela árvore como se viesse do sistema: cai no on_unhandled_event e troca o tema.
            Msg::Simular => return Task::window_event(WindowId::MAIN, Event::Key { key: Key::Character("d".into()), text: None, modifiers: Modifiers::CONTROL, pressed: true }),
            Msg::Term(TerminalEvent::Title(t)) => self.registrar(format!("Título do terminal: {t}")),
            Msg::Term(TerminalEvent::Exit(c)) => {
                self.term = None;
                self.registrar(format!("O shell saiu ({c:?})"));
            }
            Msg::Term(_) => {}
            Msg::RodarLs => {
                if let Some(t) = &self.term {
                    t.write(&b"ls\n"[..]);
                }
            }
            Msg::CopiarTela => {
                if let Some(t) = &self.term {
                    return Task::write_clipboard(t.screen_text());
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let t = self.theme();
        let (_, titulo, _, frase) = SECOES.iter().find(|s| s.0 == self.secao).copied().unwrap_or(SECOES[0]);
        let corpo = match self.secao {
            Secao::Layout => self.layout(),
            Secao::Texto => self.texto(),
            Secao::Controles => self.controles(),
            Secao::Editor => self.editor(),
            Secao::Rolagem => self.rolagem(),
            Secao::Mouse => self.mouse(),
            Secao::Imagens => self.imagens(),
            Secao::Desenho => self.desenho(),
            Secao::Lousa => self.lousa(),
            Secao::Animacao => self.animacao(),
            Secao::Janela => self.janela(),
            Secao::Tarefas => self.tarefas(),
            Secao::Eventos => self.eventos(),
            Secao::Terminal => self.terminal(),
        };
        // Editor, lousa e terminal enchem a altura; o resto rola.
        let corpo: Element<'_, Msg> = match self.secao {
            Secao::Editor | Secao::Lousa | Secao::Terminal => container(corpo).padding(Padding { right: 24.0, bottom: 20.0, ..Padding::ZERO }).height(Length::FILL).into(),
            _ => scrollable(container(corpo).padding(Padding { right: 24.0, bottom: 24.0, ..Padding::ZERO })).into(),
        };
        let conteudo = column![text(titulo).size(24).font(Font::SEMIBOLD), text(frase).size(13).color(t.muted), Space::new(0.0, 8.0), corpo]
            .spacing(4)
            .padding(Padding { top: 24.0, left: 28.0, ..Padding::ZERO })
            .width(Length::FILL)
            .height(Length::FILL);

        let principal = row![self.lateral(), vertical_rule(1.0), conteudo].height(Length::FILL);
        column![principal, horizontal_rule(1.0), self.barra_status()].into()
    }

    fn view_window(&self, id: WindowId) -> Element<'_, Msg> {
        if id == SEGUNDA {
            self.segunda_janela()
        } else {
            self.view()
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let relogio = self.relogio.then(|| Subscription::every(Duration::from_secs(1)).map(|_| Msg::Tique));
        let pulsos = self.pulsos.then(|| {
            let fluxo = futures::stream::unfold(0u32, |n| async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Some((Msg::Pulso(n), n + 1))
            });
            Subscription::run_with_id("pulsos", fluxo)
        });
        let term = self.term.as_ref().map(|t| t.subscription().map(Msg::Term));
        Subscription::batch(relogio.into_iter().chain(pulsos).chain(term))
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        let s = match ev {
            Event::FileDropped(p) => return Some(Msg::Solto(p.clone())),
            Event::FileHovered(_) if !self.pairando => return Some(Msg::Pairando(true)),
            Event::FileHoverCancelled => return Some(Msg::Pairando(false)),
            Event::ModifiersChanged(m) => format!("Modificadores: {}", modificadores(*m)),
            Event::ScaleChanged(e) => format!("Escala da tela: {e}"),
            Event::Focused(f) => format!("Janela {}", if *f { "ganhou o foco" } else { "perdeu o foco" }),
            Event::Resized(s) => format!("Novo tamanho: {:.0} × {:.0}", s.width, s.height),
            Event::Moved(p) => format!("Janela movida para {:.0}, {:.0}", p.x, p.y),
            Event::FingerPressed { id, position } => format!("Dedo {id} encostou em {:.0}, {:.0}", position.x, position.y),
            Event::FingerMoved { id, position } => format!("Dedo {id} em {:.0}, {:.0}", position.x, position.y),
            Event::FingerLifted { id, .. } => format!("Dedo {id} soltou"),
            Event::FingerLost { id, .. } => format!("Dedo {id} cancelado pelo sistema"),
            _ => return None,
        };
        Some(Msg::Evento(s))
    }

    fn on_window_event(&self, id: WindowId, ev: &Event) -> Option<Msg> {
        match (id, ev) {
            (WindowId::MAIN, _) => self.on_event(ev),
            (_, Event::CloseRequested) => Some(Msg::SegundaFechou),
            _ => None,
        }
    }

    // Atalhos globais: não disparam enquanto se digita num campo.
    fn on_unhandled_event(&self, _id: WindowId, ev: &Event) -> Option<Msg> {
        match ev {
            Event::Key { key: Key::Named(NamedKey::F11), pressed: true, .. } => Some(Msg::Janela(WindowCmd::ToggleFullscreen)),
            Event::Key { key: Key::Character(c), modifiers, pressed: true, .. } if modifiers.control_key() && c.as_str() == "d" => Some(Msg::Escuro(!self.escuro)),
            _ => None,
        }
    }
}

// ---------- Moldura ----------

impl Vitrine {
    fn lateral(&self) -> Element<'_, Msg> {
        let t = self.theme();
        let itens = column(SECOES.iter().map(|&(s, nome, _, _)| {
            let ativo = s == self.secao;
            button(text(nome).size(14).width(Length::FILL))
                .width(Length::FILL)
                .padding([7u16, 12])
                .on_press(Msg::Ir(s))
                .style(move |t, st| if ativo { Style::default().background(t.primary.alpha(0.16)).text_color(t.primary).border(Border::rounded(6.0)) } else { button_text(t, st) })
                .into()
        }))
        .spacing(2);
        let tema = column![
            toggler("Tema escuro", self.escuro).on_toggle(Msg::Escuro),
            row![text("Destaque").size(13), horizontal_space(), color_picker(t.primary, Msg::Destaque)].align(Align::Center),
            text("Ctrl+D troca o tema · F11 tela cheia · F12 inspetor").size(11).color(t.muted),
        ]
        .spacing(10);
        container(column![text("pelicano").size(18).font(Font::BOLD), text("vitrine").size(12).color(t.muted), Space::new(0.0, 12.0), itens, vertical_space(), tema].spacing(2))
            .width(220.0)
            .height(Length::FILL)
            .padding(14.0)
            .style(|t| Style::default().background(t.surface))
            .into()
    }

    fn barra_status(&self) -> Element<'_, Msg> {
        let ultimo = self.log.last().cloned().unwrap_or_default();
        let relogio = self.relogio.then(|| text(format!("{} UTC", self.agora)).size(12).font(Font::MONO));
        container(row![text(ultimo).size(12).no_wrap(), horizontal_space()].push_maybe(relogio).spacing(12).align(Align::Center))
            .width(Length::FILL)
            .padding([6u16, 14])
            .clip(true)
            .style(|t| Style::default().background(t.surface).text_color(t.muted))
            .into()
    }

    /// Sem moldura do sistema: a barra é nossa, arrasta e maximiza no duplo clique.
    fn segunda_janela(&self) -> Element<'_, Msg> {
        let fechar = Element::from(button("✕").style(button_text).padding([2u16, 8]).on_press(Msg::Segunda(WindowCmd::Close))).access_label("Fechar");
        let barra = mouse_area(container(row![text("Segunda janela").size(13).font(Font::SEMIBOLD), horizontal_space(), fechar].align(Align::Center)).padding([6u16, 10]).width(Length::FILL).style(|t| Style::default().background(t.surface)))
            .on_press(Msg::Segunda(WindowCmd::Drag))
            .on_double_click(Msg::Segunda(WindowCmd::ToggleMaximize));
        let corpo = column![
            text("Cada janela tem o próprio foco, rolagem e hover, e divide o renderizador com as outras.").size(13),
            row![text(format!("O contador é o mesmo estado do app: {}", self.contador.valor)).size(13), horizontal_space(), button("+1").on_press(Msg::Contador(contador::Msg::Mais))].align(Align::Center),
        ]
        .spacing(12)
        .padding(16.0);
        container(column![barra, horizontal_rule(1.0), corpo]).height(Length::FILL).style(|t| Style::default().background(t.background).border(Border::default().color(t.border, 1.0))).into()
    }
}

/// Um cartão com título, no estilo neutro de todas as seções.
fn cartao<'a>(titulo: &'a str, conteudo: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(column![text(titulo).size(13).font(Font::SEMIBOLD), conteudo.into()].spacing(12))
        .padding(16.0)
        .width(Length::FILL)
        .style(|t| Style::default().background(t.surface).border(Border::rounded(10.0).color(t.border, 1.0)))
        .into()
}

/// Dois cartões lado a lado, dividindo a largura.
fn par<'a>(a: Element<'a, Msg>, b: Element<'a, Msg>) -> Element<'a, Msg> {
    row![a, b].spacing(16).into()
}

/// Uma caixinha colorida com rótulo, para mostrar o layout.
fn caixa<'a>(rotulo: impl Into<String>, largura: impl Into<Length>) -> Container<'a, Msg> {
    container(text(rotulo.into()).size(12)).padding(10.0).width(largura).align_x(Align::Center).align_y(Align::Center).style(|t| Style::default().background(t.primary.alpha(0.18)).border(Border::rounded(6.0).color(t.primary.alpha(0.5), 1.0)))
}

/// Texto pequeno e apagado, para explicações.
fn nota<'a>(s: impl Into<String>) -> Element<'a, Msg> {
    text(s.into()).size(12).color(Color::rgb8(128, 128, 136)).into()
}

// ---------- Seções ----------

impl Vitrine {
    fn layout(&self) -> Element<'_, Msg> {
        let proporcoes = column![
            row![caixa("Fill(1)", Length::Fill(1)), caixa("Fill(2)", Length::Fill(2)), caixa("Fixed 120", 120.0), caixa("Shrink", Length::Shrink)].spacing(8),
            nota("Os Fill dividem a sobra na proporção; Fixed e Shrink ficam com o tamanho deles."),
        ]
        .spacing(8);

        let alinhar = |x, y, r: &'static str| container(caixa(r, Length::Shrink)).width(Length::FILL).height(80.0).align_x(x).align_y(y).style(|t| Style::default().border(Border::rounded(6.0).color(t.border, 1.0)));
        let alinhamentos = row![alinhar(Align::Start, Align::Start, "Start"), alinhar(Align::Center, Align::Center, "Center"), alinhar(Align::End, Align::End, "End")].spacing(8);

        let em_linha = row![caixa("alto", 60.0).height(50.0), caixa("align", 60.0), caixa("Center", 70.0)].spacing(8).align(Align::Center);

        let caixas = row![
            container(container(text("margin 12").size(12)).margin(12.0).padding(8.0).style(|t| Style::default().background(t.primary.alpha(0.3)).border(Border::rounded(4.0)))).style(|t| Style::default().background(t.text.alpha(0.06)).border(Border::rounded(6.0))),
            container(text("max_width 140: o texto quebra dentro da caixa").size(12)).max_width(140.0).padding(8.0).style(|t| Style::default().border(Border::rounded(6.0).color(t.border, 1.0))),
            container(row![Space::new(60.0, 50.0), Space::new(60.0, 50.0)])
                .clip(true)
                .style(|_| Style::default().background(Background::Linear { angle: 0.6, stops: vec![(0.0, Color::rgb8(99, 102, 241)), (1.0, Color::rgb8(236, 72, 153))] }).border(Border { radius: [20.0, 4.0, 20.0, 4.0], ..Border::default() })),
            container(text("sombra").size(12)).padding(12.0).style(|t| Style::default().background(t.surface).border(Border::rounded(8.0)).shadow(Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 4.0), blur: 12.0 })),
        ]
        .spacing(16)
        .align(Align::Center);

        let pilha = stack![
            button(container("Botão (camada de baixo)").height(Length::FILL).align_y(Align::Center)).width(Length::FILL).height(Length::FILL).on_press(Msg::Log("Clique no botão por baixo".into())),
            row![
                horizontal_space(),
                opaque(container(text("opaque").size(12)).width(140.0).height(Length::FILL).align_x(Align::Center).align_y(Align::Center).style(|t| Style::default().background(t.surface.alpha(0.9)).border(Border::rounded(6.0)))),
            ]
            .height(Length::FILL),
        ]
        .height(56.0);

        let espacos = column![
            row![text("A"), Space::new(40.0, 0.0), text("B"), vertical_rule(2.0), text("C"), horizontal_space(), text("fim")].spacing(8).height(24.0).align(Align::Center),
            horizontal_rule(1.0),
            horizontal_rule(3.0).color(self.theme().primary),
        ]
        .spacing(10);

        let chips = row((1..=14).map(|i| caixa(format!("item {i}"), Length::Shrink).into())).spacing(8).wrap().push_maybe(self.extra.then(|| caixa("push_maybe", Length::Shrink)));
        let quebra = column![checkbox("Mostrar o item do push_maybe", self.extra).on_toggle(Msg::Extra), chips].spacing(10);

        let adapta = container(responsive(|s: Size| {
            let itens = || [caixa("um", Length::FILL).into(), caixa("dois", Length::FILL).into(), caixa(format!("{:.0} px de largura", s.width), Length::FILL).into()];
            if s.width > 420.0 {
                row(itens()).spacing(8).into()
            } else {
                column(itens()).spacing(4).into()
            }
        }))
        .height(110.0);

        column![
            cartao("row e Length", proporcoes),
            par(cartao("container: align_x e align_y", alinhamentos), cartao("row.align(Center)", em_linha)),
            cartao("container: margin, max_width, clip, cantos diferentes e sombra", caixas),
            par(cartao("stack e opaque", pilha), cartao("Space e réguas", espacos)),
            cartao("row.wrap() e push_maybe", quebra),
            cartao("responsive: linha em tela larga, coluna em estreita (redimensione a janela)", adapta),
        ]
        .spacing(16)
        .into()
    }

    fn texto(&self) -> Element<'_, Msg> {
        let t = self.theme();
        let tamanhos = row([12, 14, 18, 24, 32].map(|s| text(format!("{s} px")).size(s).into())).spacing(18).align(Align::End);
        let pesos = row([300, 400, 500, 600, 700, 800].map(|w| text(format!("Peso {w}")).font(Font::DEFAULT.weight(w)).into())).spacing(16);
        let carregada = match self.fonte {
            Some(nome) => text(format!("{nome}: carregada com load_font e pedida com Font::with_name")).size(16).font(Font::with_name(nome)),
            None => text("Nenhuma das fontes de FONTES existe neste sistema; load_font não foi chamado.").size(12).color(t.muted),
        };
        let familias = column![
            row![text("Itálico").font(Font::DEFAULT.italic()), text("Negrito itálico").font(Font::BOLD.italic()), text("Mono 0O1lI").font(Font::MONO), text("Serifada").font(Font::SERIF), text("Liberation Serif").font(Font::with_name("Liberation Serif"))].spacing(18),
            carregada,
        ]
        .spacing(10);
        let paragrafo = "O texto quebra sozinho na largura disponível. A altura da linha muda o respiro entre as linhas do parágrafo.";
        let alturas = row![
            column![nota("line_height(1.0)"), text(paragrafo).size(13).line_height(1.0)].spacing(4).width(Length::FILL),
            column![nota("line_height(1.8)"), text(paragrafo).size(13).line_height(1.8)].spacing(4).width(Length::FILL),
        ]
        .spacing(20);
        let alinhamento = column![
            text("align_x(Start)").width(Length::FILL),
            text("align_x(Center)").width(Length::FILL).align_x(Align::Center),
            text("align_x(End)").width(Length::FILL).align_x(Align::End),
            container(text("no_wrap: esta linha não quebra e é cortada pela caixa com clip, mesmo sendo bem mais comprida do que o espaço que cabe nela, de um lado ao outro do cartão").no_wrap().color(t.primary)).width(Length::FILL).clip(true),
        ]
        .spacing(4);
        let rico = rich_text([
            span("Um parágrafo de "),
            span("trechos").font(Font::BOLD),
            span(" com "),
            span("itálico").italic(),
            span(", "),
            span("sublinhado").underline(),
            span(", "),
            span("tachado").strikethrough(),
            span(", "),
            span(" código ").font(Font::MONO).background(t.text.alpha(0.1)),
            span(", "),
            span("cor").color(Color::rgb8(220, 80, 60)),
            span(", "),
            span("tamanho").size(20.0),
            span(" e um "),
            span("link clicável").link(Msg::Link("rich_text".into())),
            span(", quebrando linha juntos."),
        ])
        .size(15)
        .line_height(1.5);
        column![
            cartao("text().size", tamanhos),
            cartao("Font: pesos", pesos),
            cartao("Font: itálico, mono, serifa e família pelo nome", familias),
            cartao("line_height", alturas),
            cartao("align_x, color e no_wrap", alinhamento),
            cartao("rich_text e span", rico),
            cartao("markdown", markdown(MARKDOWN, 14.0, Msg::Link)),
        ]
        .spacing(16)
        .into()
    }

    fn controles(&self) -> Element<'_, Msg> {
        let t = self.theme();
        let contorno = |t: &Theme, s: Status| {
            let fundo = if s == Status::Hovered { t.primary.alpha(0.12) } else { Color::TRANSPARENT };
            Style::default().background(fundo).border(Border::rounded(999.0).color(t.primary, 1.5)).text_color(t.primary).transition(Transition::new(200).easing(Easing::EaseOut))
        };
        let botoes = row![
            button("Primário").on_press(Msg::Log("Primário".into())),
            button("Texto").style(button_text).on_press(Msg::Log("Texto".into())),
            button("Desabilitado"),
            button("on_press_maybe").on_press_maybe(self.marcado.then(|| Msg::Log("Habilitado pela caixa".into()))),
            button("Contorno com transição").style(contorno).padding([8u16, 16]).on_press(Msg::Log("Contorno".into())),
        ]
        .spacing(10)
        .align(Align::Center);

        let campos = column![
            row![text_input("Seu nome (Enter envia)", &self.nome).id("nome").on_input(Msg::Nome).on_submit(Msg::Enviar).width(Length::FILL), button("Focar o campo").style(button_text).on_press(Msg::FocarNome)].spacing(8).align(Align::Center),
            row![text_input("Senha", &self.senha).secure(true).on_input(Msg::Senha).width(Length::FILL), text_input("Mono, 16 px", &self.nome).font(Font::MONO).size(16).on_input(Msg::Nome).width(Length::FILL)].spacing(8),
        ]
        .spacing(8);

        let marcas = column![
            row![checkbox("Habilita o on_press_maybe", self.marcado).on_toggle(Msg::Marcar), toggler("Notificações", self.ligado).on_toggle(Msg::Ligar)].spacing(20),
            row![radio("Pequeno", 1, self.tamanho, Msg::Tamanho), radio("Médio", 2, self.tamanho, Msg::Tamanho), radio("Grande", 3, self.tamanho, Msg::Tamanho).size(18)].spacing(20),
            row![toggler("Cor própria", self.ligado).on_toggle(Msg::Ligar).color(Color::rgb8(34, 160, 90)), checkbox("Maior", self.marcado).on_toggle(Msg::Marcar).size(20)].spacing(20),
        ]
        .spacing(12);

        let valores = column![
            row![slider(0.0..=100.0, self.volume, Msg::Volume).step(5.0).width(Length::FILL), text(format!("{:.0}", self.volume)).size(13).width(30.0)].spacing(12).align(Align::Center),
            progress_bar(0.0..=100.0, self.volume).height(8.0),
            row![number_input(self.quantidade, 0.0..=10.0, 1.0, Msg::Quantidade), nota("de 0 a 10")].spacing(12).align(Align::Center),
        ]
        .spacing(12);

        let listas = column![
            pick_list(&FRUTAS[..], self.fruta, Msg::Fruta).placeholder("Fruta (menu de 160 px)").menu_height(160.0).width(Length::FILL),
            combo_box(&PAISES[..], "País (digite para filtrar)", self.pais.as_ref(), Msg::Pais).menu_height(180.0),
        ]
        .spacing(10);

        let datas = column![
            row![date_picker(self.data, Msg::Data).placeholder("Uma data"), nota(self.data.map_or("nenhuma".into(), |d| format!("dia da semana {}", d.weekday())))].spacing(12).align(Align::Center),
            row![date_range_picker(self.periodo.map(|p| p.0), self.periodo.map(|p| p.1), Msg::Periodo).placeholder("Um período"), nota(format!("hoje: {}", Date::today()))].spacing(12).align(Align::Center),
        ]
        .spacing(10);

        let dica = |pos, nome: &'static str| tooltip(button(nome).style(button_text), format!("Dica em {nome}"), pos);
        let dicas = row![
            dica(TooltipPosition::Top, "Top"),
            dica(TooltipPosition::Bottom, "Bottom"),
            dica(TooltipPosition::Left, "Left"),
            dica(TooltipPosition::Right, "Right"),
            tooltip(button("Estilo próprio").style(button_text), "Balão com style e padding", TooltipPosition::Top).padding([8u16, 14]).style(|t| Style::default().background(t.primary).text_color(t.on_primary).border(Border::rounded(8.0))),
        ]
        .spacing(8);

        let abas = ["Geral", "Avançado", "Sobre"];
        let cor = row![color_picker(t.primary, Msg::Destaque), text(to_hex(t.primary)).font(Font::MONO).size(13)].spacing(12).align(Align::Center);
        column![
            cartao("button", botoes),
            cartao("text_input: id + Task::focus, on_submit, secure, font", campos),
            par(cartao("checkbox, toggler e radio", marcas), cartao("slider, progress_bar e number_input", valores)),
            par(cartao("pick_list (menu_height) e combo_box", listas), cartao("tabs", column![tabs(&abas, self.aba, Msg::Aba), text(format!("Aba ativa: {}", abas[self.aba])).size(13)].spacing(10))),
            par(cartao("date_picker e date_range_picker", datas), cartao("color_picker (muda o destaque do tema)", cor)),
            cartao("tooltip: posições, style e padding", dicas),
        ]
        .spacing(16)
        .into()
    }

    fn editor(&self) -> Element<'_, Msg> {
        let (lin, col) = self.codigo.cursor();
        let barra = row![
            pick_list(&TEMAS_CODIGO[..], Some(self.tema_codigo), Msg::TemaCodigo).width(130.0),
            checkbox("Números", self.numeros).on_toggle(Msg::Numeros),
            checkbox("Quebra", self.quebra).on_toggle(Msg::Quebra),
            checkbox("Invisíveis", self.invisiveis).on_toggle(Msg::Invisiveis),
            button("Desfazer").style(button_text).on_press_maybe(self.codigo.can_undo().then_some(Msg::Desfazer)),
            button("Refazer").style(button_text).on_press_maybe(self.codigo.can_redo().then_some(Msg::Refazer)),
            button("Achar fn").style(button_text).on_press(Msg::AcharFn),
            button("mapa → contagem").style(button_text).on_press(Msg::TrocarMapa),
            button("Linha 1").style(button_text).on_press(Msg::FocarEditor),
        ]
        .spacing(8)
        .align(Align::Center)
        .wrap();
        let codigo = text_editor(&self.codigo)
            .id("editor")
            .highlight("rs", self.highlight())
            .line_numbers(self.numeros)
            .wrap(self.quebra)
            .invisibles(self.invisiveis)
            .font(Font::MONO)
            .size(13.0)
            .on_change(Msg::Editou)
            .on_key(|k, m| matches!(k, Key::Character(c) if m.control_key() && c.as_str() == "s").then_some(Msg::Salvar));
        let borda = |t: &Theme| Style::default().border(Border::rounded(8.0).color(t.border, 1.0));
        let md = text_editor(&self.nota).markdown(true).placeholder("Escreva em Markdown…").size(14.0);
        let info = nota(format!("Linha {lin}, coluna {col} · {} linhas · versão {} · Ctrl+S manda uma mensagem pelo on_key", self.codigo.line_count(), self.codigo.version()));
        column![barra, row![container(codigo).padding(1.0).width(Length::Fill(3)).height(Length::FILL).style(borda), container(md).padding(1.0).width(Length::Fill(2)).height(Length::FILL).style(borda)].spacing(12).height(Length::FILL), info]
            .spacing(10)
            .height(Length::FILL)
            .into()
    }

    fn rolagem(&self) -> Element<'_, Msg> {
        let itens = column((1..=60).map(|i| container(text(format!("Linha {i}")).size(13)).padding([6u16, 10]).width(Length::FILL).style(move |t| Style::default().background(if i % 2 == 0 { t.text.alpha(0.04) } else { Color::TRANSPARENT })).into()));
        let vertical = column![
            row![
                button("Topo").style(button_text).on_press(Msg::RolarPara(0.0)),
                button("Meio (snap_to)").style(button_text).on_press(Msg::RolarFracao(0.5)),
                button("Fim").style(button_text).on_press(Msg::RolarPara(f32::INFINITY)),
                horizontal_space(),
                text(format!("{:.0}%", self.rolagem * 100.0)).size(13).font(Font::MONO),
            ]
            .spacing(6)
            .align(Align::Center),
            progress_bar(0.0..=1.0, self.rolagem).height(3.0),
            container(scrollable(itens).id("lista").on_scroll(Msg::Rolou)).height(240.0).clip(true).style(|t| Style::default().border(Border::rounded(6.0).color(t.border, 1.0))),
        ]
        .spacing(8);
        let horizontal = scrollable(row((1..=30).map(|i| caixa(format!("cartão {i}"), 110.0).into())).spacing(8).padding(Padding { bottom: 12.0, ..Padding::ZERO })).direction(Direction::Horizontal).height(60.0);
        let grade = column((0..24).map(|y| row((0..24).map(|x| container(text(format!("{x},{y}")).size(11)).width(56.0).height(32.0).align_x(Align::Center).align_y(Align::Center).style(move |t| Style::default().background(t.primary.alpha(if (x + y) % 2 == 0 { 0.12 } else { 0.04 }))).into())).into()));
        let ambos = container(scrollable(grade).direction(Direction::Both)).height(278.0).clip(true).style(|t| Style::default().border(Border::rounded(6.0).color(t.border, 1.0)));
        column![
            par(cartao("Vertical, com id, Task::scroll_to / snap_to e on_scroll", vertical), cartao("Both: as duas barras (Shift + roda rola na horizontal)", ambos)),
            cartao("Horizontal", horizontal),
        ]
        .spacing(16)
        .into()
    }

    fn mouse(&self) -> Element<'_, Msg> {
        let log = |s: &str| Msg::Log(s.to_string());
        let area = mouse_area(
            container(column![text("Clique, clique com o direito, duplo clique e role a roda aqui dentro").size(14), text(format!("Cursor em {:.0}, {:.0}", self.posicao.x, self.posicao.y)).size(13).font(Font::MONO)].spacing(6).align(Align::Center))
                .width(Length::FILL)
                .height(180.0)
                .align_x(Align::Center).align_y(Align::Center)
                .style(|t| Style::default().background(t.primary.alpha(0.08)).border(Border::rounded(10.0).color(t.primary.alpha(0.4), 1.0))),
        )
        .on_press(log("mouse_area: on_press"))
        .on_release(log("mouse_area: on_release"))
        .on_right_press(log("mouse_area: on_right_press"))
        .on_double_click(log("mouse_area: on_double_click"))
        .on_enter(log("mouse_area: on_enter"))
        .on_exit(log("mouse_area: on_exit"))
        .on_move(Msg::Posicao)
        .on_scroll(|dx, dy, m| Msg::Log(format!("mouse_area: roda dx {dx:.1} dy {dy:.1}, modificadores: {}", modificadores(m))))
        .cursor_icon(CursorIcon::Crosshair);
        let icone = |i: CursorIcon, nome: &'static str| mouse_area(caixa(nome, Length::Shrink)).cursor_icon(i).into();
        let icones = row([
            icone(CursorIcon::Pointer, "Pointer"),
            icone(CursorIcon::Text, "Text"),
            icone(CursorIcon::Grab, "Grab"),
            icone(CursorIcon::Move, "Move"),
            icone(CursorIcon::NotAllowed, "NotAllowed"),
            icone(CursorIcon::Wait, "Wait"),
            icone(CursorIcon::EwResize, "EwResize"),
        ])
        .spacing(8)
        .wrap();
        column![cartao("Eventos (o último aparece na barra de baixo)", area), cartao("cursor_icon: passe o mouse", icones)].spacing(16).into()
    }

    fn imagens(&self) -> Element<'_, Msg> {
        let t = self.theme();
        let ajuste = |fit, nome: &'static str| -> Element<'_, Msg> {
            let img: Element<'_, Msg> = match &self.foto {
                Some(f) => image(f.clone()).width(150.0).height(110.0).content_fit(fit).radius(10.0).into(),
                None => text("sem imagem").into(),
            };
            column![container(img).style(|t| Style::default().border(Border::rounded(10.0).color(t.border, 1.0))), nota(nome)].spacing(6).align(Align::Center).into()
        };
        let fits = row![ajuste(ContentFit::Fill, "Fill"), ajuste(ContentFit::Contain, "Contain"), ajuste(ContentFit::Cover, "Cover")].spacing(16);
        let origens = row![]
            .push_maybe(self.png.clone().map(|i| column![image(i).width(120.0).height(80.0), nota("Image::from_png (de um capture)")].spacing(6)))
            .push_maybe(self.pixels.clone().map(|i| column![image(i).width(80.0).height(80.0), nota("from_rgba 8×8")].spacing(6)))
            .spacing(24);
        let icone = |s: &Option<Svg>, cor: Color, tam: f32| -> Element<'_, Msg> { s.clone().map_or_else(|| text("?").into(), |s| svg(s).width(tam).height(tam).color(cor).into()) };
        let paisagem: Element<'_, Msg> = self.paisagem.clone().map_or_else(|| text("?").into(), |s| svg(s).width(120.0).height(80.0).into());
        let svgs = row![icone(&self.estrela, t.primary, 32.0), icone(&self.coracao, Color::rgb8(220, 60, 90), 32.0), icone(&self.lixo, t.muted, 32.0), icone(&self.estrela, t.text, 18.0), paisagem].spacing(16).align(Align::Center);
        // Botões só de ícone: sem texto dentro, o leitor de tela precisa do access_label.
        let icone_botao = |s: &Option<Svg>, rotulo: &'static str| Element::from(button(icone(s, t.text, 18.0)).style(button_text).padding(8.0).on_press(Msg::Log(format!("{rotulo} (access_label)")))).access_label(rotulo);
        let acessivel = column![
            row![icone_botao(&self.estrela, "Favoritar"), icone_botao(&self.coracao, "Curtir"), icone_botao(&self.lixo, "Apagar")].spacing(4),
            nota("Cada botão de ícone tem access_label: é o nome que o Orca, NVDA ou VoiceOver fala. Textos, botões, campos e caixas já se descrevem sozinhos."),
        ]
        .spacing(8);
        column![cartao("image: ContentFit e radius", fits), par(cartao("Image de PNG e de pixels", origens), cartao("svg, com .color nos ícones", svgs)), cartao("Acessibilidade", acessivel)].spacing(16).into()
    }

    fn desenho(&self) -> Element<'_, Msg> {
        let resumo = estrela(Point::ORIGIN, 50.0).map_or(String::new(), |p| {
            let segs: Vec<Segment> = p.segments().collect();
            let retas = segs.iter().filter(|s| matches!(s, Segment::LineTo(_))).count();
            let b = p.bounds();
            format!("Path::segments: a estrela tem {} segmentos ({retas} LineTo); bounds {:.0} × {:.0}.", segs.len(), b.width, b.height)
        });
        let (foto, pixels, icone) = (self.foto.clone(), self.pixels.clone(), self.estrela.clone());

        let formas = canvas(Length::FILL, 200.0, |r, a, t| {
            let p = |x: f32, y: f32| Point::new(a.x + x, a.y + y);
            let degrade = Background::Linear { angle: 0.0, stops: vec![(0.0, t.primary), (1.0, t.primary.mix(Color::WHITE, 0.6))] };
            if let Some(onda) = Path::new(|b| {
                b.move_to(p(10.0, 150.0)).bezier_to(p(60.0, 40.0), p(110.0, 200.0), p(160.0, 90.0)).quadratic_to(p(190.0, 40.0), p(220.0, 110.0)).line_to(p(220.0, 190.0)).line_to(p(10.0, 190.0)).close();
            }) {
                r.fill_path(&onda, degrade, FillRule::NonZero);
            }
            // Pizza: um arco de 0 a 270° fechado no centro.
            if let Some(pizza) = Path::new(|b| {
                b.move_to(p(310.0, 100.0)).arc(p(310.0, 100.0), Point::new(60.0, 60.0), 0.0, 1.5 * std::f32::consts::PI).close();
            }) {
                r.fill_path(&pizza, Color::rgb8(245, 158, 11), FillRule::NonZero);
            }
            // EvenOdd deixa o miolo da estrela vazado.
            if let Some(e) = estrela(p(470.0, 105.0), 75.0) {
                r.fill_path(&e, Color::rgb8(34, 160, 90), FillRule::EvenOdd);
            }
            if let Some(el) = Path::new(|b| {
                b.ellipse(p(640.0, 100.0), Point::new(60.0, 36.0));
            }) {
                r.fill_path(&el, Background::Linear { angle: 1.57, stops: vec![(0.0, Color::rgb8(236, 72, 153)), (1.0, Color::rgb8(99, 102, 241))] }, FillRule::NonZero);
            }
        });

        let tracos = canvas(Length::FILL, 200.0, |r, a, t| {
            let p = |x: f32, y: f32| Point::new(a.x + x, a.y + y);
            for (i, cap) in [LineCap::Butt, LineCap::Round, LineCap::Square].into_iter().enumerate() {
                let y = 30.0 + i as f32 * 30.0;
                r.stroke_path(&Path::line(p(30.0, y), p(200.0, y)), &Stroke::new(12.0).cap(cap), t.primary);
            }
            for (i, join) in [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel].into_iter().enumerate() {
                let x = 260.0 + i as f32 * 90.0;
                if let Some(zz) = Path::polyline(&[p(x, 110.0), p(x + 30.0, 30.0), p(x + 60.0, 110.0)], false) {
                    r.stroke_path(&zz, &Stroke::new(10.0).join(join), t.text);
                }
            }
            if let Some(c) = Path::circle(p(620.0, 75.0), 45.0) {
                r.stroke_path(&c, &Stroke::new(3.0).dash(vec![10.0, 6.0]).cap(LineCap::Round), Color::rgb8(220, 60, 90));
            }
            let onda: Vec<Point> = (0..=100).map(|i| p(20.0 + i as f32 * 6.6, 160.0 + (i as f32 / 6.0).sin() * 20.0)).collect();
            r.stroke(&onda, 2.0, t.muted);
        });

        let transformacoes = canvas(Length::FILL, 200.0, |r, a, t| {
            let p = |x: f32, y: f32| Point::new(a.x + x, a.y + y);
            let centro = p(90.0, 100.0);
            for i in 0..6 {
                r.push_transform(Transform::rotate_around(i as f32 * 15.0, centro));
                if let Some(q) = Path::rect(Rect::new(p(50.0, 60.0), Size::new(80.0, 80.0))) {
                    r.stroke_path(&q, &Stroke::new(1.5), t.primary.alpha(0.3 + i as f32 * 0.12));
                }
                r.pop_transform();
            }
            // Texto como contorno: preenchido com degradê e com um fio em volta.
            if let Some(letras) = r.text_path("Pelicano", 56.0, Font::BOLD, p(200.0, 30.0), None) {
                r.fill_path(&letras, Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(99, 102, 241)), (1.0, Color::rgb8(236, 72, 153))] }, FillRule::NonZero);
                r.stroke_path(&letras, &Stroke::new(1.0), t.text);
                // A mesma forma, menor, com Path::transform.
                let encolhe = Transform::translate(-a.x - 200.0, -a.y - 30.0).then(Transform::scale(0.4, 0.4)).then(Transform::translate(a.x + 200.0, a.y + 120.0));
                if let Some(menor) = letras.transform(encolhe) {
                    r.fill_path(&menor, t.muted, FillRule::NonZero);
                }
            }
            let giro = Transform::rotate_around(-12.0, p(560.0, 120.0));
            r.push_transform(Transform::translate(-p(520.0, 120.0).x, -p(520.0, 120.0).y).then(Transform::from_row(1.0, 0.0, -0.4, 1.0, 0.0, 0.0)).then(Transform::translate(p(520.0, 120.0).x, p(520.0, 120.0).y)).then(giro));
            r.fill_text("fill_text inclinado", 18.0, Font::SEMIBOLD, t.text, p(500.0, 110.0));
            r.pop_transform();
            // apply e invert: ida e volta de um ponto.
            if let Some(v) = giro.invert().map(|i| i.apply(giro.apply(p(10.0, 10.0)))) {
                r.draw_text(&format!("invert(apply(p)) = {:.0}, {:.0}", v.x - a.x, v.y - a.y), 11.0, Font::MONO, t.muted, p(450.0, 175.0), None);
            }
        });

        let camadas = canvas(Length::FILL, 200.0, move |r, a, t| {
            let p = |x: f32, y: f32| Point::new(a.x + x, a.y + y);
            // Opacidade por desenho: onde se sobrepõem, o de baixo aparece.
            r.push_opacity(0.6);
            for (i, cor) in [Color::rgb8(239, 68, 68), Color::rgb8(34, 197, 94), Color::rgb8(59, 130, 246)].into_iter().enumerate() {
                if let Some(c) = Path::circle(p(50.0 + i as f32 * 30.0, 70.0 + (i % 2) as f32 * 30.0), 36.0) {
                    r.fill_path(&c, cor, FillRule::NonZero);
                }
            }
            r.pop_opacity();
            // Camada: o grupo inteiro girado, menor e translúcido.
            let area = Rect::new(p(180.0, 40.0), Size::new(120.0, 100.0));
            r.push_layer(area, 0.85, 12.0, 0.8);
            r.fill_quad(area, Some(&t.primary.into()), Border::rounded(12.0), None);
            r.draw_text("push_layer", 14.0, Font::SEMIBOLD, t.on_primary, p(200.0, 80.0), None);
            r.pop_layer();
            // Recorte com cantos: o círculo grande só aparece dentro.
            let janela = Rect::new(p(330.0, 40.0), Size::new(110.0, 110.0));
            r.push_clip(janela, [24.0; 4]);
            if let Some(c) = Path::circle(p(400.0, 110.0), 90.0) {
                r.fill_path(&c, Background::Linear { angle: 0.8, stops: vec![(0.0, Color::rgb8(245, 158, 11)), (1.0, Color::rgb8(220, 60, 90))] }, FillRule::NonZero);
            }
            r.pop_clip();
            if let Some(f) = &foto {
                r.draw_image(f, Rect::new(p(470.0, 30.0), Size::new(100.0, 70.0)), [10.0; 4]);
            }
            if let Some(px) = &pixels {
                r.draw_image_nearest(px, Rect::new(p(590.0, 30.0), Size::new(64.0, 64.0)));
            }
            if let Some(s) = &icone {
                r.draw_svg(s, Rect::new(p(670.0, 40.0), Size::new(40.0, 40.0)), Some(t.primary));
            }
            // blur desfoca o que já está ali: o miolo das listras.
            for i in 0..10 {
                r.fill_quad(Rect::new(p(470.0 + i as f32 * 16.0, 120.0), Size::new(8.0, 50.0)), Some(&t.text.into()), Border::default(), None);
            }
            r.blur(Rect::new(p(520.0, 115.0), Size::new(70.0, 60.0)), 6.0, [8.0; 4]);
            let medida = r.measure("measure", 13.0, Font::MONO, None);
            r.fill_quad(Rect::new(p(20.0, 160.0), Size::new(medida.width + 8.0, medida.height + 4.0)), None, Border::rounded(4.0).color(t.border, 1.0), None);
            r.draw_text("measure", 13.0, Font::MONO, t.text, p(24.0, 162.0), None);
        });

        let anel = row![
            Element::new(Anel { valor: self.volume / 100.0, ao_clicar: Msg::Volume(if self.volume >= 100.0 { 0.0 } else { self.volume + 10.0 }) }),
            column![slider(0.0..=100.0, self.volume, Msg::Volume).width(260.0), nota("Clique no anel para somar 10. Ele anda até o valor com cx.transition.")].spacing(8),
        ]
        .spacing(24)
        .align(Align::Center);

        let moldura = |c: Canvas<'static>| container(c).clip(true).style(|t| Style::default().background(t.background).border(Border::rounded(6.0).color(t.border, 1.0)));
        column![
            cartao("Path: bezier, quadrática, arco, elipse, EvenOdd e degradê", column![moldura(formas), nota(resumo)].spacing(8)),
            cartao("Stroke: cap, join, dash e stroke() de pontos", moldura(tracos)),
            cartao("Transform, text_path, Path::transform e fill_text", moldura(transformacoes)),
            cartao("push_opacity, push_layer, push_clip, imagens, svg, blur e measure", moldura(camadas)),
            cartao("Widget próprio (trait Widget): layout, draw e event", anel),
        ]
        .spacing(16)
        .into()
    }

    fn lousa(&self) -> Element<'_, Msg> {
        column![
            row![text(format!("{} traços", self.tracos)).size(13), horizontal_space(), button("Limpar").style(button_text).on_press(Msg::Limpar)].align(Align::Center),
            program(Lousa { cor: self.theme().primary, geracao: self.geracao }),
        ]
        .spacing(10)
        .height(Length::FILL)
        .into()
    }

    fn animacao(&self) -> Element<'_, Msg> {
        let transicao = Transition::new(self.duracao as u64).easing(self.easing());
        let bolinha = container(Space::new(36.0, 36.0)).style(|t| Style::default().background(t.primary).border(Border::rounded(18.0)));
        let trilho = container(animate(bolinha).offset(if self.movido { 420.0 } else { 0.0 }, 0.0).transition(transicao)).width(Length::FILL).padding(6.0).style(|t| Style::default().background(t.text.alpha(0.05)).border(Border::rounded(24.0)));
        let curvas = column![
            row![
                pick_list(&CURVAS[..], Some(self.curva), Msg::Curva).width(140.0),
                slider(100.0..=1500.0, self.duracao, Msg::Duracao).step(50.0).width(200.0),
                text(format!("{:.0} ms", self.duracao)).size(13).font(Font::MONO),
                button("Mover").on_press(Msg::Mover),
            ]
            .spacing(12)
            .align(Align::Center),
            trilho,
        ]
        .spacing(12);

        // A transição do estilo: a cor de fundo anda no hover em vez de pular.
        let lento = move |t: &Theme, s: Status| {
            let (fundo, letra) = if s == Status::Hovered { (t.primary, t.on_primary) } else { (t.background, t.text) };
            Style::default().background(fundo).text_color(letra).border(Border::rounded(10.0).color(t.border, 1.0)).transition(transicao)
        };
        let cartoes = row((1..=3).map(|i| {
            let b = button(column![text(format!("Cartão {i}")).font(Font::SEMIBOLD), text("hover cresce, clique afunda").size(12)].spacing(4)).padding(16.0).width(Length::FILL).style(lento).on_press(Msg::Log(format!("Cartão {i}")));
            animate(b).hover(Keyframe::default().scale(1.04).offset(0.0, -3.0)).pressed(Keyframe::default().scale(0.97)).transition(Transition::new(160).easing(Easing::EaseOut)).into()
        }))
        .spacing(12);

        let roda = canvas(24.0, 24.0, |r, a, t| {
            let c = Point::new(a.x + 12.0, a.y + 12.0);
            if let Some(arco) = Path::new(|b| {
                b.arc(c, Point::new(9.0, 9.0), 0.0, 4.7);
            }) {
                r.stroke_path(&arco, &Stroke::new(3.0).cap(LineCap::Round), t.primary);
            }
        });
        let ponto = container(Space::new(12.0, 12.0)).style(|t| Style::default().background(t.danger).border(Border::rounded(6.0)));
        let repeticao = row![
            animate(roda).rotate(360.0).repeat(Repeat::Forever).transition(Transition::new(900).easing(Easing::Linear)),
            text("Repeat::Forever").size(13),
            Space::new(24.0, 0.0),
            animate(ponto).opacity(0.15).repeat(Repeat::Alternate).transition(Transition::new(700)),
            text("Repeat::Alternate").size(13),
            Space::new(24.0, 0.0),
            animate(caixa("rotate, scale, opacity", Length::Shrink)).rotate(if self.girado { 180.0 } else { 0.0 }).scale(if self.girado { 0.8 } else { 1.0 }).opacity(if self.girado { 0.5 } else { 1.0 }),
            button("Girar").style(button_text).on_press(Msg::Girar),
        ]
        .spacing(10)
        .align(Align::Center);

        let itens = column(["Baixar atualização", "Sincronizar notas", "Limpar a lixeira", "Fazer backup"].iter().enumerate().map(|(i, s)| {
            animate(caixa(*s, Length::FILL))
                .show(self.lista)
                .enter(Keyframe::default().opacity(0.0).offset(-24.0, 0.0))
                .exit(Keyframe::default().opacity(0.0).offset(24.0, 0.0))
                .transition(Transition::new(260).easing(Easing::EaseOut).delay(i as u64 * 60))
                .into()
        }))
        .spacing(6);
        let entrada = column![button(if self.lista { "Esconder" } else { "Mostrar" }).on_press(Msg::Lista), itens].spacing(10);

        column![
            cartao("Easing e duração (animate().offset com Transition)", curvas),
            cartao("Style.transition + animate().hover / pressed", cartoes),
            cartao("Repetição e alvos", repeticao),
            cartao("enter, show e exit em cascata (delay)", entrada),
            nota("Os widgets prontos já animam sozinhos: menus, calendário, seletor de cor, dicas, caixas, chave, slider, barra e abas."),
        ]
        .spacing(16)
        .into()
    }

    fn janela(&self) -> Element<'_, Msg> {
        let cmd = |rotulo: &'static str, c: WindowCmd| button(rotulo).style(button_text).on_press(Msg::Janela(c));
        let comandos = row![
            cmd("Tela cheia", WindowCmd::ToggleFullscreen),
            cmd("Maximizar", WindowCmd::ToggleMaximize),
            cmd("Minimizar", WindowCmd::Minimize),
            cmd("Mover para 80, 80", WindowCmd::Move(Point::new(80.0, 80.0))),
            cmd("1280 × 860", WindowCmd::Resize(Size::new(1280.0, 860.0))),
            cmd("Focar", WindowCmd::Focus),
            button("Mudar o título").style(button_text).on_press(Msg::Titulo),
            button("Alça crua").style(button_text).on_press(Msg::Alca),
        ]
        .spacing(6)
        .wrap();
        let opcoes = row![toggler("Sempre no topo (WindowLevel)", self.no_topo).on_toggle(Msg::NoTopo), horizontal_space(), button("Sair do app").on_press(Msg::Janela(WindowCmd::Exit))].align(Align::Center);
        let alca = |rotulo: &'static str, msg: Msg, icone: CursorIcon| {
            mouse_area(container(text(rotulo).size(13)).width(Length::FILL).height(70.0).align_x(Align::Center).align_y(Align::Center).style(|t| Style::default().background(t.text.alpha(0.05)).border(Border::rounded(8.0).color(t.border, 1.0)))).on_press(msg).cursor_icon(icone)
        };
        let arrastar = row![
            alca("Arraste aqui para mover a janela (Drag)", Msg::Janela(WindowCmd::Drag), CursorIcon::Move),
            alca("Arraste aqui para redimensionar (DragResize)", Msg::Janela(WindowCmd::DragResize(ResizeDirection::SouthEast)), CursorIcon::SeResize),
        ]
        .spacing(12);
        let (rotulo, acao) = if self.segunda { ("Fechar a segunda janela", Msg::Segunda(WindowCmd::Close)) } else { ("Abrir a segunda janela", Msg::AbrirSegunda) };
        let segunda = row![button(rotulo).on_press(acao), nota("Task::open_window com decorations: false; a barra dela é um mouse_area com Drag e duplo clique.")].spacing(12).align(Align::Center);
        let config = nota("Esta janela abre com Settings { title, size, min_size, app_id, debug: true }. Com debug, o F12 liga o inspetor de layout (margem, padding e conteúdo).");
        column![
            cartao("WindowCmd", column![comandos, opcoes].spacing(10)),
            cartao("Arrastar e redimensionar (funciona também sem moldura)", arrastar),
            cartao("Várias janelas", segunda),
            cartao("Settings", config),
        ]
        .spacing(16)
        .into()
    }

    fn tarefas(&self) -> Element<'_, Msg> {
        let baixando = self.baixando.is_some();
        let tasks = column![
            row![
                button("Task::perform (1 s)").style(button_text).on_press(Msg::Esperar),
                button("Task::batch").style(button_text).on_press(Msg::Lote),
                button("Task::run (download)").on_press_maybe((!baixando).then_some(Msg::Baixar)),
                button("Cancelar (abortable)").style(button_text).on_press_maybe(baixando.then_some(Msg::Cancelar)),
            ]
            .spacing(8)
            .align(Align::Center),
            row![progress_bar(0.0..=100.0, self.progresso as f32).width(Length::FILL), text(format!("{}%", self.progresso)).size(13).font(Font::MONO).width(44.0)].spacing(10).align(Align::Center),
        ]
        .spacing(12);
        let componente = column![self.contador.view().map(Msg::Contador), nota("Um componente com Msg próprio: Element::map e Task::map ligam ele ao app.")].spacing(8);
        let area = row![text_input("Texto", &self.area).on_input(Msg::Area).width(Length::FILL), button("Copiar").style(button_text).on_press(Msg::Copiar), button("Colar").style(button_text).on_press(Msg::Colar)].spacing(8).align(Align::Center);
        let assinaturas = column![
            toggler("Subscription::every(1 s): relógio na barra de baixo", self.relogio).on_toggle(Msg::Relogio),
            row![toggler("Subscription::run_with_id: um fluxo a cada 500 ms", self.pulsos).on_toggle(Msg::Pulsos), horizontal_space(), text(format!("pulso {}", self.pulso)).size(13).font(Font::MONO)].align(Align::Center),
            nota("Desligar tira o id da lista e o fluxo é derrubado. Ligar de novo começa outro."),
        ]
        .spacing(10);
        column![cartao("Task", tasks), par(cartao("Componente (map)", componente), cartao("Área de transferência", area)), cartao("Subscription", assinaturas)].spacing(16).into()
    }

    fn eventos(&self) -> Element<'_, Msg> {
        let p = self.pairando;
        let alvo = container(text(if p { "Pode soltar!" } else { "Arraste arquivos de fora para cima da janela" }).size(14)).width(Length::FILL).height(90.0).align_x(Align::Center).align_y(Align::Center).style(move |t| {
            Style::default().background(t.primary.alpha(if p { 0.2 } else { 0.05 })).border(Border::rounded(10.0).color(if p { t.primary } else { t.border }, if p { 2.0 } else { 1.0 })).transition(Transition::new(150))
        });
        let linhas = column(self.log.iter().rev().take(60).map(|l| text(l.as_str()).size(12).font(Font::MONO).into())).spacing(3);
        let registro = column![
            row![nota("Também chegam: Ctrl/Shift/Alt/Super, mudança de escala, foco, tamanho e toque (Finger*)."), horizontal_space(), button("Limpar").style(button_text).on_press(Msg::LimparLog)].align(Align::Center),
            container(scrollable(linhas)).height(300.0).padding(10.0).style(|t| Style::default().background(t.background).border(Border::rounded(6.0).color(t.border, 1.0))),
        ]
        .spacing(8);
        column![
            cartao("FileHovered / FileDropped", alvo),
            cartao("Registro (App::on_event)", registro),
            cartao(
                "Atalhos globais (App::on_unhandled_event)",
                row![nota("Ctrl+D troca o tema e F11 liga a tela cheia, menos enquanto se digita num campo."), horizontal_space(), button("Simular Ctrl+D (Task::window_event)").style(button_text).on_press(Msg::Simular)].align(Align::Center),
            ),
        ]
        .spacing(16)
        .into()
    }

    fn terminal(&self) -> Element<'_, Msg> {
        let corpo: Element<'_, Msg> = match (&self.term, &self.erro_term) {
            (Some(t), _) => container(terminal(t).size(13.0)).padding(1.0).height(Length::FILL).clip(true).style(|t| Style::default().border(Border::rounded(8.0).color(t.border, 1.0))).into(),
            (None, Some(e)) => text(format!("O shell não abriu: {e}")).into(),
            (None, None) => text("O shell saiu. Troque de seção e volte para abrir outro.").into(),
        };
        column![
            row![button("Rodar ls").style(button_text).on_press(Msg::RodarLs), button("Copiar a tela").style(button_text).on_press(Msg::CopiarTela), nota("Clique no terminal para digitar. Ctrl+Shift+C/V copiam e colam.")].spacing(8).align(Align::Center),
            corpo,
        ]
        .spacing(10)
        .height(Length::FILL)
        .into()
    }
}

// ---------- Canvas interativo ----------

/// Um quadro onde se desenha com o mouse. Os traços ficam no estado do `program`, guardado
/// pelo pelicano; `geracao` muda quando o app pede para limpar.
struct Lousa {
    cor: Color,
    geracao: u64,
}

#[derive(Default)]
struct Tracos {
    geracao: u64,
    linhas: Vec<Vec<Point>>,
    atual: Option<Vec<Point>>,
}

impl Program<Msg> for Lousa {
    type State = Tracos;

    fn draw(&self, st: &Tracos, r: &mut Renderer, b: Rect, t: &Theme, cursor: Option<Point>) {
        r.fill_quad(b, Some(&t.surface.into()), Border::rounded(10.0).color(t.border, 1.0), None);
        let linhas: Vec<&Vec<Point>> = if st.geracao == self.geracao { st.linhas.iter().chain(st.atual.iter()).collect() } else { Vec::new() };
        if linhas.is_empty() {
            r.draw_text("Arraste com o botão esquerdo para desenhar", 14.0, Font::DEFAULT, t.muted, Point::new(b.x + 20.0, b.y + 20.0), None);
        }
        for l in linhas {
            let pontos: Vec<Point> = l.iter().map(|p| Point::new(b.x + p.x, b.y + p.y)).collect();
            r.stroke(&pontos, 3.0, self.cor);
        }
        if let Some(mira) = cursor.filter(|c| b.contains(*c)).and_then(|c| Path::circle(c, 6.0)) {
            r.stroke_path(&mira, &Stroke::new(1.0), t.muted);
        }
    }

    fn update(&self, st: &mut Tracos, ev: &Event, b: Rect, cursor: Option<Point>, publish: &mut dyn FnMut(Msg)) -> Captured {
        if st.geracao != self.geracao {
            *st = Tracos { geracao: self.geracao, ..Default::default() };
        }
        let relativo = |p: Point| Point::new(p.x - b.x, p.y - b.y);
        match ev {
            Event::MousePressed(MouseButton::Left) => {
                if let Some(c) = cursor.filter(|c| b.contains(*c)) {
                    st.atual = Some(vec![relativo(c)]);
                    return Captured::Yes;
                }
            }
            Event::CursorMoved(p) => {
                if let Some(l) = &mut st.atual {
                    l.push(relativo(*p));
                    return Captured::Yes;
                }
            }
            Event::MouseReleased(MouseButton::Left) => {
                if let Some(l) = st.atual.take() {
                    publish(Msg::Traco(l.len()));
                    st.linhas.push(l);
                    return Captured::Yes;
                }
            }
            _ => {}
        }
        Captured::No
    }

    fn mouse_interaction(&self, _: &Tracos, b: Rect, cursor: Option<Point>) -> CursorIcon {
        if cursor.is_some_and(|c| b.contains(c)) {
            CursorIcon::Crosshair
        } else {
            CursorIcon::Default
        }
    }
}

// ---------- Widget próprio ----------

/// Um anel que mostra um valor de 0 a 1 e anda até ele quando muda. O clique manda `ao_clicar`.
struct Anel<M> {
    valor: f32,
    ao_clicar: M,
}

impl<M: Clone> Widget<M> for Anel<M> {
    fn layout(&self, cx: &mut LayoutCx, _: Limits) -> Node {
        Node::leaf(cx.key(), Size::new(96.0, 96.0))
    }

    fn draw(&self, node: &Node, cx: &mut DrawCx) {
        let r = node.rect;
        let v = cx.transition(node.key, self.valor, Transition::new(500).easing(Easing::EaseOut));
        let c = Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0);
        let cor = if cx.hovered(&r) { cx.theme.primary.mix(Color::WHITE, 0.25) } else { cx.theme.primary };
        if let Some(trilho) = Path::circle(c, 40.0) {
            cx.renderer.stroke_path(&trilho, &Stroke::new(8.0), cx.theme.border);
        }
        let inicio = -std::f32::consts::FRAC_PI_2;
        if let Some(arco) = Path::new(|b| {
            b.arc(c, Point::new(40.0, 40.0), inicio, inicio + v.max(0.001) * std::f32::consts::TAU);
        }) {
            cx.renderer.stroke_path(&arco, &Stroke::new(8.0).cap(LineCap::Round), cor);
        }
        let rotulo = format!("{:.0}%", v * 100.0);
        let tam = cx.renderer.measure(&rotulo, 16.0, Font::SEMIBOLD, None);
        cx.renderer.draw_text(&rotulo, 16.0, Font::SEMIBOLD, cx.theme.text, Point::new(c.x - tam.width / 2.0, c.y - tam.height / 2.0), None);
    }

    fn event(&self, node: &Node, ev: &Event, cx: &mut EventCx<M>) -> Captured {
        if matches!(ev, Event::MousePressed(MouseButton::Left)) && cx.cursor.is_some_and(|p| node.rect.contains(p)) {
            cx.publish(self.ao_clicar.clone());
            return Captured::Yes;
        }
        Captured::No
    }
}

// ---------- Um componente com mensagens próprias ----------

mod contador {
    use pelicano::*;

    #[derive(Debug, Clone)]
    pub enum Msg {
        Mais,
        Menos,
        ZerarDepois,
        Zerar,
    }

    #[derive(Default)]
    pub struct Contador {
        pub valor: i32,
    }

    impl Contador {
        pub fn update(&mut self, m: Msg) -> Task<Msg> {
            match m {
                Msg::Mais => self.valor += 1,
                Msg::Menos => self.valor -= 1,
                Msg::ZerarDepois => return Task::perform(tokio::time::sleep(std::time::Duration::from_millis(800)), |_| Msg::Zerar),
                Msg::Zerar => self.valor = 0,
            }
            Task::none()
        }

        pub fn view(&self) -> Element<'_, Msg> {
            row![
                button("−").on_press(Msg::Menos),
                text(self.valor.to_string()).size(18).width(40.0).align_x(Align::Center),
                button("+").on_press(Msg::Mais),
                button("Zerar em 0,8 s").style(button_text).on_press(Msg::ZerarDepois),
            ]
            .spacing(8)
            .align(Align::Center)
            .into()
        }
    }
}

// ---------- Auxiliares ----------

fn estrela(centro: Point, raio: f32) -> Option<Path> {
    let pontos: Vec<Point> = (0..5).map(|i| (i * 2 % 5) as f32 * std::f32::consts::TAU / 5.0 - std::f32::consts::FRAC_PI_2).map(|a| Point::new(centro.x + raio * a.cos(), centro.y + raio * a.sin())).collect();
    Path::polyline(&pontos, true)
}

/// Uma "foto" gerada em pixels: céu em degradê, sol e morro.
fn foto() -> Option<Image> {
    let (w, h) = (240u32, 160u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let sol = ((fx - 0.75).powi(2) + (fy - 0.3).powi(2)).sqrt() < 0.1;
            let morro = fy > 0.65 + 0.12 * (fx * 7.0).sin();
            let c = if sol { [255, 210, 90] } else if morro { [60, (120.0 + 40.0 * fy) as u8, 80] } else { [(90.0 + 100.0 * fy) as u8, (150.0 + 60.0 * fy) as u8, 240] };
            px.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Image::from_rgba(w, h, px)
}

fn pixel_art() -> Option<Image> {
    const MAPA: [&str; 8] = ["..XXXX..", ".X....X.", "X.X..X.X", "X......X", "X.X..X.X", "X..XX..X", ".X....X.", "..XXXX.."];
    let px = MAPA.iter().flat_map(|l| l.bytes()).flat_map(|b| if b == b'X' { [40, 40, 48, 255] } else { [250, 204, 21, 255] }).collect();
    Image::from_rgba(8, 8, px)
}

/// Um PNG feito pelo próprio `capture`: qualquer árvore de widgets vira imagem.
fn png_capturado() -> Option<Image> {
    let el: Element<'_, Msg> = container(text("capture → PNG").size(13).color(Color::WHITE))
        .width(Length::FILL)
        .height(Length::FILL)
        .align_x(Align::Center).align_y(Align::Center)
        .style(|_| Style::default().background(Background::Linear { angle: 0.5, stops: vec![(0.0, Color::rgb8(99, 102, 241)), (1.0, Color::rgb8(20, 184, 166))] }).border(Border::rounded(10.0)))
        .into();
    Image::from_png(&capture(&el, Size::new(120.0, 80.0), 2.0, &Theme::dark(), &mut Renderer::new(), None))
}

fn modificadores(m: Modifiers) -> String {
    let nomes = [(m.control_key(), "Ctrl"), (m.shift_key(), "Shift"), (m.alt_key(), "Alt"), (m.super_key(), "Super")];
    let ligados: Vec<&str> = nomes.iter().filter(|n| n.0).map(|n| n.1).collect();
    if ligados.is_empty() {
        "nenhum".into()
    } else {
        ligados.join("+")
    }
}

fn hora_utc() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) % 86_400;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// `--capturar <pasta>`: um PNG por seção, mais o outro tema e a segunda janela.
fn capturar(pasta: &std::path::Path) -> std::io::Result<()> {
    let mut app = Vitrine::new();
    let tamanho = Size::new(1280.0, 860.0);
    let salvar = |app: &Vitrine, el: Element<'_, Msg>, tam: Size, nome: &str| {
        let destino = pasta.join(format!("pv_{nome}.png"));
        println!("{}", destino.display());
        std::fs::write(destino, capture(&el, tam, 1.0, &app.theme(), &mut Renderer::new(), None))
    };
    for (secao, _, nome, _) in SECOES {
        let _ = app.update(Msg::Ir(secao));
        if secao == Secao::Terminal {
            // Dá tempo de o shell escrever o prompt.
            std::thread::sleep(Duration::from_millis(800));
        }
        salvar(&app, app.view(), tamanho, nome)?;
    }
    app.escuro = !app.escuro;
    app.secao = Secao::Controles;
    salvar(&app, app.view(), tamanho, "controles_outro_tema")?;
    salvar(&app, app.segunda_janela(), Size::new(420.0, 260.0), "segunda_janela")
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let pasta = std::path::PathBuf::from(args.get(i + 1).map_or(".", String::as_str));
        return capturar(&pasta).map_err(|e| Error::Window(e.to_string()));
    }
    let settings = Settings {
        title: "Vitrine — pelicano".into(),
        size: Size::new(1280.0, 860.0),
        min_size: Some(Size::new(900.0, 600.0)),
        app_id: Some("pelicano-vitrine".into()),
        debug: true,
        ..Default::default()
    };
    run(settings, || (Vitrine::new(), Task::none()))
}
