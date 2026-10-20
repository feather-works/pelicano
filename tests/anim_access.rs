//! Transições andando com o relógio do quadro, e a árvore que o leitor de tela recebe.
use pelicano::accesskit::{Role, Toggled};
use pelicano::column;
use pelicano::*;
use std::time::{Duration, Instant};

const TELA: Size = Size::new(200.0, 100.0);

fn quadro<M>(el: &Element<'_, M>, r: &mut Renderer, st: &States, cursor: Option<Point>) -> [u8; 4] {
    r.begin(TELA, 1.0, Color::TRANSPARENT);
    let n = layout(el, TELA, r, st, &Theme::dark());
    draw(el, &n, r, st, &Theme::dark(), cursor, false);
    let p = r.pixmap.pixel(5, 5).expect("dentro");
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn hover_com_transicao_passa_pelo_meio() {
    let el: Element<()> = button(Space::new(100.0, 50.0))
        .on_press(())
        .padding(0.0)
        .style(|_, s| Style::default().background(if s == Status::Hovered { Color::rgb(0.0, 0.0, 1.0) } else { Color::rgb(1.0, 0.0, 0.0) }).transition(Transition::new(100).easing(Easing::Linear)))
        .into();
    let (mut r, mut st, t0) = (Renderer::new(), States::default(), Instant::now());
    let em = |ms: u64| t0 + Duration::from_millis(ms);
    let dentro = Some(Point::new(10.0, 10.0));
    st.tick(em(0));
    assert_eq!(quadro(&el, &mut r, &st, None), [255, 0, 0, 255]);
    st.tick(em(10));
    assert_eq!(quadro(&el, &mut r, &st, dentro), [255, 0, 0, 255], "começa de onde estava");
    assert!(st.animating());
    st.tick(em(60));
    let [vm, _, az, _] = quadro(&el, &mut r, &st, dentro);
    assert!(vm.abs_diff(128) <= 3 && az.abs_diff(128) <= 3, "no meio: {vm} {az}");
    st.tick(em(200));
    assert_eq!(quadro(&el, &mut r, &st, dentro), [0, 0, 255, 255]);
    assert!(!st.animating(), "chegou: a janela para de pedir quadros");
}

#[test]
fn animate_entra_aparecendo() {
    let caixa = || container(Space::new(100.0, 50.0)).style(|_| Style::default().background(Color::WHITE));
    let el: Element<()> = animate(caixa()).enter(Keyframe::default().opacity(0.0)).transition(Transition::new(100).easing(Easing::Linear)).into();
    let (mut r, mut st, t0) = (Renderer::new(), States::default(), Instant::now());
    // Captura sem relógio: já no fim.
    assert_eq!(quadro(&el, &mut r, &st, None)[3], 255);
    st.tick(t0);
    assert_eq!(quadro(&el, &mut r, &st, None)[3], 0, "entra invisível");
    st.tick(t0 + Duration::from_millis(50));
    let a = quadro(&el, &mut r, &st, None)[3];
    assert!(a.abs_diff(128) <= 3, "meio caminho: {a}");
}

#[test]
fn leitor_de_tela_recebe_papeis_e_nomes() {
    let el: Element<()> = column![
        text("Título"),
        button("Salvar").on_press(()),
        checkbox("Lembrar", true).on_toggle(|_| ()),
        text_input("nome", "Ana").on_input(|_| ()),
        Element::from(button(Space::new(16.0, 16.0)).on_press(())).access_label("Fechar"),
    ]
    .into();
    let (mut r, st) = (Renderer::new(), States::default());
    r.begin(Size::new(400.0, 300.0), 1.0, Color::TRANSPARENT);
    let n = layout(&el, Size::new(400.0, 300.0), &mut r, &st, &Theme::dark());
    let arvore = access_tree(&el, &n, &mut r, &st, &Theme::dark(), "Notas");
    let nos: Vec<_> = arvore.nodes.iter().map(|(_, n)| n).collect();
    let tem = |role: Role, f: &dyn Fn(&accesskit::Node) -> bool| nos.iter().any(|n| n.role() == role && f(n));
    assert!(tem(Role::Window, &|n| n.label() == Some("Notas") && n.children().len() == 5));
    assert!(tem(Role::Label, &|n| n.value() == Some("Título")));
    assert!(tem(Role::Button, &|n| n.label() == Some("Salvar")), "o texto de dentro dá nome ao botão");
    assert!(!tem(Role::Label, &|n| n.value() == Some("Salvar")), "e não aparece de novo solto");
    assert!(tem(Role::CheckBox, &|n| n.label() == Some("Lembrar") && n.toggled() == Some(Toggled::True)));
    assert!(tem(Role::TextInput, &|n| n.value() == Some("Ana") && n.placeholder() == Some("nome")));
    assert!(tem(Role::Button, &|n| n.label() == Some("Fechar")), "botão só de ícone com access_label");
    assert!(nos.iter().filter(|n| n.role() != Role::Window).all(|n| n.bounds().is_some()));
}

#[test]
fn repeat_sem_enter_pulsa_a_partir_do_normal() {
    let caixa = container(Space::new(100.0, 50.0)).style(|_| Style::default().background(Color::WHITE));
    let el: Element<()> = animate(caixa).opacity(0.0).transition(Transition::new(100).easing(Easing::Linear)).repeat(Repeat::Alternate).into();
    let (mut r, mut st, t0) = (Renderer::new(), States::default(), Instant::now());
    st.tick(t0);
    assert_eq!(quadro(&el, &mut r, &st, None)[3], 255);
    st.tick(t0 + Duration::from_millis(150));
    let a = quadro(&el, &mut r, &st, None)[3];
    assert!(a.abs_diff(128) <= 3, "voltando: {a}");
    assert!(st.animating());
}

fn caixa_branca<'a>(lado: f32) -> Container<'a, ()> {
    container(Space::new(lado, lado)).style(|_| Style::default().background(Color::WHITE))
}

fn alfa_em(r: &Renderer, x: u32, y: u32) -> u8 {
    r.pixmap.pixel(x, y).expect("dentro").alpha()
}

#[test]
fn escala_cresce_em_volta_do_centro() {
    // Caixa de 20 no meio de 100: dobrada, ela vai de 30 a 70.
    let el: Element<()> = container(animate(caixa_branca(20.0)).scale(2.0)).center().into();
    let (mut r, st) = (Renderer::new(), States::default());
    let tela = Size::new(100.0, 100.0);
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    draw(&el, &n, &mut r, &st, &Theme::dark(), None, false);
    assert_eq!(alfa_em(&r, 50, 50), 255);
    assert_eq!(alfa_em(&r, 33, 50), 255, "fora da caixa original, dentro da escalada");
    assert_eq!(alfa_em(&r, 27, 50), 0);
}

#[test]
fn show_false_sai_e_para_de_receber_clique() {
    let el = |mostrar: bool| -> Element<()> { animate(button(Space::new(50.0, 50.0)).padding(0.0).on_press(())).show(mostrar).transition(Transition::new(100).easing(Easing::Linear)).into() };
    let (mut r, mut st, t0) = (Renderer::new(), States::default(), Instant::now());
    let tela = Size::new(100.0, 100.0);
    let desenhar = |r: &mut Renderer, st: &States, e: &Element<()>| {
        r.begin(tela, 1.0, Color::TRANSPARENT);
        let n = layout(e, tela, r, st, &Theme::dark());
        draw(e, &n, r, st, &Theme::dark(), None, false);
        alfa_em(r, 10, 10)
    };
    st.tick(t0);
    assert_eq!(desenhar(&mut r, &st, &el(true)), 255);
    st.tick(t0 + Duration::from_millis(10));
    assert_eq!(desenhar(&mut r, &st, &el(false)), 255, "a saída parte de onde estava");
    st.tick(t0 + Duration::from_millis(60));
    assert!(desenhar(&mut r, &st, &el(false)).abs_diff(128) <= 3);
    st.tick(t0 + Duration::from_millis(200));
    assert_eq!(desenhar(&mut r, &st, &el(false)), 0);
    assert!(!st.animating());
    // Escondido, o clique passa direto.
    let e = el(false);
    let n = layout(&e, tela, &mut r, &st, &Theme::dark());
    let mut msgs = Vec::new();
    let mut area = MemoryClipboard::default();
    let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: Some(Point::new(10.0, 10.0)), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
    assert_eq!(dispatch(&e, &n, &Event::MousePressed(MouseButton::Left), &mut cx), Captured::No);
}

#[test]
fn hover_troca_o_alvo() {
    let el: Element<()> = animate(caixa_branca(40.0)).hover(Keyframe::default().opacity(0.5)).into();
    let (mut r, st) = (Renderer::new(), States::default());
    let tela = Size::new(100.0, 100.0);
    for (cursor, esperado) in [(None, 255), (Some(Point::new(10.0, 10.0)), 128)] {
        r.begin(tela, 1.0, Color::TRANSPARENT);
        let n = layout(&el, tela, &mut r, &st, &Theme::dark());
        draw(&el, &n, &mut r, &st, &Theme::dark(), cursor, false);
        assert!(alfa_em(&r, 10, 10).abs_diff(esperado) <= 1);
    }
}

#[test]
fn camada_no_quadro_parcial_bate_com_o_cheio() {
    // Cartão girado e escalado: gravar, comparar e pintar só as regiões tem que dar o quadro cheio.
    let cena = |k: f32| -> Element<()> {
        let fundo = container(Space::new(Length::FILL, Length::FILL)).style(|_| Style::default().background(Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(200, 30, 90)), (1.0, Color::rgb8(20, 40, 200))] }));
        let cartao = animate(container(text("oi")).padding(16.0).style(|_| Style::default().background(Color::WHITE).border(Border::rounded(8.0)))).scale(k).rotate(10.0 * k);
        stack![fundo, container(cartao).center()].into()
    };
    let (tela, tema, st) = (Size::new(300.0, 200.0), Theme::dark(), States::default());
    let mut r = Renderer::new();
    let pintar = |r: &mut Renderer, e: &Element<()>| {
        let n = layout(e, tela, r, &st, &tema);
        r.record();
        draw(e, &n, r, &st, &tema, None, false);
        let c = r.changes();
        let parcial = matches!(c, Changes::Regions(_));
        match c {
            Changes::Regions(rs) => {
                for x in rs {
                    assert!(r.begin_parcial(tela, 1.0, Color::BLACK, x));
                    draw(e, &n, r, &st, &tema, None, false);
                    r.end();
                }
            }
            Changes::All => {
                r.begin(tela, 1.0, Color::BLACK);
                draw(e, &n, r, &st, &tema, None, false);
            }
            Changes::None => {}
        }
        parcial
    };
    pintar(&mut r, &cena(1.0));
    assert!(pintar(&mut r, &cena(1.3)), "só o cartão muda: quadro parcial");
    let parcial = r.pixmap.data().to_vec();
    let mut cheio = Renderer::new();
    let e = cena(1.3);
    cheio.begin(tela, 1.0, Color::BLACK);
    let n = layout(&e, tela, &mut cheio, &st, &tema);
    draw(&e, &n, &mut cheio, &st, &tema, None, false);
    let pior = cheio.pixmap.data().iter().zip(&parcial).map(|(a, b)| a.abs_diff(*b)).max();
    assert_eq!(pior, Some(0), "o pedaço redesenhado tem que bater com o quadro cheio");
}
