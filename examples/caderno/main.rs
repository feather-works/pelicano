//! Pelicano Notes: um caderno de notas em vidro escuro, no estilo do macOS. É só um teste visual.
//! `cargo run --example caderno`; `-- --fundo` desenha o papel de parede atrás do vidro (sem desfoque do compositor);
//! `-- --capturar notas.png` só gera o PNG (1090×705, com o papel de parede).
mod icones;
mod paisagem;

use icones::icone;
use pelicano::column;
use pelicano::*;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

const FONTE: Font = Font::with_name("Inter");
const MEDIA: Font = FONTE.weight(500);
const SEMI: Font = FONTE.weight(600);
const NEGRITO: Font = FONTE.weight(700);

static CLARO: AtomicBool = AtomicBool::new(false);

fn claro() -> bool {
    CLARO.load(Ordering::Relaxed)
}
fn hex(s: &str) -> Color {
    Color::hex(s).unwrap_or(Color::BLACK)
}
fn tom(escuro: &str, claro_: &str) -> Color {
    hex(if claro() { claro_ } else { escuro })
}
fn texto() -> Color {
    tom("#e8ebf2", "#1c2130")
}
fn mudo() -> Color {
    tom("#98a0b2", "#5c6476")
}
fn fraco() -> Color {
    tom("#c3c8d3", "#3c4456")
}
fn linha() -> Color {
    tom("#ffffff14", "#0000001a")
}
fn realce() -> Color {
    tom("#ffffff12", "#0000000d")
}
fn acento() -> Color {
    hex("#3d8bfd")
}

/// (nome, cor, quantidade)
const TAGS: [(&str, &str, u32); 6] = [
    ("Trabalho", "#4a90e2", 14),
    ("Pessoal", "#a66be8", 8),
    ("Projetos", "#4cb860", 10),
    ("Ideias", "#f08a3c", 6),
    ("Estudos", "#e8507e", 4),
    ("Financeiro", "#f2c94c", 5),
];
const NAV: [(&str, &str, u32); 6] =
    [("pasta", "Todas", 42), ("estrela", "Favoritas", 8), ("relogio", "Recentes", 12), ("tarefa", "Tarefas", 7), ("arquivo", "Arquivadas", 21), ("lixeira", "Lixeira", 3)];

struct Nota {
    titulo: &'static str,
    resumo: &'static str,
    tag: usize,
    data: &'static str,
    cena: usize,
    capa: usize,
    favorita: bool,
    palavras: u32,
    /// (texto, complemento apagado, feito)
    metas: Vec<(&'static str, &'static str, bool)>,
    projetos: Vec<&'static str>,
}

fn notas() -> Vec<Nota> {
    let n = |titulo, resumo, tag, data, cena, palavras, metas: Vec<(&'static str, &'static str, bool)>, projetos: Vec<&'static str>| Nota {
        titulo,
        resumo,
        tag,
        data,
        cena,
        capa: cena,
        favorita: cena == 0,
        palavras,
        metas,
        projetos,
    };
    vec![
        n(
            "Planejamento 2026",
            "Metas, projetos e organização do ano",
            0,
            "Hoje 14:32",
            0,
            256,
            vec![
                ("Lançamento Alpha do FeatherOffice", " (30/10/2026)", true),
                ("Melhorar performance do FeatherSheet", "", false),
                ("Avançar no FeatherSlide e FeatherDocs", "", false),
                ("Estruturar o FeatherWeb", " (navegador)", false),
                ("Iniciar desenvolvimento do FeatherPDF", "", false),
            ],
            vec![
                "Migração completa para Rust + Pelicano",
                "Implementação dos motores compartilhados",
                "Melhorias na interface e experiência do usuário",
                "Otimizações de carregamento de grandes arquivos",
            ],
        ),
        n(
            "Ideias de projeto",
            "Lista de ideias para o ecossistema Feather",
            2,
            "Ontem 18:11",
            1,
            134,
            vec![("Plugin de diagramas no FeatherDocs", "", false), ("Modo foco no editor", "", true), ("Sincronização entre dispositivos", "", false)],
            vec!["Protótipo de colaboração em tempo real", "Tema claro revisado"],
        ),
        n(
            "Configuração do Arch",
            "Comandos, pacotes e pós-instalação do sistema",
            4,
            "18/10/2026",
            2,
            412,
            vec![("Particionar o disco", "", true), ("Instalar o base e o linux-zen", "", true), ("Configurar o Hyprland", "", false)],
            vec!["pacman -S base-devel git", "Ativar o NetworkManager", "Fontes: Inter e JetBrains Mono"],
        ),
        n(
            "Viagem",
            "Roteiro, hotéis e lugares para visitar",
            1,
            "17/10/2026",
            3,
            98,
            vec![("Passagens compradas", "", true), ("Reservar o ryokan em Kyoto", "", false), ("Tirar o JR Pass", "", false)],
            vec!["Fushimi Inari ao amanhecer", "Arashiyama e a floresta de bambu"],
        ),
        n(
            "Snippets úteis",
            "Comandos e trechos de código",
            0,
            "16/10/2026",
            4,
            187,
            vec![("Alias do git log bonito", "", true), ("Script de backup com rsync", "", false)],
            vec!["cargo build --example caderno", "rg --files | fzf", "git worktree add ../x"],
        ),
        n(
            "Referências de arte",
            "Estilos, paletas e conceitos",
            3,
            "15/10/2026",
            5,
            76,
            vec![("Paleta de pôr do sol", "", true), ("Estudo de luz e sombra", "", false)],
            vec!["Paisagens em camadas", "Vidro e desfoque nas interfaces"],
        ),
        n(
            "Receitas",
            "Comidas, temperos e variações",
            1,
            "14/10/2026",
            6,
            143,
            vec![("Comprar açafrão", "", false), ("Testar o pão de fermentação natural", "", true)],
            vec!["Risoto de abóbora", "Curry de grão-de-bico", "Molho de tomate assado"],
        ),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Filtro {
    Nav(usize),
    Tag(usize),
}

#[derive(Debug, Clone)]
enum Msg {
    Arrastar,
    Maximizar,
    Minimizar,
    Fechar,
    Puxar(ResizeDirection),
    Busca(String),
    Filtrar(Filtro),
    Selecionar(usize),
    Marcar(usize, bool),
    Favorita,
    TrocarCapa,
    Tema,
    Nada,
}

struct Caderno {
    notas: Vec<Nota>,
    atual: usize,
    filtro: Filtro,
    busca: String,
    miniaturas: Vec<Image>,
    capas: Vec<Option<Image>>,
    anexo: Image,
    avatar: Image,
    fundo: Option<Image>,
    margem: Padding,
}

impl Caderno {
    fn new(fundo: bool) -> Caderno {
        let mut c = Caderno {
            notas: notas(),
            atual: 0,
            filtro: Filtro::Nav(0),
            busca: String::new(),
            miniaturas: (0..7).map(|n| paisagem::cena(n, 112, 112)).collect(),
            capas: vec![None; 8],
            anexo: paisagem::cena(7, 320, 220),
            avatar: paisagem::cena(5, 56, 56),
            fundo: fundo.then(|| paisagem::cena(0, 1090, 705)),
            margem: Padding::ZERO,
        };
        c.garantir_capa();
        c
    }

    fn garantir_capa(&mut self) {
        let n = self.notas[self.atual].capa;
        if self.capas[n].is_none() {
            self.capas[n] = Some(paisagem::cena(n, 1140, 256));
        }
    }

    fn visiveis(&self) -> Vec<usize> {
        let busca = self.busca.to_lowercase();
        (0..self.notas.len())
            .filter(|&i| {
                let n = &self.notas[i];
                let passa = match self.filtro {
                    Filtro::Nav(1) => n.favorita,
                    Filtro::Tag(t) => n.tag == t,
                    _ => true,
                };
                passa && (busca.is_empty() || n.titulo.to_lowercase().contains(&busca) || n.resumo.to_lowercase().contains(&busca))
            })
            .collect()
    }

    // ---------- barra de título ----------

    fn barra(&self) -> Element<'_, Msg> {
        let luz = |cor: &'static str, m: Msg, nome: &'static str| -> Element<'_, Msg> {
            Element::from(button(Space::new(12.0, 12.0))
                .padding(0)
                .on_press(m)
                .style(move |_, s| {
                    let c = hex(cor);
                    let c = if matches!(s, Status::Hovered | Status::Pressed) { c.mix(Color::BLACK, 0.18) } else { c };
                    Style::default().background(c).border(Border::rounded(6.0).color(Color::BLACK.alpha(0.18), 0.5))
                }))
                .access_label(nome)
        };
        let luzes = row![luz("#ff5f57", Msg::Fechar, "Fechar"), luz("#febc2e", Msg::Minimizar, "Minimizar"), luz("#28c840", Msg::Maximizar, "Maximizar")].spacing(8);
        let marca = row![luzes, Space::new(22.0, 1.0), ic("pena", 20.0, hex("#4f9cf9")), t("Pelicano Notes", 12.5, texto()).font(SEMI)].spacing(8).align(Align::Center).width(Length::FILL);

        let tecla = |s: &'static str| container(t(s, 10.5, mudo())).padding([1, 5]).style(|_| Style::default().background(realce()).border(Border::rounded(4.0).color(linha(), 1.0)));
        let campo = text_input("Pesquisar notas, tarefas, tags…", &self.busca)
            .on_input(Msg::Busca)
            .size(12.5)
            .font(FONTE)
            .padding([4, 2])
            .width(Length::FILL)
            .style(|_, _| Style::default().text_color(texto()));
        let busca = container(row![ic("busca", 16.0, mudo()), campo, tecla("Ctrl"), tecla("K")].spacing(6).align(Align::Center))
            .width(441)
            .height(30)
            .padding([0, 10])
            .align_y(Align::Center)
            .style(|_| Style::default().background(tom("#ffffff0f", "#0000000a")).border(Border::rounded(10.0).color(linha(), 1.0)));

        let sino = stack![
            container(ic("sino", 18.0, fraco())).width(22).height(22).align_x(Align::Center).align_y(Align::Center),
            container(container(Space::new(7.0, 7.0)).style(|_| Style::default().background(hex("#ff453a")).border(Border::rounded(3.5)))).width(22).height(22).align_x(Align::End).padding([2, 2]),
        ];
        let lados = row![
            horizontal_space(),
            botao(ic(if claro() { "lua" } else { "sol" }, 18.0, fraco()), Msg::Tema, 6.0).access_label("Tema"),
            Space::new(8.0, 1.0),
            botao(sino.into(), Msg::Nada, 4.0),
            Space::new(14.0, 1.0),
            container(image(self.avatar.clone()).width(28).height(28).radius(14.0).content_fit(ContentFit::Cover))
                .padding(1)
                .style(|_| Style::default().border(Border::rounded(15.0).color(Color::WHITE.alpha(0.25), 1.0))),
            ic("abaixo", 15.0, mudo()),
        ]
        .spacing(6)
        .align(Align::Center)
        .width(Length::FILL);
        let barra = row![marca, busca, lados].height(52).padding([0, 18]).align(Align::Center);
        mouse_area(barra).on_press(Msg::Arrastar).on_double_click(Msg::Maximizar).into()
    }

    // ---------- lateral ----------

    fn lateral(&self) -> Element<'_, Msg> {
        let nova = button(row![ic("mais", 15.0, Color::WHITE), t("Nova Nota", 12.5, Color::WHITE).font(MEDIA)].spacing(8).align(Align::Center))
            .width(140)
            .height(32)
            .on_press(Msg::Nada)
            .style(|_, s| {
                let k = match s {
                    Status::Hovered => 0.1,
                    Status::Pressed => -0.1,
                    _ => 0.0,
                };
                let (a, b) = (hex("#4a8ef0"), hex("#3a63c9"));
                let (a, b) = if k >= 0.0 { (a.mix(Color::WHITE, k), b.mix(Color::WHITE, k)) } else { (a.mix(Color::BLACK, -k), b.mix(Color::BLACK, -k)) };
                Style::default()
                    .background(Background::Linear { angle: 0.0, stops: vec![(0.0, a), (1.0, b)] })
                    .border(Border::rounded(16.0).color(Color::WHITE.alpha(0.22), 1.0))
                    .shadow(Shadow { color: hex("#3d7ff0").alpha(0.45), offset: Point::new(0.0, 3.0), blur: 14.0 })
                    .transition(Transition::new(120))
            });
        let nova = container(row![nova]).width(Length::FILL).align_x(Align::Center);

        let mut col = column![nova, Space::new(1.0, 14.0)].spacing(0);
        for (i, (icn, nome, n)) in NAV.iter().enumerate() {
            let sel = self.filtro == Filtro::Nav(i);
            let cor_ic = if i == 0 { hex("#5aa2f7") } else { fraco() };
            col = col.push(item(ic(icn, 18.0, cor_ic), nome, *n, sel, Msg::Filtrar(Filtro::Nav(i))));
        }
        col = col.push(Space::new(1.0, 14.0)).push(container(container(Space::new(Length::FILL, 1.0)).style(|_| Style::default().background(linha()))).padding([0, 8]));
        col = col.push(Space::new(1.0, 14.0)).push(
            row![t("Tags", 13.0, mudo()).font(MEDIA), horizontal_space(), botao(ic("mais", 15.0, mudo()), Msg::Nada, 3.0)].align(Align::Center).padding(p4(0, 6, 0, 13)),
        );
        col = col.push(Space::new(1.0, 6.0));
        for (i, (nome, cor, n)) in TAGS.iter().enumerate() {
            let bola = container(Space::new(14.0, 14.0)).style(move |_| {
                Style::default()
                    .background(Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, hex(cor).mix(Color::WHITE, 0.25)), (1.0, hex(cor))] })
                    .border(Border::rounded(7.0))
            });
            col = col.push(item(bola.into(), nome, *n, self.filtro == Filtro::Tag(i), Msg::Filtrar(Filtro::Tag(i))));
        }
        container(col).width(181).height(Length::FILL).padding(p4(12, 9, 12, 14)).into()
    }

    // ---------- lista de notas ----------

    fn lista(&self) -> Element<'_, Msg> {
        let topo = row![t("Notas", 16.0, texto()).font(SEMI), horizontal_space(), botao(ic("filtro", 17.0, fraco()), Msg::Nada, 4.0)].align(Align::Center).padding(p4(0, 4, 0, 9));
        let mut cartoes = column(Vec::<Element<Msg>>::new()).spacing(6).padding(p4(0, 7, 8, 0));
        let vis = self.visiveis();
        if vis.is_empty() {
            cartoes = cartoes.push(container(t("Nenhuma nota encontrada", 12.0, mudo())).padding(16).width(Length::FILL).align_x(Align::Center).align_y(Align::Center));
        }
        for i in vis {
            cartoes = cartoes.push(self.cartao(i));
        }
        container(column![topo, scrollable(cartoes)].spacing(10))
            .width(279)
            .height(Length::FILL)
            .padding(p4(12, 0, 0, 7))
            .style(|_| {
                Style::default()
                    .background(tom("#1a1f2bb0", "#ffffff80"))
                    .border(Border { radius: [12.0, 0.0, 0.0, 0.0], ..Border::default() }.color(linha(), 1.0))
            })
            .into()
    }

    fn cartao(&self, i: usize) -> Element<'_, Msg> {
        let n = &self.notas[i];
        let sel = i == self.atual;
        let corpo = column![
            t(n.titulo, 12.5, texto()).font(SEMI),
            t(cortar(n.resumo, 30), 11.0, mudo()),
            row![chip(n.tag, false), horizontal_space(), t(n.data, 10.5, mudo())].align(Align::Center),
        ]
        .spacing(5)
        .width(Length::FILL);
        button(row![image(self.miniaturas[n.cena].clone()).width(56).height(56).radius(8.0).content_fit(ContentFit::Cover), corpo].spacing(10).align(Align::Center))
            .padding(8)
            .width(Length::FILL)
            .on_press(Msg::Selecionar(i))
            .style(move |_, s| {
                let hover = matches!(s, Status::Hovered | Status::Pressed);
                let st = if sel && claro() {
                    Style::default().background(hex("#dce8fd")).border(Border::rounded(10.0).color(acento(), 1.0))
                } else if sel {
                    Style::default()
                        .background(Background::Linear { angle: 0.0, stops: vec![(0.0, hex("#2b4a86").alpha(0.75)), (1.0, hex("#243a66").alpha(0.6))] })
                        .border(Border::rounded(10.0).color(hex("#5b8ff0").alpha(0.9), 1.0))
                        .shadow(Shadow { color: hex("#3d7ff0").alpha(0.35), offset: Point::ORIGIN, blur: 10.0 })
                } else {
                    Style::default()
                        .background(if hover { tom("#ffffff12", "#0000000f") } else { tom("#ffffff07", "#ffffff90") })
                        .border(Border::rounded(10.0).color(if hover { linha() } else { Color::TRANSPARENT }, 1.0))
                };
                st.transition(Transition::new(140))
            })
            .into()
    }

    // ---------- editor ----------

    fn editor(&self) -> Element<'_, Msg> {
        let n = &self.notas[self.atual];
        let capa: Element<Msg> = match &self.capas[n.capa] {
            Some(img) => image(img.clone()).width(Length::FILL).height(128).radius(10.0).content_fit(ContentFit::Cover).into(),
            None => Space::new(Length::FILL, 128.0).into(),
        };
        let alterar = button(row![ic("imagem", 14.0, Color::WHITE), t("Alterar capa", 12.0, Color::WHITE).font(MEDIA)].spacing(6).align(Align::Center))
            .padding([5, 10])
            .on_press(Msg::TrocarCapa)
            .style(|_, s| {
                let a = if matches!(s, Status::Hovered | Status::Pressed) { 0.7 } else { 0.5 };
                Style::default().background(hex("#10141e").alpha(a)).border(Border::rounded(8.0).color(Color::WHITE.alpha(0.15), 1.0)).backdrop_blur(8.0).transition(Transition::new(120))
            });
        let capa = stack![capa, container(alterar).width(Length::FILL).height(128).align_x(Align::End).align_y(Align::End).padding(10)];

        let redondo = |e: Element<'static, Msg>, m: Msg| -> Element<'static, Msg> {
            button(container(e).width(18).height(18).align_x(Align::Center).align_y(Align::Center))
                .padding(8)
                .on_press(m)
                .style(|_, s| {
                    let bg = if matches!(s, Status::Hovered | Status::Pressed) { tom("#ffffff1f", "#00000014") } else { tom("#ffffff12", "#0000000a") };
                    Style::default().background(bg).border(Border::rounded(17.0)).transition(Transition::new(120))
                })
                .into()
        };
        let estrela = if n.favorita { ic("estrela_cheia", 17.0, hex("#f7b733")) } else { ic("estrela", 17.0, fraco()) };
        let cursor = container(Space::new(1.5, 28.0)).style(|_| Style::default().background(texto().alpha(0.7)));
        let titulo = row![
            t(n.titulo, 24.0, texto()).font(NEGRITO),
            cursor,
            horizontal_space(),
            redondo(estrela, Msg::Favorita),
            redondo(ic("compartilhar", 17.0, fraco()), Msg::Nada),
            redondo(ic("reticencias", 17.0, fraco()), Msg::Nada),
        ]
        .spacing(4)
        .align(Align::Center);
        let titulo = row![titulo].spacing(0);
        let ponto = || t("•", 11.0, mudo());
        let meta = row![
            ic("calendario", 14.0, mudo()),
            t("20 de outubro de 2026", 12.0, mudo()),
            Space::new(4.0, 1.0),
            ponto(),
            Space::new(4.0, 1.0),
            ic("relogio", 14.0, mudo()),
            t("14:32", 12.0, mudo()),
            Space::new(4.0, 1.0),
            ponto(),
            Space::new(4.0, 1.0),
            chip_grande(Some(n.tag), TAGS[n.tag].0),
            chip_grande(None, "Planejamento"),
            botao(ic("mais", 14.0, fraco()), Msg::Nada, 5.0),
        ]
        .spacing(6)
        .align(Align::Center);

        let painel = container(column![self.ferramentas(), container(Space::new(Length::FILL, 1.0)).style(|_| Style::default().background(linha())), row![self.conteudo(), linha_v(), self.anexos()].height(Length::FILL)])
            .height(Length::FILL)
            .clip(true)
            .style(|_| Style::default().background(tom("#ffffff06", "#ffffffa0")).border(Border::rounded(12.0).color(linha(), 1.0)));

        let rodape = row![
            ic("salvo", 16.0, mudo()),
            t("Salvo automaticamente", 12.0, mudo()),
            horizontal_space(),
            t(format!("{} palavras", n.palavras), 12.0, mudo()),
            Space::new(14.0, 1.0),
            botao(ic("notas", 17.0, fraco()), Msg::Nada, 5.0),
            botao(ic("expandir", 17.0, fraco()), Msg::Nada, 5.0),
        ]
        .spacing(8)
        .align(Align::Center)
        .height(40)
        .padding([0, 10]);

        container(column![
            capa,
            Space::new(1.0, 12.0),
            container(titulo).padding(p4(0, 2, 0, 8)),
            Space::new(1.0, 8.0),
            container(meta).padding([0, 8]),
            Space::new(1.0, 12.0),
            container(painel).height(Length::FILL).padding(p4(0, 0, 0, 2)),
            rodape,
        ])
        .width(Length::FILL)
        .height(Length::FILL)
        .padding(p4(12, 16, 6, 11))
        .style(|_| Style::default().background(tom("#0d1119a0", "#f7f8fbb0")))
        .into()
    }

    fn ferramentas(&self) -> Element<'_, Msg> {
        let f = |nome: &'static str| botao(ic(nome, 16.0, fraco()), Msg::Nada, 6.0);
        let sep = || container(container(Space::new(1.0, 16.0)).style(|_| Style::default().background(linha()))).padding([0, 4]);
        let padrao = button(row![t("Padrão", 12.5, fraco()), ic("abaixo", 14.0, fraco())].spacing(14).align(Align::Center)).padding([5, 8]).on_press(Msg::Nada).style(estilo_botao);
        row![
            padrao,
            sep(),
            f("negrito"),
            f("italico"),
            f("sublinhado"),
            f("tachado"),
            f("codigo"),
            sep(),
            f("lista"),
            f("numerada"),
            sep(),
            f("tarefa"),
            sep(),
            f("link"),
            f("imagem"),
            f("tabela"),
            sep(),
            f("reticencias"),
        ]
        .spacing(5)
        .height(36)
        .padding([0, 8])
        .align(Align::Center)
        .into()
    }

    fn conteudo(&self) -> Element<'_, Msg> {
        let n = &self.notas[self.atual];
        let cabeca = |icn: &'static str, cor: &str, s: &'static str| row![ic(icn, 18.0, hex(cor)), t(s, 14.0, texto()).font(SEMI)].spacing(10).align(Align::Center);
        let mut col = column![cabeca("alvo", "#4f9cf9", "Metas principais"), Space::new(1.0, 1.0)].spacing(4);
        for (k, (s, extra, feito)) in n.metas.iter().enumerate() {
            let rotulo = rich_text([span(*s), span(*extra).color(mudo())]).size(12.5).font(FONTE).color(fraco());
            col = col.push(row![checkbox("", *feito).on_toggle(move |b| Msg::Marcar(k, b)), mouse_area(rotulo).on_press(Msg::Marcar(k, !feito))].spacing(8).align(Align::Center));
        }
        col = col.push(Space::new(1.0, 12.0)).push(cabeca("barras", "#43b85c", "Projetos em andamento")).push(Space::new(1.0, 1.0));
        for p in &n.projetos {
            col = col.push(row![t("•", 13.0, fraco()), t(*p, 12.5, fraco())].spacing(9).padding(p4(0, 0, 0, 6)));
        }
        scrollable(container(col).width(Length::FILL).padding(p4(10, 16, 10, 16))).into()
    }

    fn anexos(&self) -> Element<'_, Msg> {
        let mini = image(self.anexo.clone()).width(28).height(28).radius(4.0).content_fit(ContentFit::Cover);
        let pagina = stack![ic("pagina", 26.0, tom("#e9ebf0", "#c3c9d4")), ic("pagina_linhas", 26.0, hex("#9aa1ae"))];
        let anexo = |icone: Element<'static, Msg>, nome: &'static str, tam: &'static str| {
            row![
                container(icone).width(28).height(28).align_x(Align::Center).align_y(Align::Center),
                column![t(nome, 11.0, texto()).font(MEDIA), t(tam, 10.5, mudo())].spacing(2).width(Length::FILL),
                botao(ic("reticencias", 16.0, fraco()), Msg::Nada, 3.0),
            ]
            .spacing(8)
            .align(Align::Center)
            .padding(p4(7, 2, 7, 6))
        };
        let lista = container(column![anexo(mini.into(), "roadmap-2026.png", "2.4 MB"), anexo(pagina.into(), "ideias.txt", "12 KB")].spacing(2))
            .padding(2)
            .style(|_| Style::default().background(tom("#ffffff08", "#0000000a")).border(Border::rounded(8.0)));
        column![
            image(self.anexo.clone()).width(Length::FILL).height(110).radius(8.0).content_fit(ContentFit::Cover),
            Space::new(1.0, 12.0),
            row![ic("clipe", 15.0, fraco()), t("Anexos", 13.0, texto()).font(MEDIA), horizontal_space(), botao(ic("mais", 15.0, fraco()), Msg::Nada, 5.0)].spacing(8).align(Align::Center),
            lista,
        ]
        .spacing(4)
        .width(190)
        .padding(p4(12, 12, 12, 12))
        .into()
    }

    fn alcas(&self) -> Element<'_, Msg> {
        let alca = |w: Length, h: Length, d: ResizeDirection, i: CursorIcon| mouse_area(Space::new(w, h)).on_press(Msg::Puxar(d)).cursor_icon(i);
        stack![
            container(alca(Length::Fixed(5.0), Length::FILL, ResizeDirection::East, CursorIcon::EResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End),
            container(alca(Length::FILL, Length::Fixed(5.0), ResizeDirection::South, CursorIcon::SResize)).width(Length::FILL).height(Length::FILL).align_y(Align::End),
            container(alca(Length::Fixed(14.0), Length::Fixed(14.0), ResizeDirection::SouthEast, CursorIcon::SeResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End),
        ]
        .into()
    }
}

fn t<'a>(s: impl Into<Cow<'a, str>>, tam: f32, cor: Color) -> Text<'a> {
    text(s).font(FONTE).size(tam).color(cor).no_wrap()
}

fn ic<'a>(nome: &'static str, tam: f32, cor: Color) -> Element<'a, Msg> {
    svg(icone(nome)).width(tam).height(tam).color(cor).into()
}

/// shortcut: corta por caracteres, não pela largura medida; o pelicano não tem reticências no texto.
fn cortar(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>().trim_end())
    }
}

fn p4(top: u16, right: u16, bottom: u16, left: u16) -> Padding {
    Padding { top: top as f32, right: right as f32, bottom: bottom as f32, left: left as f32 }
}

fn linha_v<'a>() -> Element<'a, Msg> {
    container(Space::new(1.0, Length::FILL)).padding([12, 0]).style(|_| Style::default().background(linha())).into()
}

fn estilo_botao(_: &Theme, s: Status) -> Style {
    let bg = match s {
        Status::Hovered => Some(realce()),
        Status::Pressed => Some(realce().mix(texto(), 0.08)),
        _ => None,
    };
    Style { background: bg.map(Into::into), border: Border::rounded(6.0), ..Default::default() }.transition(Transition::new(120))
}

/// Botão só de ícone: sem fundo, acende no hover.
fn botao<'a>(e: Element<'a, Msg>, m: Msg, pad: f32) -> Element<'a, Msg> {
    button(e).padding(pad).on_press(m).style(estilo_botao).into()
}

fn item<'a>(icone: Element<'a, Msg>, nome: &'a str, n: u32, sel: bool, m: Msg) -> Element<'a, Msg> {
    button(row![container(icone).width(18).align_x(Align::Center).align_y(Align::Center), t(nome, 12.5, texto()), horizontal_space(), t(n.to_string(), 11.5, mudo())].spacing(12).align(Align::Center))
        .width(Length::FILL)
        .height(34)
        .padding(p4(0, 11, 0, 13))
        .on_press(m)
        .style(move |_, s| {
            let bg = if sel {
                Some(tom("#ffffff14", "#0000000f"))
            } else if matches!(s, Status::Hovered | Status::Pressed) {
                Some(tom("#ffffff0a", "#00000008"))
            } else {
                None
            };
            Style { background: bg.map(Into::into), border: Border::rounded(8.0), ..Default::default() }.transition(Transition::new(120))
        })
        .into()
}

/// O texto de um chip na cor da tag, legível nos dois temas.
fn tinta(c: Color) -> Color {
    if claro() {
        c.mix(Color::BLACK, 0.35)
    } else {
        c.mix(Color::WHITE, 0.55)
    }
}

fn chip<'a>(tag: usize, _grande: bool) -> Element<'a, Msg> {
    let (nome, cor, _) = TAGS[tag];
    let c = hex(cor);
    let bola = container(Space::new(6.0, 6.0)).style(move |_| Style::default().background(c.mix(Color::WHITE, 0.4)).border(Border::rounded(3.0)));
    container(row![bola, t(nome, 10.5, tinta(c))].spacing(3).align(Align::Center))
        .padding(p4(1, 6, 1, 4))
        .style(move |_| Style::default().background(c.alpha(0.32)).border(Border::rounded(8.0)))
        .into()
}

/// O chip da linha de detalhes: `None` = cinza.
fn chip_grande<'a>(tag: Option<usize>, nome: &'a str) -> Element<'a, Msg> {
    let c = tag.map_or(hex("#8a90a0"), |i| hex(TAGS[i].1));
    let fundo = if tag.is_some() { c.alpha(0.3) } else { tom("#ffffff12", "#0000000d") };
    let bola = container(Space::new(7.0, 7.0)).style(move |_| Style::default().background(c.mix(Color::WHITE, 0.3)).border(Border::rounded(3.5)));
    container(row![bola, t(nome, 12.0, if tag.is_some() { tinta(c) } else { fraco() })].spacing(4).align(Align::Center))
        .padding(p4(4, 9, 4, 7))
        .style(move |_| Style::default().background(fundo).border(Border::rounded(12.0)))
        .into()
}

impl App for Caderno {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Arrastar => return Task::window(WindowCmd::Drag),
            Msg::Maximizar => return Task::window(WindowCmd::ToggleMaximize),
            Msg::Minimizar => return Task::window(WindowCmd::Minimize),
            Msg::Fechar => return Task::window(WindowCmd::Close),
            Msg::Puxar(d) => return Task::window(WindowCmd::DragResize(d)),
            Msg::Busca(s) => self.busca = s,
            Msg::Filtrar(f) => self.filtro = f,
            Msg::Selecionar(i) => {
                self.atual = i;
                self.garantir_capa();
            }
            Msg::Marcar(k, b) => self.notas[self.atual].metas[k].2 = b,
            Msg::Favorita => self.notas[self.atual].favorita ^= true,
            Msg::TrocarCapa => {
                let n = &mut self.notas[self.atual];
                n.capa = [0, 7, 5, 3, 1][([0, 7, 5, 3, 1].iter().position(|&c| c == n.capa).unwrap_or(0) + 1) % 5];
                self.garantir_capa();
            }
            Msg::Tema => CLARO.store(!claro(), Ordering::Relaxed),
            Msg::Nada => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let corpo = row![self.lateral(), self.lista(), self.editor()].height(Length::FILL);
        let janela = container(column![self.barra(), container(Space::new(Length::FILL, 1.0)).style(|_| Style::default().background(linha())), corpo])
            .width(Length::FILL)
            .height(Length::FILL)
            .clip(true)
            .style(|_| {
                Style::default()
                    .background(Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, tom("#1a2236d8", "#f4f6faea")), (1.0, tom("#111624e0", "#eef1f6ea"))] })
                    .border(Border::rounded(14.0).color(tom("#8fb4ff40", "#ffffffcc"), 1.0))
                    .shadow(Shadow { color: Color::BLACK.alpha(0.45), offset: Point::new(0.0, 12.0), blur: 30.0 })
                    .backdrop_blur(28.0)
            });
        let janela = container(janela).padding(self.margem);
        match &self.fundo {
            Some(f) => stack![image(f.clone()).width(Length::FILL).height(Length::FILL).content_fit(ContentFit::Cover), janela, self.alcas()].into(),
            None => stack![janela, self.alcas()].into(),
        }
    }

    fn theme(&self) -> Theme {
        let base = if claro() { Theme::light() } else { Theme::dark() };
        Theme {
            background: Color::TRANSPARENT,
            surface: tom("#1b2130", "#ffffff"),
            text: texto(),
            muted: mudo(),
            primary: acento(),
            on_primary: Color::WHITE,
            border: tom("#5d6577", "#a0a7b5"),
            ..base
        }
    }
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        CLARO.store(args.iter().any(|a| a == "--claro"), Ordering::Relaxed);
        let mut app = Caderno::new(true);
        app.margem = Padding { top: 27.0, right: 19.0, bottom: 25.0, left: 14.0 };
        let png = capture(&app.view(), Size::new(1090.0, 705.0), 1.0, &app.theme(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("notas.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    let fundo = args.iter().any(|a| a == "--fundo");
    let settings = Settings {
        title: "Pelicano Notes".into(),
        size: Size::new(1060.0, 660.0),
        min_size: Some(Size::new(960.0, 560.0)),
        decorations: false,
        transparent: true,
        blur: true,
        app_id: Some("pelicano-notes".into()),
        ..Default::default()
    };
    run(settings, move || (Caderno::new(fundo), Task::none()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn filtrar_e_marcar() {
        let mut app = Caderno::new(false);
        let _ = app.update(Msg::Busca("arch".into()));
        assert_eq!(app.visiveis(), [2]);
        let _ = app.update(Msg::Busca(String::new()));
        let _ = app.update(Msg::Filtrar(Filtro::Tag(1)));
        assert_eq!(app.visiveis(), [3, 6]);
        let _ = app.update(Msg::Selecionar(3));
        let _ = app.update(Msg::Marcar(1, true));
        assert!(app.notas[3].metas[1].2);
        assert!(app.capas[3].is_some());
    }
}
