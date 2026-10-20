//! Xadrez personalizável: temas de tabuleiro, virar, dicas de lance.
//! `cargo run --example xadrez`; `-- --capturar x.png` só gera o PNG (com e2 selecionada).
//! shortcut: sem roque, en passant e xeque; os lances são os de cada peça. Entram se virar jogo de verdade.
use pelicano::column;
use pelicano::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cor {
    Branca,
    Preta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tipo {
    Rei,
    Dama,
    Torre,
    Bispo,
    Cavalo,
    Peao,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Peca(Cor, Tipo);

impl Peca {
    fn simbolo(self) -> &'static str {
        // Sempre o desenho cheio: a cor vem do texto, então o tema pinta as duas.
        match self.1 {
            Tipo::Rei => "♚",
            Tipo::Dama => "♛",
            Tipo::Torre => "♜",
            Tipo::Bispo => "♝",
            Tipo::Cavalo => "♞",
            Tipo::Peao => "♟",
        }
    }
}

type Casa = (i8, i8); // (coluna 0..8 = a..h, linha 0..8 = 1..8)
type Tabuleiro = [[Option<Peca>; 8]; 8];

fn inicial() -> Tabuleiro {
    let mut t = [[None; 8]; 8];
    let fundo = [Tipo::Torre, Tipo::Cavalo, Tipo::Bispo, Tipo::Dama, Tipo::Rei, Tipo::Bispo, Tipo::Cavalo, Tipo::Torre];
    for c in 0..8 {
        t[0][c] = Some(Peca(Cor::Branca, fundo[c]));
        t[1][c] = Some(Peca(Cor::Branca, Tipo::Peao));
        t[6][c] = Some(Peca(Cor::Preta, Tipo::Peao));
        t[7][c] = Some(Peca(Cor::Preta, fundo[c]));
    }
    t
}

fn em(t: &Tabuleiro, (c, l): Casa) -> Option<Peca> {
    t[l as usize][c as usize]
}

fn dentro((c, l): Casa) -> bool {
    (0..8).contains(&c) && (0..8).contains(&l)
}

/// Para onde a peça em `de` pode ir (sem checar xeque).
fn lances(t: &Tabuleiro, de: Casa) -> Vec<Casa> {
    let Some(Peca(cor, tipo)) = em(t, de) else { return Vec::new() };
    let livre = |c: Casa| dentro(c) && em(t, c).is_none();
    let inimiga = |c: Casa| dentro(c) && em(t, c).is_some_and(|p| p.0 != cor);
    let mut v = Vec::new();
    let deslizar = |dirs: &[(i8, i8)], v: &mut Vec<Casa>| {
        for &(dc, dl) in dirs {
            let mut c = (de.0 + dc, de.1 + dl);
            while livre(c) {
                v.push(c);
                c = (c.0 + dc, c.1 + dl);
            }
            if inimiga(c) {
                v.push(c);
            }
        }
    };
    const RETAS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    const DIAGONAIS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    match tipo {
        Tipo::Torre => deslizar(&RETAS, &mut v),
        Tipo::Bispo => deslizar(&DIAGONAIS, &mut v),
        Tipo::Dama => {
            deslizar(&RETAS, &mut v);
            deslizar(&DIAGONAIS, &mut v);
        }
        Tipo::Rei | Tipo::Cavalo => {
            let saltos: &[(i8, i8)] = if tipo == Tipo::Rei { &[(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] } else { &[(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)] };
            v.extend(saltos.iter().map(|&(dc, dl)| (de.0 + dc, de.1 + dl)).filter(|&c| livre(c) || inimiga(c)));
        }
        Tipo::Peao => {
            let (frente, base) = if cor == Cor::Branca { (1, 1) } else { (-1, 6) };
            let um = (de.0, de.1 + frente);
            if livre(um) {
                v.push(um);
                let dois = (de.0, de.1 + 2 * frente);
                if de.1 == base && livre(dois) {
                    v.push(dois);
                }
            }
            v.extend([-1, 1].map(|dc| (de.0 + dc, de.1 + frente)).into_iter().filter(|&c| inimiga(c)));
        }
    }
    v
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Tema {
    nome: &'static str,
    clara: Color,
    escura: Color,
    pecas: (Color, Color),
    vidro: bool,
}

impl std::fmt::Display for Tema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.nome)
    }
}

const TEMAS: [Tema; 3] = [
    Tema { nome: "Clássico", clara: Color::rgb8(0xf0, 0xd9, 0xb5), escura: Color::rgb8(0xb5, 0x88, 0x63), pecas: (Color::rgb8(0xff, 0xff, 0xff), Color::rgb8(0x1f, 0x1a, 0x17)), vidro: false },
    Tema { nome: "Verde", clara: Color::rgb8(0xee, 0xee, 0xd2), escura: Color::rgb8(0x76, 0x96, 0x56), pecas: (Color::rgb8(0xfa, 0xfa, 0xfa), Color::rgb8(0x22, 0x22, 0x22)), vidro: false },
    Tema { nome: "Vidro", clara: Color::rgba(1.0, 1.0, 1.0, 0.22), escura: Color::rgba(1.0, 1.0, 1.0, 0.06), pecas: (Color::rgb8(0xf8, 0xfa, 0xfc), Color::rgb8(0x0f, 0x17, 0x2a)), vidro: true },
];

#[derive(Debug, Clone)]
enum Msg {
    Casa(Casa),
    Tema(Tema),
    Virar(bool),
    Dicas(bool),
    Reiniciar,
}

struct Xadrez {
    tab: Tabuleiro,
    vez: Cor,
    selecionada: Option<Casa>,
    ultimo: Option<(Casa, Casa)>,
    capturadas: Vec<Peca>,
    tema: Tema,
    virado: bool,
    dicas: bool,
}

impl Xadrez {
    fn novo() -> Self {
        Xadrez { tab: inicial(), vez: Cor::Branca, selecionada: None, ultimo: None, capturadas: Vec::new(), tema: TEMAS[0], virado: false, dicas: true }
    }

    fn clicar(&mut self, c: Casa) {
        let da_vez = em(&self.tab, c).is_some_and(|p| p.0 == self.vez);
        match self.selecionada {
            Some(de) if lances(&self.tab, de).contains(&c) => {
                let mut p = em(&self.tab, de).expect("selecionada tem peça");
                // Peão que chega ao fim vira dama.
                if p.1 == Tipo::Peao && (c.1 == 0 || c.1 == 7) {
                    p.1 = Tipo::Dama;
                }
                if let Some(x) = em(&self.tab, c) {
                    self.capturadas.push(x);
                }
                self.tab[c.1 as usize][c.0 as usize] = Some(p);
                self.tab[de.1 as usize][de.0 as usize] = None;
                self.ultimo = Some((de, c));
                self.selecionada = None;
                self.vez = if self.vez == Cor::Branca { Cor::Preta } else { Cor::Branca };
            }
            _ if da_vez => self.selecionada = Some(c),
            _ => self.selecionada = None,
        }
    }

    fn casa(&self, c: Casa) -> Element<'_, Msg> {
        let tema = self.tema;
        let clara = (c.0 + c.1) % 2 == 1;
        let selecionada = self.selecionada == Some(c);
        let ultimo = self.ultimo.is_some_and(|(a, b)| a == c || b == c);
        let alvo = self.dicas && self.selecionada.is_some_and(|s| lances(&self.tab, s).contains(&c));
        let peca = em(&self.tab, c);
        let simbolo = peca.map_or("", Peca::simbolo);
        let cor_peca = peca.map_or(Color::TRANSPARENT, |p| if p.0 == Cor::Branca { tema.pecas.0 } else { tema.pecas.1 });
        let conteudo = stack![
            container(text(simbolo).size(46).color(cor_peca)).center(),
            // Ponto no lance vazio, anel na captura, como nos sites de xadrez.
            canvas(Length::FILL, Length::FILL, move |r, a, _| {
                if !alvo {
                    return;
                }
                let tinta = Color::BLACK.alpha(0.25);
                if peca.is_some() {
                    r.fill_quad(a, None, Border::rounded(a.width / 2.0).color(tinta, 5.0), None);
                } else {
                    let d = a.width * 0.3;
                    r.fill_quad(Rect { x: a.x + (a.width - d) / 2.0, y: a.y + (a.height - d) / 2.0, width: d, height: d }, Some(&tinta.into()), Border::rounded(d / 2.0), None);
                }
            }),
        ];
        button(conteudo)
            .on_press(Msg::Casa(c))
            .padding(0.0)
            .width(64.0)
            .height(64.0)
            .style(move |_, s| {
                let base = if clara { tema.clara } else { tema.escura };
                let destaque = Color::rgb8(0xf6, 0xe0, 0x5e);
                let fundo = if selecionada {
                    base.mix(destaque, 0.65)
                } else if ultimo {
                    base.mix(destaque, 0.35)
                } else if s == Status::Hovered && peca.is_some() {
                    base.mix(Color::WHITE, 0.15)
                } else {
                    base
                };
                Style::default().background(fundo)
            })
            .into()
    }

    fn tabuleiro(&self) -> Element<'_, Msg> {
        let linhas: Vec<i8> = if self.virado { (0..8).collect() } else { (0..8).rev().collect() };
        let colunas: Vec<i8> = if self.virado { (0..8).rev().collect() } else { (0..8).collect() };
        let rotulo = |s: String| container(text(s).size(12)).width(20.0).height(64.0).align_y(Align::Center);
        let grade = column(linhas.iter().map(|&l| row(std::iter::once(rotulo((l + 1).to_string()).into()).chain(colunas.iter().map(|&c| self.casa((c, l))))).into()));
        let letras = row(std::iter::once(Space::new(20.0, 16.0).into()).chain(colunas.iter().map(|&c| container(text(((b'a' + c as u8) as char).to_string()).size(12)).width(64.0).align_x(Align::Center).into())));
        let tema = self.tema;
        container(column![container(grade).clip(true).style(|_| Style::default().border(Border::rounded(10.0))), letras].spacing(4))
            .padding(16.0)
            .style(move |t| cartao(tema, t))
            .into()
    }
}

/// Cartão sólido; vidro só no tema Vidro.
fn cartao(tema: Tema, t: &Theme) -> Style {
    if tema.vidro {
        Style::glass(16.0)
    } else {
        Style::default().background(t.surface).border(Border::rounded(16.0).color(t.border, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.3), offset: Point::new(0.0, 6.0), blur: 20.0 })
    }
}

impl App for Xadrez {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Casa(c) => self.clicar(c),
            Msg::Tema(t) => self.tema = t,
            Msg::Virar(v) => self.virado = v,
            Msg::Dicas(d) => self.dicas = d,
            Msg::Reiniciar => *self = Xadrez { tema: self.tema, virado: self.virado, dicas: self.dicas, ..Xadrez::novo() },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let vez = if self.vez == Cor::Branca { "Vez das brancas" } else { "Vez das pretas" };
        let capturadas = |cor: Cor| -> String { self.capturadas.iter().filter(|p| p.0 == cor).map(|p| p.simbolo()).collect() };
        let painel = column![
            text("Xadrez").size(26).font(Font::BOLD),
            text(vez).size(15),
            horizontal_rule(1.0),
            row![text("Tabuleiro").size(13), horizontal_space(), pick_list(&TEMAS[..], Some(self.tema), Msg::Tema).width(140.0)].align(Align::Center),
            toggler("Virar tabuleiro", self.virado).on_toggle(Msg::Virar),
            toggler("Mostrar lances", self.dicas).on_toggle(Msg::Dicas),
            horizontal_rule(1.0),
            text("Capturadas").size(13).color(Theme::dark().muted),
            text(format!("{} {}", capturadas(Cor::Preta), capturadas(Cor::Branca))).size(22),
            vertical_space(),
            button(text("Nova partida").font(Font::SEMIBOLD)).on_press(Msg::Reiniciar).width(Length::FILL),
        ]
        .spacing(14)
        .width(240.0)
        .height(560.0);
        let tema = self.tema;
        container(row![self.tabuleiro(), container(painel).padding(20.0).style(move |t| cartao(tema, t))].spacing(24).align(Align::Center))
            .center()
            .style(move |t| {
                if tema.vidro {
                    // O vidro precisa de algo colorido atrás para aparecer.
                    Style::default().background(Background::Linear { angle: 0.7, stops: vec![(0.0, Color::rgb8(0x4c, 0x1d, 0x95)), (1.0, Color::rgb8(0x0e, 0x74, 0x90))] })
                } else {
                    Style::default().background(t.background)
                }
            })
            .into()
    }
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--capturar") {
        let mut x = Xadrez::novo();
        for c in [(4, 1), (4, 3), (4, 6), (4, 4), (6, 0)] {
            x.clicar(c);
        }
        if args.iter().any(|a| a == "--vidro") {
            x.tema = TEMAS[2];
        }
        let png = capture(&x.view(), Size::new(940.0, 680.0), 1.0, &Theme::dark(), &mut Renderer::new(), None);
        let destino = args.get(i + 1).map_or("xadrez.png", String::as_str);
        std::fs::write(destino, png).map_err(|e| Error::Window(e.to_string()))?;
        println!("{destino}");
        return Ok(());
    }
    run(Settings { title: "Xadrez — Pelicano".into(), size: Size::new(940.0, 680.0), min_size: Some(Size::new(900.0, 660.0)), debug: true, ..Default::default() }, || (Xadrez::novo(), Task::none()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn abertura_tem_vinte_lances_e_peao_avanca() {
        let t = inicial();
        let total: usize = (0..8).flat_map(|c| (0..2).map(move |l| (c, l))).map(|c| lances(&t, c).len()).sum();
        assert_eq!(total, 20);
        let mut x = Xadrez::novo();
        x.clicar((4, 1));
        x.clicar((4, 3));
        assert_eq!(em(&x.tab, (4, 3)), Some(Peca(Cor::Branca, Tipo::Peao)));
        assert_eq!(x.vez, Cor::Preta);
        // Peça branca não pode ser escolhida na vez das pretas.
        x.clicar((3, 1));
        assert_eq!(x.selecionada, None);
    }

    #[test]
    fn captura_e_promocao() {
        let mut t = [[None; 8]; 8];
        t[6][0] = Some(Peca(Cor::Branca, Tipo::Peao));
        t[7][1] = Some(Peca(Cor::Preta, Tipo::Torre));
        let mut x = Xadrez { tab: t, ..Xadrez::novo() };
        assert_eq!(lances(&x.tab, (0, 6)), vec![(0, 7), (1, 7)]);
        x.clicar((0, 6));
        x.clicar((1, 7));
        assert_eq!(em(&x.tab, (1, 7)), Some(Peca(Cor::Branca, Tipo::Dama)));
        assert_eq!(x.capturadas, vec![Peca(Cor::Preta, Tipo::Torre)]);
    }

    #[test]
    fn torre_para_na_propria_peca_e_captura_a_inimiga() {
        let mut t = [[None; 8]; 8];
        t[0][0] = Some(Peca(Cor::Branca, Tipo::Torre));
        t[0][3] = Some(Peca(Cor::Branca, Tipo::Rei));
        t[4][0] = Some(Peca(Cor::Preta, Tipo::Peao));
        let mut v = lances(&t, (0, 0));
        v.sort();
        assert_eq!(v, vec![(0, 1), (0, 2), (0, 3), (0, 4), (1, 0), (2, 0)]);
    }
}
