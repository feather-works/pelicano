//! Texto rico: trechos de tamanhos diferentes no mesmo parágrafo, e o link que recebe o clique.
use pelicano::*;

#[test]
fn link_no_meio_do_texto_recebe_o_clique() {
    let el: Element<&str> = rich_text([span("Leia o "), span("manual").link("abrir").size(20.0), span(" antes.")]).into();
    let (mut r, mut st, tela) = (Renderer::new(), States::default(), Size::new(400.0, 100.0));
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    assert!(n.rect.height >= 20.0, "a linha cresce com o trecho maior: {}", n.rect.height);
    let largura_curta = r.measure("Leia o ", 14.0, Font::DEFAULT, None).width;
    let clicar = |r: &mut Renderer, st: &mut States, x: f32| {
        let mut msgs = Vec::new();
        let mut area = MemoryClipboard::default();
        let mut cx = EventCx { renderer: r, states: st, cursor: Some(Point::new(x, 10.0)), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
        dispatch(&el, &n, &Event::MousePressed(MouseButton::Left), &mut cx);
        msgs
    };
    assert_eq!(clicar(&mut r, &mut st, largura_curta + 10.0), vec!["abrir"]);
    assert!(clicar(&mut r, &mut st, 5.0).is_empty(), "fora do link nada acontece");
}

#[test]
fn quebra_a_linha_na_largura() {
    let el: Element<()> = rich_text([span("uma frase comprida que precisa quebrar "), span("em negrito").font(Font::BOLD)]).width(120.0).into();
    let (mut r, st, tela) = (Renderer::new(), States::default(), Size::new(400.0, 400.0));
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    assert!(n.rect.height > 40.0, "várias linhas: {}", n.rect.height);
    draw(&el, &n, &mut r, &st, &Theme::dark(), None, false);
}

#[test]
fn rolagem_horizontal_com_shift_e_on_scroll() {
    let el: Element<Viewport> = scrollable(Space::new(1000.0, 50.0)).direction(Direction::Horizontal).on_scroll(|v| v).height(50.0).into();
    let (mut r, mut st, tela) = (Renderer::new(), States::default(), Size::new(200.0, 100.0));
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let mut msgs = Vec::new();
    for _ in 0..2 {
        let n = layout(&el, tela, &mut r, &st, &Theme::dark());
        let mut area = MemoryClipboard::default();
        let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: Some(Point::new(10.0, 10.0)), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
        dispatch(&el, &n, &Event::Wheel { dx: 0.0, dy: -100.0, modifiers: Modifiers::SHIFT }, &mut cx);
    }
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    assert_eq!(n.children[0].rect.x, -200.0, "duas roladas de 100 para a direita");
    assert_eq!(msgs.iter().map(|v| v.offset.x).collect::<Vec<_>>(), vec![100.0, 200.0], "um aviso por mudança");
}

#[test]
fn responsive_monta_com_o_tamanho_que_tem() {
    let el: Element<()> = responsive(|s| text(format!("{}", s.width as u32)).into()).into();
    let (mut r, st) = (Renderer::new(), States::default());
    for w in [300.0, 120.0] {
        let tela = Size::new(w, 50.0);
        r.begin(tela, 1.0, Color::TRANSPARENT);
        let n = layout(&el, tela, &mut r, &st, &Theme::dark());
        let esperado = r.measure(&format!("{}", w as u32), 14.0, Font::DEFAULT, None).width;
        assert_eq!(n.children[0].rect.width, esperado);
        draw(&el, &n, &mut r, &st, &Theme::dark(), None, false);
    }
}
