//! A tabela de Markdown editada na grade: Tab, Enter e os "+" de linha e coluna.
use pelicano::*;

const TELA: Size = Size::new(500.0, 300.0);

/// Desenha (é o desenho que acha a grade) e manda `ev` com o mouse em `ponto`.
fn evento(c: &Content, st: &mut States, ponto: Point, ev: Event) {
    let e: Element<()> = text_editor(c).markdown(true).into();
    let (mut r, mut area) = (Renderer::new(), MemoryClipboard::default());
    r.begin(TELA, 1.0, Color::TRANSPARENT);
    let n = layout(&e, TELA, &mut r, st, &Theme::dark());
    draw(&e, &n, &mut r, st, &Theme::dark(), Some(ponto), false);
    let mut msgs = Vec::new();
    let mut cx = EventCx { renderer: &mut r, states: st, cursor: Some(ponto), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
    dispatch(&e, &n, &ev, &mut cx);
}

fn tecla(k: NamedKey, modifiers: Modifiers) -> Event {
    Event::Key { key: Key::Named(k), text: None, modifiers, pressed: true }
}

#[test]
fn tab_enter_e_os_mais() {
    let c = Content::with_text("| Nome | Valor |\n|---|---|\n| Café | 10 |\n\nfim");
    let mut st = States::default();
    let longe = Point::new(480.0, 280.0);
    // Clica no "Café" (x da primeira coluna, y da terceira linha do texto).
    let altura = 15.0 * 1.45;
    evento(&c, &mut st, Point::new(30.0, altura * 2.0 - 2.0), Event::MousePressed(MouseButton::Left));
    evento(&c, &mut st, longe, Event::MouseReleased(MouseButton::Left));
    assert_eq!(c.cursor().0, 3, "o clique cai na linha do Café");
    // Tab: a célula seguinte inteira selecionada.
    evento(&c, &mut st, longe, tecla(NamedKey::Tab, Modifiers::empty()));
    assert_eq!(c.selection().as_deref(), Some("10"));
    // Shift+Tab volta.
    evento(&c, &mut st, longe, tecla(NamedKey::Tab, Modifiers::SHIFT));
    assert_eq!(c.selection().as_deref(), Some("Café"));
    // Tab na última célula: linha nova no fim da tabela.
    evento(&c, &mut st, longe, tecla(NamedKey::Tab, Modifiers::empty()));
    evento(&c, &mut st, longe, tecla(NamedKey::Tab, Modifiers::empty()));
    assert_eq!(c.text(), "| Nome | Valor |\n|---|---|\n| Café | 10 |\n|  |  |\n\nfim");
    // Enter: outra linha embaixo da do cursor.
    evento(&c, &mut st, longe, tecla(NamedKey::Enter, Modifiers::empty()));
    assert_eq!(c.line_count(), 7);
    assert_eq!(c.line(5).unwrap(), "|  |  |");
    assert_eq!(c.cursor(), (5, 3));
}

#[test]
fn o_mais_da_direita_poe_coluna() {
    let c = Content::with_text("| A | B |\n|---|---|\n| 1 | 2 |");
    let mut st = States::default();
    // Acha a borda direita da grade: o primeiro x, da direita para a esquerda, em que o clique pega coluna nova.
    for (x, y) in (20..480).rev().step_by(4).map(|x| (x, 20)) {
        let p = Point::new(x as f32, y as f32);
        evento(&c, &mut st, p, Event::MousePressed(MouseButton::Left));
        evento(&c, &mut st, p, Event::MouseReleased(MouseButton::Left));
        if c.text().contains("Coluna 3") {
            assert_eq!(c.text(), "| A | B | Coluna 3 |\n|---|---|---|\n| 1 | 2 |  |");
            assert_eq!(c.selection().as_deref(), Some("Coluna 3"));
            return;
        }
    }
    panic!("nenhum clique achou o + da coluna: {:?}", c.text());
}

#[test]
fn o_mais_de_baixo_poe_linha() {
    let c = Content::with_text("| A | B |\n|---|---|\n| 1 | 2 |\n\nfim");
    let mut st = States::default();
    // Acha o fim da grade: o primeiro y, de cima para baixo, em que o clique no canto esquerdo pega linha nova.
    for y in (40..200).step_by(4) {
        let p = Point::new(21.0, y as f32);
        evento(&c, &mut st, p, Event::MousePressed(MouseButton::Left));
        evento(&c, &mut st, p, Event::MouseReleased(MouseButton::Left));
        if c.line_count() == 6 {
            assert_eq!(c.text(), "| A | B |\n|---|---|\n| 1 | 2 |\n|  |  |\n\nfim");
            assert_eq!(c.cursor(), (4, 3), "o cursor vai para a linha nova");
            return;
        }
    }
    panic!("nenhum clique achou o + da linha: {:?}", c.text());
}
