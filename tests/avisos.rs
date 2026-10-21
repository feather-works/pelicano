//! Os trechos sublinhados em onda do editor e a dica deles.
use pelicano::*;

const TELA: Size = Size::new(400.0, 200.0);

/// O quadro do editor com estes avisos e o mouse em `mouse`.
fn quadro(c: &Content, avisos: Vec<Warning>, mouse: Option<Point>) -> Renderer {
    let e: Element<()> = text_editor(c).warnings(avisos).into();
    let (mut r, st) = (Renderer::new(), States::default());
    r.begin(TELA, 1.0, Color::BLACK);
    let n = layout(&e, TELA, &mut r, &st, &Theme::dark());
    // Sem relógio a transição da dica já está no fim.
    draw(&e, &n, &mut r, &st, &Theme::dark(), mouse, false);
    r
}

#[test]
fn onda_embaixo_do_trecho_e_dica_com_o_mouse() {
    let c = Content::with_text("um link falso aqui\nsegunda");
    let aviso = || vec![Warning { line: 0, range: 3..13, tip: "Dica do aviso".into() }];
    let (limpo, com) = (quadro(&c, Vec::new(), None), quadro(&c, aviso(), None));
    // A onda é o que muda: só na primeira linha e só sob "link falso" (não sob "um" nem "aqui").
    let onda: Vec<(u32, u32)> = (0..TELA.height as u32).flat_map(|y| (0..TELA.width as u32).map(move |x| (x, y))).filter(|&(x, y)| limpo.pixmap.pixel(x, y) != com.pixmap.pixel(x, y)).collect();
    assert!(!onda.is_empty(), "sem onda");
    let (x0, x1) = (onda.iter().map(|p| p.0).min().unwrap() as f32, onda.iter().map(|p| p.0).max().unwrap() as f32);
    let mut regua = Renderer::new();
    let mut medir = |s: &str| regua.measure(s, 15.0, Font::DEFAULT, None).width;
    assert!((x0 - 12.0 - medir("um ")).abs() < 2.0, "começa em \"link\": {x0}");
    assert!((x1 - 12.0 - medir("um link falso")).abs() < 2.0, "acaba em \"falso\": {x1}");
    assert!(onda.iter().all(|p| p.1 < 12 + 22), "só na primeira linha");

    // Mouse em cima: a dica aparece.
    let dica = quadro(&c, aviso(), Some(Point::new((x0 + x1) / 2.0, onda[0].1 as f32 - 6.0)));
    assert_ne!(dica.pixmap.data(), com.pixmap.data(), "a dica tem que aparecer");
    if let Some(d) = std::env::var_os("PELICANO_CAPTURAR") {
        std::fs::write(std::path::Path::new(&d).join("avisos.png"), dica.pixmap.encode_png().unwrap()).unwrap();
    }
}

#[test]
fn trecho_fora_do_texto_nao_quebra() {
    let c = Content::with_text("curto");
    quadro(&c, vec![Warning { line: 5, range: 0..3, tip: "x".into() }, Warning { line: 0, range: 2..90, tip: "y".into() }], Some(Point::new(30.0, 20.0)));
}

/// Ctrl+clique abre (manda o endereço); clique simples não; link com aviso não é link.
#[test]
fn ctrl_clique_no_link_manda_o_endereco() {
    let texto = "[site](https://a.b) e [falso](https://pаypal.com)";
    let c = Content::with_text(texto);
    let p = texto.find("pаypal").unwrap();
    let e: Element<String> = text_editor(&c).markdown(true).warnings(vec![Warning { line: 0, range: p..p + "pаypal".len(), tip: "falso".into() }]).on_link(|u| u).into();
    let (mut r, mut st, mut area) = (Renderer::new(), States::default(), MemoryClipboard::default());
    let mut ev = |ponto: Point, ev: Event| {
        r.begin(TELA, 1.0, Color::TRANSPARENT);
        let n = layout(&e, TELA, &mut r, &st, &Theme::dark());
        // Como a janela: o quadro é desenhado antes do clique (é o desenho que monta as linhas).
        draw(&e, &n, &mut r, &st, &Theme::dark(), Some(ponto), false);
        let mut msgs = Vec::new();
        let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: Some(ponto), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
        dispatch(&e, &n, &ev, &mut cx);
        msgs
    };
    // A linha toda, da esquerda para a direita: o que cada clique manda.
    let varrer = |ev: &mut dyn FnMut(Point, Event) -> Vec<String>| -> Vec<String> {
        let mut todos = Vec::new();
        for x in (12..390).step_by(3) {
            let p = Point::new(x as f32, 22.0);
            todos.extend(ev(p, Event::MousePressed(MouseButton::Left)));
            ev(p, Event::MouseReleased(MouseButton::Left));
        }
        todos
    };
    assert!(varrer(&mut ev).is_empty(), "sem Ctrl o clique só põe o cursor");
    ev(Point::ORIGIN, Event::ModifiersChanged(Modifiers::CONTROL));
    let abertos = varrer(&mut ev);
    assert!(!abertos.is_empty() && abertos.iter().all(|u| u == "https://a.b"), "{abertos:?}");
}
