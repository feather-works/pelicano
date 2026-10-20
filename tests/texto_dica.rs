//! Altura de linha do texto e o estilo do balão de dica.
use pelicano::*;

const TELA: Size = Size::new(200.0, 150.0);

fn altura(el: &Element<'_, ()>) -> f32 {
    let mut r = Renderer::new();
    r.begin(TELA, 1.0, Color::TRANSPARENT);
    layout(el, TELA, &mut r, &States::default(), &Theme::dark()).rect.height
}

#[test]
fn line_height_aumenta_a_altura_e_o_padrao_nao_muda() {
    let s = "uma\nduas\ntrês";
    let padrao = altura(&text(s).into());
    assert_eq!(padrao, altura(&text(s).line_height(1.3).into()), "1.3 é o padrão");
    let largo = altura(&text(s).line_height(1.5).into());
    assert!(largo > padrao + 5.0, "{largo} > {padrao}");
}

#[test]
fn dica_usa_o_estilo_e_o_padding() {
    let el: Element<()> = tooltip(Space::new(100.0, 50.0), "a", TooltipPosition::Bottom)
        .padding(20.0)
        .style(|_| Style::default().background(Color::rgb(1.0, 0.0, 0.0)))
        .into();
    let (mut r, st) = (Renderer::new(), States::default());
    r.begin(TELA, 1.0, Color::TRANSPARENT);
    let n = layout(&el, TELA, &mut r, &st, &Theme::dark());
    // Sem relógio a transição já está no fim: o balão aparece inteiro.
    draw(&el, &n, &mut r, &st, &Theme::dark(), Some(Point::new(10.0, 10.0)), false);
    let w = r.measure("a", 12.0, Font::DEFAULT, Some(320.0)).width + 40.0;
    let x = ((100.0 - w) / 2.0).max(4.0) as u32;
    // Topo do balão em 56; 20 de padding: dentro dele só há fundo.
    let p = r.pixmap.pixel(x + 5, 56 + 10).expect("dentro");
    assert_eq!([p.red(), p.green(), p.blue(), p.alpha()], [255, 0, 0, 255]);
}

#[test]
fn texto_respeita_o_alfa_da_cor_e_a_opacidade() {
    // O pixel mais coberto do "H": cheio com alfa 1, metade com 0,5, um quinto com opacidade 0,2.
    let mais = |a: f32, opacidade: f32| {
        let mut r = Renderer::new();
        r.begin(TELA, 1.0, Color::TRANSPARENT);
        r.push_opacity(opacidade);
        r.draw_text("H", 40.0, Font::BOLD, Color::WHITE.alpha(a), Point::new(10.0, 10.0), None);
        r.pop_opacity();
        r.pixmap.pixels().iter().map(|p| p.alpha()).max().unwrap()
    };
    assert_eq!(mais(1.0, 1.0), 255);
    assert!(mais(0.5, 1.0).abs_diff(128) <= 1, "{}", mais(0.5, 1.0));
    assert!(mais(1.0, 0.2).abs_diff(51) <= 1, "{}", mais(1.0, 0.2));
}

#[test]
fn letras_que_passam_da_linha_baixa_nao_deixam_fantasma() {
    // Linha de 1,0: o acento do Á e as pernas do gjpy saem da caixa medida.
    let cena = |r: &mut Renderer, s: &str| r.draw_text_lh(s, 64.0, Font::DEFAULT, Color::WHITE, Point::new(10.0, 40.0), None, 1.0);
    let mut r = Renderer::new();
    for (i, s) in ["Ágjpy", "x"].into_iter().enumerate() {
        r.record();
        cena(&mut r, s);
        match r.changes() {
            Changes::Regions(rs) => {
                assert!(i > 0);
                for x in rs {
                    assert!(r.begin_parcial(TELA, 1.0, Color::BLACK, x));
                    cena(&mut r, s);
                    r.end();
                }
            }
            _ => {
                r.begin(TELA, 1.0, Color::BLACK);
                cena(&mut r, s);
            }
        }
    }
    let mut cheio = Renderer::new();
    cheio.begin(TELA, 1.0, Color::BLACK);
    cena(&mut cheio, "x");
    assert!(cheio.pixmap.data() == r.pixmap.data(), "sobrou pedaço do texto de antes");
}
