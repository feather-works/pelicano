//! O markdown vira widgets: títulos, listas, código, tabela e links clicáveis.
use pelicano::*;

const TEXTO: &str = "# Título\n\nUm parágrafo com **negrito**, *itálico*, ~~tachado~~, `código` e um [link](https://exemplo.com).\n\n- item um\n- item dois\n  - dentro\n\n1. primeiro\n2. segundo\n\n> uma citação\n\n```rust\nfn main() {}\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n- [x] feito\n- [ ] por fazer\n\n---\n";

#[test]
fn monta_tudo_e_o_link_manda_o_endereco() {
    let el: Element<String> = markdown(TEXTO, 14.0, |u| u);
    let (mut r, mut st, tela) = (Renderer::new(), States::default(), Size::new(500.0, 900.0));
    let png = capture(&el, tela, 1.0, &Theme::dark(), &mut r, None);
    if let Ok(d) = std::env::var("PELICANO_PNG") {
        std::fs::write(format!("{d}/markdown.png"), png).unwrap();
    }
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    // Procura o link varrendo a segunda linha de texto (o parágrafo).
    let mut achou = Vec::new();
    let y0 = n.children[1].rect.y + 8.0;
    for x in (0..500).step_by(4) {
        let mut msgs = Vec::new();
        let mut area = MemoryClipboard::default();
        let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: Some(Point::new(x as f32, y0)), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
        dispatch(&el, &n, &Event::MousePressed(MouseButton::Left), &mut cx);
        achou.extend(msgs);
    }
    assert!(achou.iter().all(|u| u == "https://exemplo.com") && !achou.is_empty(), "{achou:?}");
    assert!(n.children.len() >= 8, "um bloco por parte: {}", n.children.len());
}
