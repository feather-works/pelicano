//! Pelicano Explorer: gerenciador de arquivos com terminal e monitor do sistema, tudo de verdade.
//! `cargo run --example explorer --features terminal`; `-- --capturar e.png` só gera o PNG.
mod icones;
mod sistema;

use icones::icone;
use pelicano::column;
use pelicano::*;
use sistema::{Entrada, Monitor};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MONO: Font = Font::with_name("JetBrainsMono Nerd Font");
const NEGRITO: Font = MONO.weight(700);
const ALTURA_BAIXO: f32 = 256.0;
const LARGURA_PREVIA: f32 = 410.0;
const COL_TAMANHO: f32 = 84.0;
const COL_TIPO: f32 = 104.0;
const COL_DATA: f32 = 142.0;

fn hex(s: &str) -> Color {
    Color::hex(s).unwrap_or(Color::BLACK)
}
fn fundo() -> Color {
    hex("#000608")
}
fn painel() -> Color {
    hex("#01080c")
}
fn borda() -> Color {
    hex("#04466b")
}
fn borda_acesa() -> Color {
    hex("#0b8ac6")
}
fn acento() -> Color {
    hex("#38bdf8")
}
fn texto() -> Color {
    hex("#c6d1df")
}
fn mudo() -> Color {
    hex("#7f8fa6")
}
fn selecao() -> Color {
    hex("#002c4a")
}

fn t<'a>(s: impl Into<Cow<'a, str>>) -> Text<'a> {
    text(s).font(MONO).size(12).color(texto()).no_wrap()
}

fn ic<'a>(nome: &'static str, tam: f32, cor: Color) -> Element<'a, Msg> {
    svg(icone(nome)).width(tam).height(tam).color(cor).into()
}

fn caixa(fundo: Color, cor_borda: Color, raio: f32) -> Style {
    Style::default().background(fundo).border(Border::rounded(raio).color(cor_borda, 1.0))
}

fn estilo_painel(_: &Theme) -> Style {
    caixa(painel(), borda(), 4.0)
}

/// Botão só de ícone: sem fundo, acende no hover.
fn botao_icone(_: &Theme, s: Status) -> Style {
    let bg = match s {
        Status::Hovered => Some(acento().alpha(0.10)),
        Status::Pressed => Some(acento().alpha(0.18)),
        _ => None,
    };
    Style { background: bg.map(Into::into), border: Border::rounded(4.0), ..Default::default() }
}

fn linha_h<'a>() -> Element<'a, Msg> {
    container(Space::new(Length::FILL, 1.0)).width(Length::FILL).style(|_| Style::default().background(borda())).into()
}

fn linha_v<'a>(h: impl Into<Length>) -> Element<'a, Msg> {
    container(Space::new(1.0, h)).style(|_| Style::default().background(borda())).into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Aba {
    Arquivos,
    Terminal,
    Monitor,
}

impl Aba {
    fn rotulo(self) -> (&'static str, &'static str) {
        match self {
            Aba::Arquivos => ("pasta", "Arquivos"),
            Aba::Terminal => ("terminal", "Terminal"),
            Aba::Monitor => ("pulso", "Monitor do Sistema"),
        }
    }
}

#[derive(Debug, Clone)]
enum Msg {
    Arrastar,
    Maximizar,
    Minimizar,
    Fechar,
    Puxar(ResizeDirection),
    Ir(PathBuf),
    Voltar,
    Avancar,
    Subir,
    Caminho(String),
    Busca(String),
    Selecionar(PathBuf),
    Ativar(PathBuf),
    Lateral(usize),
    Grade(bool),
    Aba(Aba),
    FecharAba(Aba),
    NovaAba,
    AbaPrevia(usize),
    AbaBaixo(usize),
    AbaMonitor(usize),
    Tique,
    Term(TerminalEvent),
}

enum Previa {
    Nada,
    Imagem(Image),
    Texto(String),
}

/// O que se sabe do item escolhido, calculado uma vez na seleção.
struct Detalhe {
    entrada: Entrada,
    previa: Previa,
    /// Só para pastas: (arquivos, pastas, bytes).
    conteudo: Option<(usize, usize, u64)>,
}

/// (ícone, rótulo, destino). `None` no destino: abre uma aba.
fn lateral() -> Vec<(&'static str, &'static str, Option<PathBuf>)> {
    let h = sistema::home();
    let favorito = std::fs::read_to_string(h.join(".config/gtk-3.0/bookmarks"))
        .ok()
        .and_then(|s| s.lines().next().and_then(|l| l.split_whitespace().next()).and_then(|u| u.strip_prefix("file://")).map(PathBuf::from))
        .filter(|p| p.is_dir());
    vec![
        ("casa", "Início", Some(h.clone())),
        ("pasta", "Arquivos", Some("/".into())),
        ("grade", "Aplicativos", Some("/usr/share/applications".into())),
        ("engrenagem", "Configurações", Some(h.join(".config"))),
        ("monitor", "Sistema", Some("/etc".into())),
        ("terminal", "Terminal", None),
        ("grafico", "Monitor", None),
        ("marcador", "Favoritos", favorito.or_else(|| Some(h.clone()))),
    ]
}

fn dispositivos() -> Vec<(&'static str, &'static str, Option<PathBuf>)> {
    use sistema::pasta_xdg as x;
    vec![
        ("arquivo", "Arquivo", Some("/".into())),
        ("pasta", "Projetos", x("PROJECTS", &["Projetos", "Projects", "projetos", "GitHub", "dev", "src"])),
        ("download", "Downloads", x("DOWNLOAD", &["Downloads", "Transferências"])),
        ("imagem", "Imagens", x("PICTURES", &["Imagens", "Pictures"])),
        ("documento", "Documentos", x("DOCUMENTS", &["Documentos", "Documents"])),
        ("video", "Vídeos", x("VIDEOS", &["Vídeos", "Videos"])),
        ("musica", "Música", x("MUSIC", &["Música", "Music"])),
        ("lixeira", "Lixeira", Some(sistema::home().join(".local/share/Trash/files")).filter(|p| p.is_dir())),
    ]
}

struct Explorer {
    dir: PathBuf,
    caminho: String,
    voltar: Vec<PathBuf>,
    avancar: Vec<PathBuf>,
    entradas: Vec<Entrada>,
    erro: Option<String>,
    busca: String,
    detalhe: Option<Detalhe>,
    grade: bool,
    atalhos: Vec<(&'static str, &'static str, Option<PathBuf>)>,
    lateral: usize,
    abas: Vec<Aba>,
    aba: Aba,
    aba_previa: usize,
    aba_baixo: usize,
    aba_monitor: usize,
    term: Option<Terminal>,
    monitor: Monitor,
    logs: Vec<String>,
    processos: Vec<(u32, String, u64)>,
}

impl Explorer {
    fn new(term: Option<Terminal>) -> Explorer {
        let atalhos = lateral().into_iter().chain(dispositivos()).collect();
        let mut e = Explorer {
            dir: PathBuf::new(),
            caminho: String::new(),
            voltar: Vec::new(),
            avancar: Vec::new(),
            entradas: Vec::new(),
            erro: None,
            busca: String::new(),
            detalhe: None,
            grade: false,
            atalhos,
            lateral: 0,
            abas: vec![Aba::Arquivos, Aba::Terminal, Aba::Monitor],
            aba: Aba::Arquivos,
            aba_previa: 0,
            aba_baixo: 0,
            aba_monitor: 0,
            term,
            monitor: Monitor::new(),
            logs: Vec::new(),
            processos: Vec::new(),
        };
        e.carregar(sistema::home());
        e
    }

    fn log(&mut self, s: String) {
        let hora = sistema::data(Some(std::time::SystemTime::now()));
        self.logs.push(format!("[{}] {s}", &hora[11..]));
        if self.logs.len() > 200 {
            self.logs.remove(0);
        }
    }

    fn carregar(&mut self, dir: PathBuf) {
        match sistema::listar(&dir) {
            Ok(v) => {
                self.entradas = v;
                self.erro = None;
                self.log(format!("aberto {}", dir.display()));
            }
            Err(e) => {
                self.entradas.clear();
                self.erro = Some(e.to_string());
                self.log(format!("erro em {}: {e}", dir.display()));
            }
        }
        self.caminho = dir.display().to_string();
        self.busca.clear();
        // A própria pasta fica no painel até escolherem um item.
        let atual = Entrada { nome: dir.file_name().map_or("/".into(), |n| n.to_string_lossy().into_owned()), caminho: dir.clone(), pasta: true, tamanho: 0, modificado: std::fs::metadata(&dir).and_then(|m| m.modified()).ok() };
        self.dir = dir;
        self.detalhar(atual);
    }

    fn ir(&mut self, dir: PathBuf) {
        if dir != self.dir {
            self.voltar.push(std::mem::take(&mut self.dir));
            self.avancar.clear();
            self.carregar(dir);
        }
    }

    fn detalhar(&mut self, e: Entrada) {
        let (previa, conteudo) = if e.pasta {
            // Pasta mostra só o ícone: abrir a primeira foto de dentro expunha fotos pessoais sem pedir.
            let (arqs, pastas, bytes) = sistema::resumo_pasta(&e.caminho);
            (Previa::Nada, Some((arqs, pastas, bytes)))
        } else if let Some(img) = sistema::carregar_imagem(&e.caminho) {
            (Previa::Imagem(img), None)
        } else {
            (sistema::primeiras_linhas(&e.caminho, 9).map_or(Previa::Nada, Previa::Texto), None)
        };
        self.detalhe = Some(Detalhe { entrada: e, previa, conteudo });
    }

    fn visiveis(&self) -> impl Iterator<Item = &Entrada> {
        let b = self.busca.to_lowercase();
        self.entradas.iter().filter(move |e| b.is_empty() || e.nome.to_lowercase().contains(&b))
    }

    fn selecionado(&self, p: &Path) -> bool {
        self.detalhe.as_ref().is_some_and(|d| d.entrada.caminho == p)
    }

    // ---------- barra de título e lateral ----------

    fn barra(&self) -> Element<'_, Msg> {
        let botao = |nome, tam, m| button(ic(nome, tam, texto())).padding([8, 10]).style(botao_icone).on_press(m);
        let conteudo = row![
            ic("logo", 22.0, acento()),
            Space::new(8.0, 1.0),
            t("Pelicano Explorer").size(13.5).font(MONO.weight(600)).color(acento()),
            Space::new(10.0, 1.0),
            t("v0.1.0").size(12).color(mudo()),
            horizontal_space(),
            botao("minimizar", 18.0, Msg::Minimizar),
            botao("maximizar", 15.0, Msg::Maximizar),
            botao("x", 19.0, Msg::Fechar),
        ]
        .align(Align::Center)
        .padding([0, 8]);
        let barra = container(conteudo).padding(Padding { top: 0.0, right: 12.0, bottom: 0.0, left: 18.0 }).height(40).width(Length::FILL).align_y(Align::Center).style(|_| Style::default().background(fundo()));
        column![mouse_area(barra).on_press(Msg::Arrastar).on_double_click(Msg::Maximizar), linha_h()].into()
    }

    fn item_lateral(&self, i: usize) -> Element<'_, Msg> {
        let (nome, rotulo, ref destino) = self.atalhos[i];
        let ativo = self.lateral == i;
        let existe = destino.is_some() || i == 5 || i == 6;
        let cor_txt = if !existe { mudo() } else if ativo { acento() } else { texto() };
        let conteudo = row![ic(nome, 18.0, if existe { acento() } else { mudo() }), t(rotulo).size(12.5).color(cor_txt).font(if ativo { NEGRITO } else { MONO })].spacing(12).align(Align::Center).width(Length::FILL);
        button(conteudo)
            .width(Length::FILL)
            .padding([5, 10])
            .style(move |th, s| if ativo { caixa(hex("#00253c"), borda_acesa(), 4.0) } else { botao_icone(th, s) })
            .on_press_maybe(existe.then_some(Msg::Lateral(i)))
            .into()
    }

    fn lateral(&self) -> Element<'_, Msg> {
        let topo = column((0..8).map(|i| self.item_lateral(i))).spacing(4);
        let baixo = column((8..self.atalhos.len()).map(|i| self.item_lateral(i))).spacing(3);
        let conteudo = column![
            topo,
            Space::new(1.0, 16.0),
            linha_h(),
            Space::new(1.0, 14.0),
            container(t("Dispositivos").font(NEGRITO).color(acento())).padding([0, 8]),
            Space::new(1.0, 8.0),
            baixo,
        ]
        .padding([20, 10])
        .width(Length::FILL);
        row![container(conteudo).width(174).height(Length::FILL), linha_v(Length::FILL)].into()
    }

    // ---------- abas e barra de ferramentas ----------

    fn abas(&self) -> Element<'_, Msg> {
        let mut r = row(Vec::<Element<Msg>>::new()).spacing(6).align(Align::End);
        for &a in &self.abas {
            let (nome, rotulo) = a.rotulo();
            r = r.push(aba(nome, rotulo, self.aba == a, Msg::Aba(a), (self.abas.len() > 1).then_some(Msg::FecharAba(a)), true, false));
        }
        let mais = button(ic("mais", 18.0, acento())).padding([5, 7]).style(|_, s| if s == Status::Hovered { caixa(selecao(), borda_acesa(), 5.0) } else { caixa(painel(), borda(), 5.0) });
        r = r.push(mais.on_press_maybe((self.abas.len() < 3).then_some(Msg::NovaAba)));
        r.into()
    }

    fn ferramentas(&self) -> Element<'_, Msg> {
        let nav = |nome, m: Option<Msg>| {
            let cor = if m.is_some() { acento() } else { acento().alpha(0.45) };
            button(ic(nome, 20.0, cor)).padding([6, 8]).style(|_, s| match s {
                Status::Hovered => caixa(selecao(), borda(), 4.0),
                _ => Style::default().background(painel()).border(Border::rounded(4.0)),
            }).on_press_maybe(m)
        };
        let campo = |icone_nome, valor: &str, dica: &'static str, on: fn(String) -> Msg, enviar: Option<Msg>| {
            let mut entrada = text_input(dica, valor.to_string()).font(MONO).size(12.5).padding([6, 4]).on_input(on).style(|_, _| Style { text_color: Some(texto()), ..Default::default() });
            if let Some(m) = enviar {
                entrada = entrada.on_submit(m);
            }
            container(row![ic(icone_nome, 20.0, acento()), entrada].spacing(10).align(Align::Center)).padding([2, 12]).height(34).align_y(Align::Center).style(|_| caixa(painel(), borda(), 4.0))
        };
        let vista = |nome, ativo: bool, m| {
            button(ic(nome, 18.0, acento())).padding([6, 9]).style(move |th, s| if ativo { caixa(selecao(), borda_acesa(), 4.0) } else { botao_icone(th, s) }).on_press(m)
        };
        row![
            nav("esquerda", (!self.voltar.is_empty()).then_some(Msg::Voltar)),
            nav("direita", (!self.avancar.is_empty()).then_some(Msg::Avancar)),
            nav("cima", self.dir.parent().map(|_| Msg::Subir)),
            Space::new(8.0, 1.0),
            campo("pasta", &self.caminho, "/", Msg::Caminho, Some(Msg::Ir(PathBuf::from(&self.caminho)))).width(Length::FILL),
            Space::new(2.0, 1.0),
            campo("busca", &self.busca, "Pesquisar...", Msg::Busca, None).width(190),
            container(row![vista("lista", !self.grade, Msg::Grade(false)), vista("grade", self.grade, Msg::Grade(true)), vista("reticencias", false, Msg::Aba(Aba::Monitor))].spacing(2))
                .padding(1)
                .style(|_| caixa(painel(), borda(), 4.0)),
        ]
        .spacing(6)
        .align(Align::Center)
        .into()
    }

    // ---------- lista de arquivos ----------

    fn tabela(&self) -> Element<'_, Msg> {
        let cel = |e: Element<'static, Msg>, w: Length| container(e).width(w).padding([0, 10]).clip(true);
        let cab = |s: &'static str| -> Element<'static, Msg> { t(s).font(NEGRITO).color(acento()).into() };
        let cabecalho = container(
            row![
                cel(cab("Nome"), Length::FILL),
                linha_v(28.0),
                cel(cab("Tamanho"), COL_TAMANHO.into()),
                linha_v(28.0),
                cel(cab("Tipo"), COL_TIPO.into()),
                linha_v(28.0),
                cel(cab("Modificado"), COL_DATA.into()),
            ]
            .align(Align::Center),
        )
        .width(Length::FILL)
        .style(|_| Style::default().background(hex("#011421")).border(Border { radius: [4.0, 4.0, 0.0, 0.0], ..Border::default() }));

        let corpo: Element<Msg> = if let Some(erro) = &self.erro {
            container(t(format!("Não deu para abrir: {erro}")).color(mudo())).padding(14).into()
        } else if self.grade {
            let cartoes = self.visiveis().map(|e| {
                let sel = self.selecionado(&e.caminho);
                let (_, nome_icone) = sistema::tipo(e);
                let cartao = container(column![ic(nome_icone, 40.0, acento()), container(t(e.nome.clone()).size(12)).width(Length::FILL).align_x(Align::Center).clip(true)].spacing(6).align(Align::Center))
                    .width(104)
                    .height(86)
                    .padding([10, 6])
                    .style(move |_| if sel { caixa(selecao(), borda_acesa(), 4.0) } else { Style::default() });
                mouse_area(cartao).on_press(Msg::Selecionar(e.caminho.clone())).on_double_click(Msg::Ativar(e.caminho.clone())).into()
            });
            scrollable(row(cartoes.collect::<Vec<Element<Msg>>>()).wrap().spacing(6).padding(6).width(Length::FILL)).into()
        } else {
            let linhas = self.visiveis().map(|e| {
                let sel = self.selecionado(&e.caminho);
                let (tipo, nome_icone) = sistema::tipo(e);
                let tamanho = if e.pasta { "-".to_string() } else { sistema::tamanho_humano(e.tamanho) };
                let nome = row![ic(nome_icone, 17.0, acento()), t(e.nome.clone())].spacing(11).align(Align::Center);
                let linha = container(
                    row![
                        cel(nome.into(), Length::FILL),
                        cel(t(tamanho).into(), (COL_TAMANHO + 1.0).into()),
                        cel(t(tipo).into(), (COL_TIPO + 1.0).into()),
                        cel(t(sistema::data(e.modificado)).into(), (COL_DATA + 1.0).into()),
                    ]
                    .align(Align::Center),
                )
                .height(24.5)
                .width(Length::FILL)
                .align_y(Align::Center)
                .style(move |_| if sel { caixa(selecao(), borda_acesa(), 3.0) } else { Style::default() });
                mouse_area(linha).on_press(Msg::Selecionar(e.caminho.clone())).on_double_click(Msg::Ativar(e.caminho.clone())).into()
            });
            scrollable(column(linhas.collect::<Vec<Element<Msg>>>()).padding([2, 1])).into()
        };
        let topo: Option<Element<Msg>> = (!self.grade).then(|| column![cabecalho, linha_h()].into());
        container(column(topo.into_iter().chain([corpo]).collect::<Vec<_>>())).width(Length::FILL).height(Length::FILL).clip(true).style(estilo_painel).into()
    }

    // ---------- painel de prévia ----------

    fn previa(&self) -> Element<'_, Msg> {
        let abas_previa = row![
            aba("imagem", "Visualização", self.aba_previa == 0, Msg::AbaPrevia(0), None, false, true),
            aba("info", "Informações", self.aba_previa == 1, Msg::AbaPrevia(1), None, false, true),
            aba("ajustes", "Propriedades", self.aba_previa == 2, Msg::AbaPrevia(2), None, false, true),
        ]
        .spacing(4)
        .padding([3, 3]);
        let corpo: Element<Msg> = match &self.detalhe {
            None => container(t("Nada selecionado").color(mudo())).padding(14).into(),
            Some(d) => match self.aba_previa {
                0 => self.visualizacao(d),
                1 => self.informacoes(d),
                _ => self.propriedades(d),
            },
        };
        container(column![abas_previa, linha_h(), corpo]).width(LARGURA_PREVIA).height(Length::FILL).clip(true).style(estilo_painel).into()
    }

    fn visualizacao<'a>(&'a self, d: &'a Detalhe) -> Element<'a, Msg> {
        let e = &d.entrada;
        let (tipo, nome_icone) = sistema::tipo(e);
        let quadro = |conteudo: Element<'a, Msg>| container(conteudo).width(Length::FILL).height(168).clip(true).style(|_| caixa(hex("#020910"), borda(), 4.0));
        let topo: Element<Msg> = match &d.previa {
            Previa::Imagem(img) => image(img.clone()).content_fit(ContentFit::Cover).width(Length::FILL).height(168).radius(4.0).into(),
            Previa::Texto(s) => quadro(text(s.clone()).font(MONO).size(11.5).color(texto()).into()).padding([8, 10]).into(),
            Previa::Nada => quadro(ic(if e.pasta { "pasta" } else { nome_icone }, 64.0, acento().alpha(0.6))).center().into(),
        };
        let descricao = if e.pasta { "Pasta de arquivos".to_string() } else { format!("Arquivo {}", tipo.to_lowercase()) };
        let cabeca = row![
            ic(if e.pasta { "pasta" } else { nome_icone }, 34.0, acento()),
            column![t(e.nome.clone()).size(14).font(NEGRITO).color(acento()), t(descricao)].spacing(3),
        ]
        .spacing(16)
        .align(Align::Center)
        .padding([0, 6]);
        let local = e.caminho.display().to_string();
        let mut kv = vec![("Local:", local)];
        match d.conteudo {
            Some((a, p, b)) => {
                kv.push(("Itens:", format!("{a} arquivos, {p} pastas")));
                kv.push(("Tamanho:", sistema::tamanho_humano(b)));
            }
            None => kv.push(("Tamanho:", sistema::tamanho_humano(e.tamanho))),
        }
        kv.push(("Modificado:", sistema::data(e.modificado)));
        column![topo, cabeca, linha_h(), tabela_chave_valor(kv)].spacing(8).padding([6, 8]).into()
    }

    fn informacoes<'a>(&'a self, d: &'a Detalhe) -> Element<'a, Msg> {
        let e = &d.entrada;
        let md = std::fs::symlink_metadata(&e.caminho).ok();
        let (tipo, _) = sistema::tipo(e);
        let mut kv = vec![
            ("Nome:", e.nome.clone()),
            ("Tipo:", tipo.to_string()),
            ("Extensão:", sistema::extensao(&e.caminho)),
            ("Pasta:", e.caminho.parent().map_or("-".into(), |p| p.display().to_string())),
            ("Tamanho:", format!("{} ({} bytes)", sistema::tamanho_humano(e.tamanho), e.tamanho)),
            ("Modificado:", sistema::data(e.modificado)),
            ("Acessado:", sistema::data(md.as_ref().and_then(|m| m.accessed().ok()))),
            ("Criado:", sistema::data(md.as_ref().and_then(|m| m.created().ok()))),
        ];
        if md.as_ref().is_some_and(|m| m.file_type().is_symlink()) {
            kv.push(("Link para:", std::fs::read_link(&e.caminho).map_or("?".into(), |p| p.display().to_string())));
        }
        container(tabela_chave_valor(kv)).padding([12, 8]).into()
    }

    fn propriedades<'a>(&'a self, d: &'a Detalhe) -> Element<'a, Msg> {
        use std::os::unix::fs::MetadataExt;
        let Ok(md) = std::fs::metadata(&d.entrada.caminho) else {
            return container(t("Sem acesso").color(mudo())).padding(14).into();
        };
        let modo = md.mode();
        let rwx: String = (0..9).rev().map(|i| if modo >> i & 1 == 1 { ['x', 'w', 'r'][i % 3] } else { '-' }).collect();
        let kv = vec![
            ("Permissões:", format!("{}{rwx} ({:o})", if md.is_dir() { 'd' } else { '-' }, modo & 0o7777)),
            ("Dono:", format!("uid {}", md.uid())),
            ("Grupo:", format!("gid {}", md.gid())),
            ("Inode:", md.ino().to_string()),
            ("Links:", md.nlink().to_string()),
            ("Blocos:", format!("{} × 512 B", md.blocks())),
            ("Só leitura:", if md.permissions().readonly() { "sim" } else { "não" }.into()),
        ];
        container(tabela_chave_valor(kv)).padding([12, 8]).into()
    }

    // ---------- painéis de baixo ----------

    fn terminal_view(&self) -> Element<'_, Msg> {
        match &self.term {
            Some(term) => terminal(term).size(12.5).font(MONO).background(Color::BLACK).text_color(texto()).padding(10.0).into(),
            None => container(t("O shell não abriu.").color(mudo())).padding(14).into(),
        }
    }

    fn painel_terminal(&self) -> Element<'_, Msg> {
        let abas_baixo = row![
            aba("terminal", "Terminal", self.aba_baixo == 0, Msg::AbaBaixo(0), None, true, false),
            aba("download", "Logs", self.aba_baixo == 1, Msg::AbaBaixo(1), None, true, false),
            aba("lista", "Tarefas", self.aba_baixo == 2, Msg::AbaBaixo(2), None, true, false),
        ]
        .spacing(0);
        let corpo: Element<Msg> = match self.aba_baixo {
            0 => self.terminal_view(),
            1 => scrollable(column(self.logs.iter().rev().map(|l| t(l.as_str()).color(texto()).into()).collect::<Vec<Element<Msg>>>()).spacing(3).padding(12)).into(),
            _ => {
                let linha = |a: String, b: String, c: String, cor: Color| -> Element<Msg> {
                    row![t(a).width(70).color(cor), t(b).width(Length::FILL).color(cor), t(c).color(cor)].padding([0, 4]).into()
                };
                let cab = linha("PID".into(), "Processo".into(), "Memória".into(), acento());
                let linhas = self.processos.iter().map(|(pid, nome, b)| linha(pid.to_string(), nome.clone(), sistema::tamanho_humano(*b), texto()));
                scrollable(column(std::iter::once(cab).chain(linhas).collect::<Vec<Element<Msg>>>()).spacing(3).padding(12)).into()
            }
        };
        container(column![abas_baixo, linha_h(), corpo]).width(Length::FILL).height(Length::FILL).clip(true).style(estilo_painel).into()
    }

    /// (título, linhas de texto, série, máximo da escala, rótulo no gráfico)
    fn metrica(&self, i: usize) -> (&'static str, Vec<String>, &[f32], f32, String) {
        let m = &self.monitor;
        let ultimo = |v: &[f32]| v.last().copied().unwrap_or(0.0);
        // CPU e disco em escala automática (piso de 10%), para a forma aparecer com a máquina parada.
        let escala = |v: &[f32]| (v.iter().copied().fold(0.0, f32::max) * 1.25).clamp(10.0, 100.0);
        let gb = |b: u64| format!("{:.1} GB", b as f64 / (1u64 << 30) as f64);
        match i {
            0 => ("CPU", vec![m.cpu_nome.clone(), format!("{:.1} GHz", m.cpu_ghz), format!("Uso: {:.0}%", ultimo(&m.cpu))], &m.cpu, escala(&m.cpu), format!("{:.0}%", ultimo(&m.cpu))),
            1 => ("Memória", vec![format!("{} / {}", gb(m.mem_usada), gb(m.mem_total)), format!("Uso: {:.0}%", ultimo(&m.mem))], &m.mem, 100.0, format!("{:.0}%", ultimo(&m.mem))),
            2 => ("Disco", vec![m.disco_nome.clone(), format!("Uso: {:.0}%", ultimo(&m.disco))], &m.disco, escala(&m.disco), format!("{:.0}%", ultimo(&m.disco))),
            _ => {
                let max = m.rede.iter().copied().fold(1.0, f32::max);
                let taxa = |k: f64| if k >= 1024.0 { format!("{:.1} MB/s", k / 1024.0) } else { format!("{k:.0} KB/s") };
                ("Rede", vec![format!("↓ {}", taxa(m.rede_rx)), format!("↑ {}", taxa(m.rede_tx))], &m.rede, max * 1.15, taxa(m.rede_rx + m.rede_tx))
            }
        }
    }

    fn cartao_metrica(&self, i: usize, largura: Length, altura: f32, grande: bool) -> Element<'_, Msg> {
        let (titulo, linhas, serie, max, rotulo) = self.metrica(i);
        let info = column(std::iter::once(t(titulo).font(NEGRITO).color(acento()).into()).chain(linhas.into_iter().map(|l| t(l).into())).collect::<Vec<Element<Msg>>>()).spacing(if grande { 4 } else { 1 });
        if grande {
            column![info, grafico(serie.to_vec(), max, rotulo, Length::FILL, Length::FILL)].spacing(10).into()
        } else {
            row![grafico(serie.to_vec(), max, rotulo, largura, altura.into()), info].spacing(18).align(Align::Center).into()
        }
    }

    fn painel_monitor(&self) -> Element<'_, Msg> {
        let abas_mon = row![
            aba("cpu", "CPU", self.aba_monitor == 0, Msg::AbaMonitor(0), None, true, true),
            aba("memoria", "Memória", self.aba_monitor == 1, Msg::AbaMonitor(1), None, true, true),
            aba("disco", "Disco", self.aba_monitor == 2, Msg::AbaMonitor(2), None, true, true),
            aba("rede", "Rede", self.aba_monitor == 3, Msg::AbaMonitor(3), None, true, true),
        ];
        // A aba escolhida vem primeiro, seguida das outras.
        let mut corpo = column(Vec::<Element<Msg>>::new()).padding([4, 8]);
        for k in 0..3 {
            if k > 0 {
                corpo = corpo.push(linha_h());
            }
            corpo = corpo.push(container(self.cartao_metrica((self.aba_monitor + k) % 4, 188.0.into(), 56.0, false)).padding([4, 2]));
        }
        container(column![abas_mon, linha_h(), corpo]).width(366).height(Length::FILL).clip(true).style(estilo_painel).into()
    }

    // ---------- áreas das abas de cima ----------

    fn area_arquivos(&self) -> Element<'_, Msg> {
        column![
            self.ferramentas(),
            row![self.tabela(), self.previa()].spacing(8).height(Length::FILL),
            row![self.painel_terminal(), self.painel_monitor()].spacing(6).height(ALTURA_BAIXO),
        ]
        .spacing(8)
        .into()
    }

    fn area_monitor(&self) -> Element<'_, Msg> {
        let cartao = |i| container(self.cartao_metrica(i, Length::FILL, 190.0, true)).padding(16).width(Length::FILL).height(Length::FILL).style(estilo_painel);
        column![row![cartao(0), cartao(1)].spacing(8).height(Length::FILL), row![cartao(2), cartao(3)].spacing(8).height(Length::FILL)].spacing(8).into()
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

/// Uma aba: caixa com borda em cima e dos lados. `caixa_inativa`: as inativas também têm moldura.
fn aba<'a>(nome: &'static str, rotulo: &'a str, ativa: bool, m: Msg, fechar: Option<Msg>, caixa_inativa: bool, encher: bool) -> Element<'a, Msg> {
    let cor = if ativa { acento() } else { mudo() };
    let mut conteudo = row![ic(nome, 16.0, cor), t(rotulo).color(if ativa { acento() } else { texto().mix(mudo(), 0.4) }).font(if ativa { NEGRITO } else { MONO })].spacing(if encher { 7 } else { 10 }).align(Align::Center);
    if let Some(f) = fechar {
        conteudo = conteudo.push(Space::new(4.0, 1.0)).push(button(ic("x", 15.0, if ativa { acento() } else { mudo() })).padding(2).style(botao_icone).on_press(f));
    }
    let estilo = move |_: &Theme| {
        let raio = Border { radius: [5.0, 5.0, 0.0, 0.0], ..Border::default() };
        if ativa {
            Style::default().background(hex("#00131f")).border(Border { color: borda_acesa(), width: 1.0, ..raio })
        } else if caixa_inativa {
            Style::default().background(painel()).border(Border { color: borda(), width: 1.0, ..raio })
        } else {
            Style::default()
        }
    };
    let largura = if encher { Length::FILL } else { Length::Shrink };
    mouse_area(container(conteudo).height(30).width(largura).padding([5, if encher { 4 } else { 10 }]).align_x(Align::Center).align_y(Align::Center).style(estilo)).on_press(m).into()
}

fn tabela_chave_valor<'a>(kv: Vec<(&'static str, String)>) -> Element<'a, Msg> {
    column(kv.into_iter().map(|(k, v)| row![t(k).width(98), t(v)].padding([0, 4]).into()).collect::<Vec<Element<Msg>>>()).spacing(2).into()
}

/// Gráfico de área: a série encostada na direita, preenchida com degradê.
fn grafico<'a>(serie: Vec<f32>, max: f32, rotulo: String, w: Length, h: Length) -> Element<'a, Msg> {
    canvas(w, h, move |r, b, _| {
        r.fill_quad(b, Some(&Background::Color(hex("#020a12"))), Border::rounded(3.0).color(borda_acesa().alpha(0.75), 1.0), None);
        let area = Rect { x: b.x + 1.5, y: b.y + 6.0, width: b.width - 3.0, height: b.height - 8.0 };
        let passo = area.width / 59.0;
        let base = area.y + area.height;
        let pts: Vec<Point> = serie
            .iter()
            .enumerate()
            .map(|(i, v)| Point::new(area.x + area.width - (serie.len() - 1 - i) as f32 * passo, base - area.height * (v / max.max(0.001)).clamp(0.0, 1.0) * 0.82))
            .collect();
        if pts.len() >= 2 {
            let caminho = pelicano::Path::new(|p| {
                p.move_to(Point::new(pts[0].x, base));
                for q in &pts {
                    p.line_to(*q);
                }
                p.line_to(Point::new(pts[pts.len() - 1].x, base)).close();
            });
            if let Some(c) = caminho {
                let degrade = Background::Linear { angle: std::f32::consts::FRAC_PI_2, stops: vec![(0.0, acento().alpha(0.55)), (1.0, acento().alpha(0.08))] };
                r.fill_path(&c, degrade, FillRule::NonZero);
            }
            r.stroke(&pts, 1.3, acento());
        }
        let tam = r.measure(&rotulo, 12.5, MONO, None);
        r.draw_text(&rotulo, 12.5, MONO, texto(), Point::new(b.x + b.width - tam.width - 8.0, b.y + 5.0), None);
    })
    .into()
}

impl App for Explorer {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Arrastar => return Task::window(WindowCmd::Drag),
            Msg::Maximizar => return Task::window(WindowCmd::ToggleMaximize),
            Msg::Minimizar => return Task::window(WindowCmd::Minimize),
            Msg::Fechar => return Task::window(WindowCmd::Close),
            Msg::Puxar(d) => return Task::window(WindowCmd::DragResize(d)),
            Msg::Ir(p) => {
                if p.is_dir() {
                    self.ir(p);
                } else {
                    self.log(format!("não é uma pasta: {}", p.display()));
                    self.caminho = self.dir.display().to_string();
                }
            }
            Msg::Voltar => {
                if let Some(p) = self.voltar.pop() {
                    self.avancar.push(std::mem::take(&mut self.dir));
                    self.carregar(p);
                }
            }
            Msg::Avancar => {
                if let Some(p) = self.avancar.pop() {
                    self.voltar.push(std::mem::take(&mut self.dir));
                    self.carregar(p);
                }
            }
            Msg::Subir => {
                if let Some(p) = self.dir.parent().map(Path::to_path_buf) {
                    self.ir(p);
                }
            }
            Msg::Caminho(s) => self.caminho = s,
            Msg::Busca(s) => self.busca = s,
            Msg::Selecionar(p) => {
                if let Some(e) = self.entradas.iter().find(|e| e.caminho == p).cloned() {
                    self.detalhar(e);
                }
            }
            Msg::Ativar(p) => {
                if p.is_dir() {
                    self.ir(p);
                } else {
                    let r = std::process::Command::new("xdg-open").arg(&p).spawn();
                    self.log(match r {
                        Ok(_) => format!("abrindo {}", p.display()),
                        Err(e) => format!("xdg-open falhou: {e}"),
                    });
                }
            }
            Msg::Lateral(i) => {
                self.lateral = i;
                match (i, self.atalhos[i].2.clone()) {
                    (5, _) => return self.update(Msg::Aba(Aba::Terminal)),
                    (6, _) => return self.update(Msg::Aba(Aba::Monitor)),
                    (_, Some(p)) => {
                        self.aba = Aba::Arquivos;
                        if !self.abas.contains(&Aba::Arquivos) {
                            self.abas.insert(0, Aba::Arquivos);
                        }
                        self.ir(p);
                    }
                    _ => {}
                }
            }
            Msg::Grade(g) => self.grade = g,
            Msg::Aba(a) => {
                if !self.abas.contains(&a) {
                    self.abas.push(a);
                }
                self.aba = a;
            }
            Msg::FecharAba(a) => {
                self.abas.retain(|x| *x != a);
                if self.aba == a {
                    self.aba = self.abas[0];
                }
            }
            Msg::NovaAba => {
                if let Some(a) = [Aba::Arquivos, Aba::Terminal, Aba::Monitor].into_iter().find(|a| !self.abas.contains(a)) {
                    return self.update(Msg::Aba(a));
                }
            }
            Msg::AbaPrevia(i) => self.aba_previa = i,
            Msg::AbaBaixo(i) => {
                self.aba_baixo = i;
                self.processos = sistema::processos(40);
            }
            Msg::AbaMonitor(i) => self.aba_monitor = i,
            Msg::Tique => {
                self.monitor.amostrar();
                if self.aba_baixo == 2 {
                    self.processos = sistema::processos(40);
                }
            }
            Msg::Term(TerminalEvent::Exit(c)) => {
                self.log(format!("o shell saiu ({c:?})"));
                self.term = None;
            }
            Msg::Term(TerminalEvent::Title(s)) => self.log(format!("terminal: {s}")),
            Msg::Term(_) => {}
        }
        Task::none()
    }

    fn theme(&self) -> Theme {
        Theme { background: fundo(), surface: painel(), text: texto(), muted: mudo(), primary: acento(), border: borda(), ..Theme::dark() }
    }

    fn view(&self) -> Element<'_, Msg> {
        let area: Element<Msg> = match self.aba {
            Aba::Arquivos => self.area_arquivos(),
            Aba::Terminal => container(self.terminal_view()).width(Length::FILL).height(Length::FILL).clip(true).style(estilo_painel).into(),
            Aba::Monitor => self.area_monitor(),
        };
        let principal = column![self.abas(), area].spacing(6).padding(Padding { top: 10.0, right: 6.0, bottom: 12.0, left: 6.0 }).width(Length::FILL).height(Length::FILL);
        let corpo = column![self.barra(), row![self.lateral(), principal].height(Length::FILL)];
        container(stack![corpo, self.alcas()])
            .width(Length::FILL)
            .height(Length::FILL)
            .clip(true)
            .style(|_| caixa(fundo(), borda(), 0.0).text_color(texto()))
            .into()
    }

    fn subscription(&self) -> Subscription<Msg> {
        let relogio = Subscription::every(Duration::from_secs(1)).map(|_| Msg::Tique);
        match &self.term {
            Some(term) => Subscription::batch([relogio, term.subscription().map(Msg::Term)]),
            None => relogio,
        }
    }
}

fn abrir_terminal() -> Option<Terminal> {
    Terminal::new(TerminalSettings { cwd: Some(sistema::home()), ..Default::default() }).ok()
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let term = abrir_terminal();
        if let Some(t) = &term {
            std::thread::sleep(Duration::from_millis(600));
            t.write(&b"ls -la\n"[..]);
        }
        let mut app = Explorer::new(term);
        // Umas amostras rápidas para os gráficos terem forma.
        for _ in 0..62 {
            std::thread::sleep(Duration::from_millis(50));
            app.monitor.amostrar();
        }
        if let Some(e) = app.entradas.iter().find(|e| e.pasta && e.nome.starts_with("Docu")).cloned() {
            app.detalhar(e);
        }
        app.grade = args.iter().any(|a| a == "--grade");
        if args.iter().any(|a| a == "--monitor") {
            app.aba = Aba::Monitor;
        }
        if args.iter().any(|a| a == "--terminal") {
            app.aba = Aba::Terminal;
        }
        let png = capture(&app.view(), Size::new(1140.0, 760.0), 1.0, &app.theme(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("explorer.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    // `--medir [escala]`: o custo de cada quadro de redimensionar (view, layout, pintura), sem janela.
    if let Some(i) = args.iter().position(|a| a == "--medir") {
        let escala: f32 = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let app = Explorer::new(None);
        let (tema, mut r, st) = (app.theme(), Renderer::new(), pelicano::States::default());
        for passo in 0..30 {
            let tam = Size::new(1140.0 + passo as f32 * 7.0, 760.0 + passo as f32 * 5.0);
            let t0 = std::time::Instant::now();
            let el = app.view();
            let t_view = t0.elapsed();
            let no = pelicano::layout(&el, tam, &mut r, &st, &tema);
            let t_layout = t0.elapsed();
            r.begin(tam, escala, tema.background);
            pelicano::draw(&el, &no, &mut r, &st, &tema, None, false);
            if passo % 10 == 9 {
                eprintln!("{tam:?}: view {t_view:?} layout {t_layout:?} total {:?}", t0.elapsed());
            }
        }
        return Ok(());
    }
    let settings = Settings {
        title: "Pelicano Explorer".into(),
        size: Size::new(1140.0, 760.0),
        min_size: Some(Size::new(900.0, 600.0)),
        decorations: false,
        app_id: Some("pelicano-explorer".into()),
        ..Default::default()
    };
    run(settings, || (Explorer::new(abrir_terminal()), Task::none()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn navegar_buscar_e_abas() {
        let raiz = std::env::temp_dir().join(format!("explorer-teste-{}", std::process::id()));
        std::fs::create_dir_all(raiz.join("zeta")).unwrap();
        std::fs::write(raiz.join("Alfa.txt"), "oi\nmundo").unwrap();
        std::fs::write(raiz.join(".oculto"), "").unwrap();
        let mut app = Explorer::new(None);
        let _ = app.update(Msg::Ir(raiz.clone()));
        let nomes: Vec<&str> = app.entradas.iter().map(|e| e.nome.as_str()).collect();
        assert_eq!(nomes, ["zeta", "Alfa.txt"], "pastas primeiro, sem ocultos");
        let _ = app.update(Msg::Busca("alf".into()));
        assert_eq!(app.visiveis().count(), 1);
        let _ = app.update(Msg::Selecionar(raiz.join("Alfa.txt")));
        assert!(matches!(app.detalhe.as_ref().map(|d| &d.previa), Some(Previa::Texto(s)) if s == "oi\nmundo"));
        let _ = app.update(Msg::Ativar(raiz.join("zeta")));
        assert_eq!(app.dir, raiz.join("zeta"));
        let _ = app.update(Msg::Voltar);
        assert_eq!(app.dir, raiz);
        let _ = app.update(Msg::Avancar);
        assert_eq!(app.dir, raiz.join("zeta"));
        let _ = app.update(Msg::Subir);
        assert_eq!((app.dir.clone(), app.voltar.len()), (raiz.clone(), 3));
        let _ = app.update(Msg::FecharAba(Aba::Terminal));
        assert_eq!(app.abas, [Aba::Arquivos, Aba::Monitor]);
        let _ = app.update(Msg::NovaAba);
        assert_eq!((app.abas.len(), app.aba), (3, Aba::Terminal));
        std::fs::remove_dir_all(&raiz).unwrap();
    }
}
