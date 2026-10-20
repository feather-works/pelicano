//! Cada widget recebendo eventos de verdade, e os pixels do recorte e do vidro.
use pelicano::column;
use pelicano::*;

const TELA: Size = Size::new(400.0, 300.0);

/// Monta a árvore de novo a cada evento, como a janela faz.
struct H {
    r: Renderer,
    st: States,
}

impl H {
    fn new() -> Self {
        H { r: Renderer::new(), st: States::default() }
    }
    fn node<M>(&mut self, el: &Element<'_, M>) -> Node {
        self.r.begin(TELA, 1.0, Color::TRANSPARENT);
        layout(el, TELA, &mut self.r, &self.st, &Theme::dark())
    }
    fn ev<M>(&mut self, el: &Element<'_, M>, p: Point, ev: Event) -> (Vec<M>, Captured) {
        let n = self.node(el);
        let mut msgs = Vec::new();
        let mut cx = EventCx { renderer: &mut self.r, states: &mut self.st, cursor: Some(p), messages: &mut msgs, window: &mut Vec::new(), redraw: false };
        let c = dispatch(el, &n, &ev, &mut cx);
        (msgs, c)
    }
    fn click<M>(&mut self, el: &Element<'_, M>, p: Point) -> Vec<M> {
        let mut m = self.ev(el, p, Event::MousePressed(MouseButton::Left)).0;
        m.extend(self.ev(el, p, Event::MouseReleased(MouseButton::Left)).0);
        m
    }
    fn tecla<M>(&mut self, el: &Element<'_, M>, k: Key, texto: Option<&str>) -> Vec<M> {
        self.ev(el, Point::ORIGIN, Event::Key { key: k, text: texto.map(Into::into), modifiers: Modifiers::empty(), pressed: true }).0
    }
    fn desenhar<M>(&mut self, el: &Element<'_, M>, cursor: Option<Point>) -> CursorIcon {
        let n = self.node(el);
        draw(el, &n, &mut self.r, &self.st, &Theme::dark(), cursor, false)
    }
}

fn centro(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

fn nomeada(n: NamedKey) -> Key {
    Key::Named(n)
}

#[derive(Debug, Clone, PartialEq)]
enum M {
    Clique(usize),
    Escolha(&'static str),
    Valor(f32),
    Marcado(bool),
    Texto(String),
    Enviar,
    Duplo,
}

// ---------- Rolagem ----------

fn lista<'a>() -> Element<'a, M> {
    let itens = (0..20).map(|i| button(text(format!("{i}"))).on_press(M::Clique(i)).height(50.0).width(Length::FILL).into());
    scrollable(column(itens)).height(200.0).width(300.0).into()
}

fn deslocamento(h: &mut H, el: &Element<M>) -> f32 {
    let n = h.node(el);
    n.rect.y - n.children[0].rect.y
}

#[test]
fn rolagem_pela_roda_respeita_os_limites() {
    let (mut h, el) = (H::new(), lista());
    let dentro = Point::new(100.0, 100.0);
    h.ev(&el, dentro, Event::Wheel { dx: 0.0, dy: -120.0 });
    assert_eq!(deslocamento(&mut h, &el), 120.0);
    h.ev(&el, dentro, Event::Wheel { dx: 0.0, dy: -100_000.0 });
    // 20 itens de 50 = 1000, menos os 200 visíveis.
    assert_eq!(deslocamento(&mut h, &el), 800.0);
    h.ev(&el, dentro, Event::Wheel { dx: 0.0, dy: 100_000.0 });
    assert_eq!(deslocamento(&mut h, &el), 0.0);
    // Roda com o mouse fora não rola.
    h.ev(&el, Point::new(350.0, 100.0), Event::Wheel { dx: 0.0, dy: -120.0 });
    assert_eq!(deslocamento(&mut h, &el), 0.0);
}

#[test]
fn rolagem_pelo_polegar_e_clique_no_item_certo() {
    let (mut h, el) = (H::new(), lista());
    // O polegar fica colado na direita, no topo: arrasta até embaixo.
    let polegar = Point::new(300.0 - 5.0, 10.0);
    let (_, c) = h.ev(&el, polegar, Event::MousePressed(MouseButton::Left));
    assert_eq!(c, Captured::Yes);
    h.ev(&el, Point::new(295.0, 400.0), Event::CursorMoved(Point::new(295.0, 400.0)));
    h.ev(&el, Point::new(295.0, 400.0), Event::MouseReleased(MouseButton::Left));
    assert_eq!(deslocamento(&mut h, &el), 800.0);
    // Rolado até o fim, o topo da área mostra o item 16.
    assert_eq!(h.click(&el, Point::new(100.0, 10.0)), vec![M::Clique(16)]);
}

#[test]
fn clique_no_conteudo_escondido_nao_vale() {
    let (mut h, el) = (H::new(), column![lista(), Space::new(10.0, 900.0)].into());
    // O item 19 está em y = 950, fora da janela de 200 da rolagem.
    assert_eq!(h.click(&el, Point::new(100.0, 960.0)), vec![]);
}

// ---------- Dropdown ----------

const OPCOES: [&str; 3] = ["a", "b", "c"];

#[test]
fn dropdown_abre_escolhe_e_fecha() {
    let mut h = H::new();
    let el: Element<M> = pick_list(&OPCOES[..], None, M::Escolha).width(120.0).into();
    let campo = h.node(&el).rect;
    let item = |i: f32| Point::new(campo.x + 20.0, campo.y + campo.height + 4.0 + 4.0 + 30.0 * i + 15.0);
    // Fechado, o lugar do menu não responde.
    assert_eq!(h.click(&el, item(1.0)), vec![]);
    assert_eq!(h.click(&el, centro(campo)), vec![]);
    assert_eq!(h.click(&el, item(1.0)), vec![M::Escolha("b")]);
    // Escolher fecha: o mesmo clique de novo não faz nada.
    assert_eq!(h.click(&el, item(2.0)), vec![]);
    // Esc fecha sem escolher.
    h.click(&el, centro(campo));
    h.tecla(&el, nomeada(NamedKey::Escape), None);
    assert_eq!(h.click(&el, item(0.0)), vec![]);
    // Clique fora fecha sem escolher.
    h.click(&el, centro(campo));
    assert_eq!(h.click(&el, Point::new(390.0, 290.0)), vec![]);
    assert_eq!(h.click(&el, item(0.0)), vec![]);
}

// ---------- Slider ----------

#[test]
fn slider_clique_arrasto_e_passo() {
    let mut h = H::new();
    // 216 de largura: 8 de folga de cada lado, trilho de 200.
    let el: Element<M> = slider(0.0..=100.0, 0.0, M::Valor).step(10.0).width(216.0).into();
    let y = h.node(&el).rect.y + 10.0;
    assert_eq!(h.ev(&el, Point::new(108.0, y), Event::MousePressed(MouseButton::Left)).0, vec![M::Valor(50.0)]);
    // 37/200 = 18,5 -> passo de 10 arredonda para 20.
    assert_eq!(h.ev(&el, Point::new(45.0, y), Event::CursorMoved(Point::new(45.0, y))).0, vec![M::Valor(20.0)]);
    // Passou do fim: fica no máximo.
    assert_eq!(h.ev(&el, Point::new(900.0, y), Event::CursorMoved(Point::new(900.0, y))).0, vec![M::Valor(100.0)]);
    h.ev(&el, Point::new(900.0, y), Event::MouseReleased(MouseButton::Left));
    // Solto, mover o mouse não muda nada.
    assert_eq!(h.ev(&el, Point::new(45.0, y), Event::CursorMoved(Point::new(45.0, y))).0, vec![]);
}

// ---------- Checkbox e toggler ----------

#[test]
fn checkbox_e_toggler_invertem_e_desabilitado_ignora() {
    let mut h = H::new();
    let el: Element<M> = column![checkbox("aviso", false).on_toggle(M::Marcado), toggler("escuro", true).on_toggle(M::Marcado), checkbox::<M>("travado", false)].spacing(10).into();
    let n = h.node(&el);
    let [a, b, c] = [0, 1, 2].map(|i| centro(n.children[i].rect));
    assert_eq!(h.click(&el, a), vec![M::Marcado(true)]);
    assert_eq!(h.click(&el, b), vec![M::Marcado(false)]);
    assert_eq!(h.click(&el, c), vec![]);
}

// ---------- Campo de texto ----------

/// O ciclo do app: cada `Texto` vira o novo valor do campo, que é montado de novo.
struct Campo {
    h: H,
    valor: String,
}

impl Campo {
    fn el(valor: &str) -> Element<'_, M> {
        column![text_input("", valor).on_input(M::Texto).on_submit(M::Enviar).width(200.0), Space::new(10.0, 100.0)].into()
    }
    fn aplicar(&mut self, msgs: Vec<M>) -> Vec<M> {
        for m in &msgs {
            if let M::Texto(t) = m {
                self.valor = t.clone();
            }
        }
        msgs
    }
    fn tecla(&mut self, k: Key, t: Option<&str>) -> Vec<M> {
        let el = Campo::el(&self.valor);
        let m = self.h.tecla(&el, k, t);
        drop(el);
        self.aplicar(m)
    }
    fn digitar(&mut self, t: &str) -> Vec<M> {
        self.tecla(Key::Character(t.into()), Some(t))
    }
    fn click(&mut self, p: Point) {
        let el = Campo::el(&self.valor);
        let m = self.h.click(&el, p);
        drop(el);
        self.aplicar(m);
    }
}

#[test]
fn campo_edita_navega_envia_e_perde_o_foco() {
    let mut c = Campo { h: H::new(), valor: "abc".into() };
    let campo = c.h.node(&Campo::el("abc")).children[0].rect;
    // Sem foco, digitar não faz nada.
    assert_eq!(c.digitar("x"), vec![]);
    // Clique depois do texto põe o cursor no fim.
    c.click(Point::new(campo.x + 150.0, campo.y + 5.0));
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![M::Texto("ab".into())]);
    c.tecla(nomeada(NamedKey::ArrowLeft), None);
    c.tecla(nomeada(NamedKey::ArrowLeft), None);
    assert_eq!(c.digitar("X"), vec![M::Texto("Xab".into())]);
    // O cursor ficou depois do X; Delete apaga a letra seguinte (o "a").
    assert_eq!(c.tecla(nomeada(NamedKey::Delete), None), vec![M::Texto("Xb".into())]);
    c.tecla(nomeada(NamedKey::End), None);
    assert_eq!(c.digitar("ç"), vec![M::Texto("Xbç".into())]);
    // Backspace num caractere de 2 bytes apaga ele inteiro.
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![M::Texto("Xb".into())]);
    c.tecla(nomeada(NamedKey::Home), None);
    assert_eq!(c.tecla(nomeada(NamedKey::Backspace), None), vec![], "Backspace no início não apaga");
    assert_eq!(c.tecla(nomeada(NamedKey::Enter), None), vec![M::Enviar]);
    // Ctrl+letra não é texto.
    let el = Campo::el(&c.valor);
    let ctrl = c.h.ev(&el, Point::ORIGIN, Event::Key { key: Key::Character("a".into()), text: Some("a".into()), modifiers: Modifiers::CONTROL, pressed: true }).0;
    drop(el);
    assert_eq!(ctrl, vec![]);
    // Clique fora tira o foco.
    c.click(Point::new(5.0, campo.y + campo.height + 50.0));
    assert_eq!(c.digitar("x"), vec![]);
    assert_eq!(c.valor, "Xb");
}

#[test]
fn campo_senha_mapeia_o_clique_para_o_valor() {
    let mut h = H::new();
    let el: Element<M> = text_input("", "çãb").secure(true).on_input(M::Texto).width(200.0).into();
    let campo = h.node(&el).rect;
    // Clique no começo: o cursor fica antes do "ç" (2 bytes), e não no meio dele.
    h.click(&el, Point::new(campo.x + 10.0, campo.y + 5.0));
    h.tecla(&el, nomeada(NamedKey::Home), None);
    h.tecla(&el, nomeada(NamedKey::ArrowRight), None);
    assert_eq!(h.tecla(&el, Key::Character("-".into()), Some("-")), vec![M::Texto("ç-ãb".into())]);
}

// ---------- Mouse, camadas e map ----------

#[test]
fn duplo_clique_e_clique_simples() {
    let mut h = H::new();
    let el: Element<M> = mouse_area(Space::new(100.0, 100.0)).on_press(M::Clique(0)).on_double_click(M::Duplo).into();
    let p = Point::new(50.0, 50.0);
    assert_eq!(h.click(&el, p), vec![M::Clique(0)]);
    assert_eq!(h.click(&el, p), vec![M::Duplo]);
    // Depois do duplo, o próximo volta a ser simples.
    assert_eq!(h.click(&el, p), vec![M::Clique(0)]);
}

#[test]
fn camada_opaca_bloqueia_clique_e_hover_de_baixo() {
    let mut h = H::new();
    let base = || button(text("fundo")).on_press(M::Clique(1)).width(Length::FILL).height(Length::FILL);
    let p = Point::new(50.0, 50.0);
    let livre: Element<M> = stack![base()].into();
    assert_eq!(h.click(&livre, p), vec![M::Clique(1)]);
    assert_eq!(h.desenhar(&livre, Some(p)), CursorIcon::Pointer);
    let coberto: Element<M> = stack![base(), opaque(Space::new(100.0, 100.0))].into();
    assert_eq!(h.click(&coberto, p), vec![]);
    assert_eq!(h.desenhar(&coberto, Some(p)), CursorIcon::Default, "o botão coberto não acende");
    // Fora da camada opaca, o fundo volta a responder.
    assert_eq!(h.click(&coberto, Point::new(300.0, 200.0)), vec![M::Clique(1)]);
}

#[test]
fn map_converte_as_mensagens() {
    let mut h = H::new();
    let el: Element<String> = Element::from(button("ok").on_press(7usize)).map(|n| format!("n={n}"));
    assert_eq!(h.click(&el, Point::new(5.0, 5.0)), vec!["n=7".to_string()]);
}

#[test]
fn botao_desabilitado_e_soltar_fora_nao_disparam() {
    let mut h = H::new();
    let el: Element<M> = column![button("a").on_press_maybe(None), button("b").on_press(M::Enviar)].spacing(40).into();
    let n = h.node(&el);
    assert_eq!(h.click(&el, centro(n.children[0].rect)), vec![]);
    // Apertar no botão e soltar fora cancela, como no navegador.
    let b = centro(n.children[1].rect);
    h.ev(&el, b, Event::MousePressed(MouseButton::Left));
    assert_eq!(h.ev(&el, Point::new(390.0, 290.0), Event::MouseReleased(MouseButton::Left)).0, vec![]);
}

// ---------- Layout ----------

#[test]
fn wrap_quebra_a_linha_quando_nao_cabe() {
    let mut h = H::new();
    let el: Element<()> = container(row((0..5).map(|_| Space::new(100.0, 40.0).into())).spacing(10).wrap()).width(330.0).into();
    let linha = &h.node(&el).children[0];
    let pos: Vec<(f32, f32)> = linha.children.iter().map(|c| (c.rect.x, c.rect.y)).collect();
    assert_eq!(pos, vec![(0.0, 0.0), (110.0, 0.0), (220.0, 0.0), (0.0, 50.0), (110.0, 50.0)]);
    assert_eq!(linha.rect.height, 90.0);
}

#[test]
fn espaco_que_enche_respeita_a_altura_fixa_da_coluna() {
    let mut h = H::new();
    let el: Element<()> = column![Space::new(10.0, 50.0), vertical_space(), Space::new(10.0, 30.0)].height(200.0).into();
    let n = h.node(&el);
    assert_eq!(n.rect.height, 200.0);
    // O último encosta no fundo da coluna, não da tela (300).
    assert_eq!(n.children[2].rect.y + n.children[2].rect.height, 200.0);
}

#[test]
fn alinhamento_no_centro_e_no_fim() {
    let mut h = H::new();
    let el: Element<()> = container(Space::new(100.0, 50.0)).width(400.0).height(300.0).align_x(Align::End).align_y(Align::Center).into();
    let filho = h.node(&el).children[0].rect;
    assert_eq!((filho.x, filho.y), (300.0, 125.0));
}

// ---------- Pixels ----------

fn pixel(h: &H, x: u32, y: u32) -> [u8; 4] {
    let p = h.r.pixmap.pixel(x, y).expect("dentro do quadro");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn imagem_segue_o_raio_do_conteiner() {
    let mut h = H::new();
    let vermelho = Image::from_rgba(4, 4, [255, 0, 0, 255].repeat(16)).expect("4x4");
    let el: Element<()> = container(image(vermelho).width(Length::FILL).height(Length::FILL).content_fit(ContentFit::Fill)).width(200.0).height(100.0).clip(true).style(|_| Style::default().border(Border::rounded(30.0))).into();
    h.desenhar(&el, None);
    assert_eq!(pixel(&h, 2, 2)[3], 0, "o canto fica transparente");
    assert_eq!(pixel(&h, 100, 50), [255, 0, 0, 255], "o meio é a imagem");
    assert_eq!(pixel(&h, 199 - 2, 99 - 2)[3], 0, "o canto oposto também");
}

#[test]
fn vidro_desfoca_o_que_esta_atras() {
    let mut h = H::new();
    // Metade preta, metade branca; o vidro em cima da divisa mistura as duas.
    let fundo = canvas(Length::FILL, Length::FILL, |r, a, _| {
        r.fill_quad(Rect { width: a.width / 2.0, ..a }, Some(&Color::BLACK.into()), Border::default(), None);
        r.fill_quad(Rect { x: a.width / 2.0, width: a.width / 2.0, ..a }, Some(&Color::WHITE.into()), Border::default(), None);
    });
    let vidro = container(Space::new(Length::FILL, Length::FILL)).margin(100.0).style(|_| Style::default().backdrop_blur(20.0));
    let el: Element<()> = stack![fundo, vidro].into();
    h.desenhar(&el, None);
    let meio = pixel(&h, 200, 150)[0];
    assert!((60..200).contains(&meio), "na divisa o vidro dá cinza, veio {meio}");
    // Fora do vidro continua nítido.
    assert_eq!(pixel(&h, 198, 5)[0], 0);
    assert_eq!(pixel(&h, 202, 5)[0], 255);
}

#[test]
fn quadro_parcial_igual_ao_cheio() {
    // Degradê, sombra, vidro desfocado e texto: redesenhar só um pedaço tem que dar os mesmos pixels.
    let cena = || -> Element<'static, ()> {
        let fundo = container(Space::new(Length::FILL, Length::FILL))
            .style(|_| Style::default().background(Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(200, 30, 90)), (1.0, Color::rgb8(20, 40, 200))] }));
        let vidro = container(text("vidro"))
            .margin(60.0)
            .padding(20.0)
            .style(|_| Style::default().backdrop_blur(20.0).border(Border::rounded(14.0)).shadow(Shadow { color: Color::BLACK.alpha(0.5), offset: Point::new(0.0, 6.0), blur: 20.0 }));
        stack![fundo, vidro].into()
    };
    let (size, tema, st) = (Size::new(400.0, 300.0), Theme::dark(), States::default());
    let mut r = Renderer::new();
    let el = cena();
    r.begin(size, 1.0, Color::BLACK);
    let n = layout(&el, size, &mut r, &st, &tema);
    draw(&el, &n, &mut r, &st, &tema, None, false);
    let cheio = r.pixmap.data().to_vec();
    assert!(r.begin_parcial(size, 1.0, Color::BLACK, Rect { x: 37.0, y: 21.0, width: 150.0, height: 110.0 }));
    let n = layout(&el, size, &mut r, &st, &tema);
    draw(&el, &n, &mut r, &st, &tema, None, false);
    r.end();
    let pior = cheio.iter().zip(r.pixmap.data()).map(|(a, b)| a.abs_diff(*b)).max();
    assert_eq!(pior, Some(0), "o pedaço redesenhado tem que bater com o quadro cheio");
}

#[test]
fn hover_redesenha_so_o_que_mudou_e_bate_com_o_cheio() {
    // Botão sobre um vidro: gravar, comparar e pintar só as regiões tem que dar o mesmo que o quadro cheio.
    let el: Element<()> = stack![
        container(Space::new(Length::FILL, Length::FILL)).style(|_| Style::default().background(Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(200, 30, 90)), (1.0, Color::rgb8(20, 40, 200))] })),
        container(button("ok").on_press(())).margin(60.0).padding(20.0).style(|_| Style::default().backdrop_blur(20.0).border(Border::rounded(14.0)))
    ]
    .into();
    let (size, tema, st) = (Size::new(400.0, 300.0), Theme::dark(), States::default());
    let n = { let mut r = Renderer::new(); layout(&el, size, &mut r, &st, &tema) };
    let quadro = |r: &mut Renderer, cursor: Option<Point>| {
        r.record();
        draw(&el, &n, r, &st, &tema, cursor, false);
        let c = r.changes();
        match &c {
            Changes::Regions(rs) => {
                for x in rs {
                    assert!(r.begin_parcial(size, 1.0, Color::BLACK, *x));
                    draw(&el, &n, r, &st, &tema, cursor, false);
                    r.end();
                }
            }
            Changes::All => {
                r.begin(size, 1.0, Color::BLACK);
                draw(&el, &n, r, &st, &tema, cursor, false);
            }
            Changes::None => {}
        }
        c
    };
    let mut r = Renderer::new();
    assert_eq!(quadro(&mut r, None), Changes::All);
    assert_eq!(quadro(&mut r, Some(Point::new(5.0, 5.0))), Changes::None, "mouse no vazio não muda nada");
    let botao = Point::new(85.0, 85.0);
    assert!(matches!(quadro(&mut r, Some(botao)), Changes::Regions(_)));
    let mut cheio = Renderer::new();
    cheio.begin(size, 1.0, Color::BLACK);
    draw(&el, &n, &mut cheio, &st, &tema, Some(botao), false);
    assert_ne!(r.pixmap.data(), { let mut v = Renderer::new(); v.begin(size, 1.0, Color::BLACK); draw(&el, &n, &mut v, &st, &tema, None, false); v.pixmap.data().to_vec() }.as_slice(), "o hover tem que mudar algo");
    let pior = cheio.pixmap.data().iter().zip(r.pixmap.data()).map(|(a, b)| a.abs_diff(*b)).max();
    assert_eq!(pior, Some(0), "as regiões redesenhadas têm que bater com o quadro cheio");
}

#[test]
fn rolagem_de_dentro_tem_a_vez() {
    // Uma lista que rola dentro de uma página que também rola: a roda sobre a lista move a lista.
    let el: Element<M> = scrollable(column![Space::new(10.0, 20.0), lista(), Space::new(10.0, 800.0)]).into();
    let mut h = H::new();
    let rolagens = |h: &mut H| {
        let pagina = h.node(&el);
        let lista = &pagina.children[0].children[1];
        (pagina.rect.y - pagina.children[0].rect.y, lista.rect.y - lista.children[0].rect.y)
    };
    h.ev(&el, Point::new(100.0, 100.0), Event::Wheel { dx: 0.0, dy: -120.0 });
    assert_eq!(rolagens(&mut h), (0.0, 120.0), "só a lista rolou");
    // No fim da lista a página não rola por tabela (o evento já foi da lista).
    h.ev(&el, Point::new(100.0, 100.0), Event::Wheel { dx: 0.0, dy: -100_000.0 });
    assert_eq!(rolagens(&mut h).1, 800.0);
    // Fora da lista, rola a página.
    h.ev(&el, Point::new(350.0, 100.0), Event::Wheel { dx: 0.0, dy: -50.0 });
    assert_eq!(rolagens(&mut h).0, 50.0);
}

#[test]
fn imagem_nova_nao_herda_o_cache_da_velha() {
    // A imagem liberada deixa o endereço para a próxima; o cache não pode confundir as duas.
    let mut h = H::new();
    for cor in [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]] {
        let img = Image::from_rgba(4, 4, cor.repeat(16)).expect("4x4");
        let el: Element<()> = image(img).width(40.0).height(40.0).content_fit(ContentFit::Fill).into();
        h.desenhar(&el, None);
        assert_eq!(pixel(&h, 20, 20), cor);
    }
}

#[test]
fn sombra_grande_sem_emenda() {
    // A sombra de caixa grande é a de uma pequena esticada pelo meio: não pode ter degrau.
    let mut r = Renderer::new();
    let tam = Size::new(400.0, 300.0);
    r.begin(tam, 1.0, Color::TRANSPARENT);
    let sombra = Shadow { color: Color::BLACK, offset: Point::ORIGIN, blur: 12.0 };
    r.fill_quad(Rect { x: 50.0, y: 50.0, width: 300.0, height: 200.0 }, None, Border::rounded(10.0), Some(sombra));
    let alfa = |r: &Renderer, x: u32, y: u32| r.pixmap.pixel(x, y).expect("dentro").alpha() as i32;
    // Uma linha acima da caixa: o valor varia aos poucos no canto e fica igual no meio.
    for y in [40, 46] {
        let linha: Vec<i32> = (40..360).map(|x| alfa(&r, x, y)).collect();
        let salto = linha.windows(2).map(|w| (w[1] - w[0]).abs()).max().unwrap_or(0);
        assert!(salto <= 12, "degrau de {salto} na linha {y}");
        assert_eq!(alfa(&r, 150, y), alfa(&r, 250, y), "o meio da borda é constante");
    }
    // O canto da grande bate com o de uma caixa pequena (sem esticar).
    let canto = alfa(&r, 48, 48);
    r.begin(tam, 1.0, Color::TRANSPARENT);
    r.fill_quad(Rect { x: 50.0, y: 50.0, width: 45.0, height: 45.0 }, None, Border::rounded(10.0), Some(sombra));
    assert!((alfa(&r, 48, 48) - canto).abs() <= 8, "canto {} x {}", alfa(&r, 48, 48), canto);
}
