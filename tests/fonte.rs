// Arquivo próprio: o `load_font` é global e mudaria a chave dos desenhos de outros testes do mesmo processo.
use pelicano::{load_font, Changes, Color, Font, Point, Renderer, Size};

#[test]
fn fonte_carregada_depois_redesenha() {
    let mut r = Renderer::new();
    let cena = |r: &mut Renderer| r.draw_text("Olá", 14.0, Font::with_name("Fonte Nova"), Color::WHITE, Point::ORIGIN, None);
    let quadro = |r: &mut Renderer| {
        r.begin(Size::new(100.0, 40.0), 1.0, Color::BLACK);
        r.record();
        cena(r);
        r.changes()
    };
    quadro(&mut r);
    assert_eq!(quadro(&mut r), Changes::None);
    // Bytes inválidos: o fontdb só ignora, mas o quadro seguinte já conta a fonte nova.
    load_font(b"nao e fonte".to_vec());
    assert_ne!(quadro(&mut r), Changes::None);
}
