//! Rastros: arrastando coisas quadro a quadro, o quadro redesenhado só nas regiões que mudaram
//! (tiny-skia parcial e faixas da CPU) tem de sair igual ao quadro pintado do zero.
use super::vetor::{Path, Stroke, Transform};
use super::*;
use crate::{Background, Border, Color, FillRule, Font, Point, Rect, Shadow, Size};

const TAM: Size = Size { width: 320.0, height: 220.0 };

/// A cena no quadro `t`: o que se arrasta num editor de slides.
fn cena(r: &mut Renderer, img: Option<&Image>, t: f32) {
    let x = 20.0 + t * 7.3;
    let y = 30.0 + t * 3.1;
    r.fill_quad(Rect::new(Point::new(0.0, 0.0), Size::new(320.0, 220.0)), Some(&Background::from(Color::rgb8(240, 240, 240))), Border::default(), None);
    // Caixa com sombra e borda.
    let sombra = Shadow { color: Color::BLACK.alpha(0.5), offset: Point::new(4.0, 6.0), blur: 10.0 };
    r.fill_quad(Rect::new(Point::new(x, y), Size::new(60.0, 40.0)), Some(&Background::from(Color::rgb8(30, 120, 220))), Border::rounded(6.0).color(Color::rgb8(200, 40, 40), 3.0), Some(sombra));
    // Forma girada, preenchida e contornada (o contorno grosso passa da caixa).
    let c = Point::new(x + 120.0, y + 40.0);
    r.push_transform(Transform::rotate_around(t * 9.0, c));
    if let Some(p) = Path::rect(Rect::new(Point::new(c.x - 30.0, c.y - 20.0), Size::new(60.0, 40.0))) {
        r.fill_path(&p, Color::rgb8(250, 180, 30), FillRule::NonZero);
        r.stroke_path(&p, &Stroke::new(6.0), Color::rgb8(20, 20, 20));
    }
    r.fill_text("Girado", 14.0, Font::DEFAULT, Color::BLACK, Point::new(c.x - 25.0, c.y - 8.0));
    // Imagem girada: a GPU ainda não faz (o quadro sai todo no tiny-skia).
    if let Some(img) = img {
        r.draw_image(img, Rect::new(Point::new(c.x - 10.0, c.y + 25.0), Size::new(20.0, 20.0)), [0.0; 4]);
    }
    r.pop_transform();
    // Alças de seleção (traços redondos) e um texto comum.
    r.stroke(&[Point::new(x - 4.0, y - 4.0), Point::new(x + 64.0, y - 4.0), Point::new(x + 64.0, y + 44.0)], 2.0, Color::rgb8(0, 120, 255));
    if let Some(p) = Path::circle(Point::new(x + 64.0, y + 44.0), 5.0) {
        r.fill_path(&p, Color::WHITE, FillRule::NonZero);
        r.stroke_path(&p, &Stroke::new(1.5), Color::rgb8(0, 120, 255));
    }
    r.draw_text("Arrastando", 15.0, Font::DEFAULT, Color::rgb8(40, 40, 40), Point::new(x, y + 60.0), None);
}

fn imagem() -> Image {
    let px: Vec<u8> = (0..16 * 16).flat_map(|i| [(i * 13 % 256) as u8, 90, 200, 255]).collect();
    Image::from_rgba(16, 16, px).expect("imagem")
}

fn diferentes(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 2)).count()
}

type Desenho<'a> = &'a dyn Fn(&mut Renderer);

/// Arrasta 12 quadros, nas escalas 1 e 1,5: `parcial` redesenha só as regiões que mudaram
/// (`None` = tudo) sobre o quadro anterior, `cheio` pinta do zero num renderizador novo.
fn arrastar(girar_imagem: bool, parcial: impl Fn(&mut Renderer, f32, Option<&[Rect]>, Desenho), cheio: impl Fn(&mut Renderer, f32, Desenho) -> Vec<u8>) {
    let img = imagem();
    for escala in [1.0, 1.5] {
        let mut r = Renderer::new();
        r.gpu.max_textura = gpu::SEM_LIMITE_TEXTURA;
        for t in 0..12 {
            let t = t as f32;
            let d = |r: &mut Renderer| cena(r, girar_imagem.then_some(&img), t);
            r.record();
            d(&mut r);
            let regioes = match r.changes() {
                Changes::Regions(rs) => Some(rs),
                Changes::None => Some(Vec::new()),
                Changes::All => None,
            };
            parcial(&mut r, escala, regioes.as_deref(), &d);
            if t == 0.0 {
                continue;
            }
            // Senão o teste não exercita o parcial.
            assert!(regioes.as_ref().is_some_and(|rs| !rs.is_empty()), "quadro {t}: {regioes:?}");
            let mut novo = Renderer::new();
            novo.gpu.max_textura = gpu::SEM_LIMITE_TEXTURA;
            let esperado = cheio(&mut novo, escala, &d);
            let n = diferentes(r.pixmap.data(), &esperado);
            assert_eq!(n, 0, "escala {escala}, quadro {t}: {n} pixels de rastro");
        }
    }
}

#[test]
fn sem_rastro_no_parcial_do_tiny_skia() {
    arrastar(
        true,
        |r, e, regioes, d| {
            let ok = regioes.is_some_and(|rs| rs.iter().all(|s| r.begin_parcial(TAM, e, Color::WHITE, *s) && {
                d(r);
                r.end();
                true
            }));
            if !ok {
                r.begin(TAM, e, Color::WHITE);
                d(r);
                r.end();
            }
        },
        |n, e, d| {
            n.begin(TAM, e, Color::WHITE);
            d(n);
            n.end();
            n.pixmap.data().to_vec()
        },
    );
}

#[test]
fn sem_rastro_nas_faixas() {
    arrastar(
        false,
        |r, e, regioes, d| {
            r.begin_gpu(TAM, e, Color::WHITE);
            d(r);
            r.end();
            assert_eq!(r.falta_gpu, None);
            r.pintar_lista_cpu(regioes);
        },
        |n, e, d| n.capture_bands(TAM, e, Color::WHITE, |r| d(r)).expect("faixas"),
    );
}

/// Um ícone que começa antes da região parcial: o `draw_pixmap` do tiny-skia, com deslocamento
/// negativo, pintava uma linha a mais embaixo.
#[test]
fn svg_cortado_pela_regiao() {
    let svg = Svg::from_bytes(br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#18a050"/></svg>"##).expect("svg");
    let d = |r: &mut Renderer| r.draw_svg(&svg, Rect::new(Point::new(13.0, 8.5), Size::new(27.0, 27.0)), None);
    let mut r = Renderer::new();
    r.begin(TAM, 1.25, Color::WHITE);
    d(&mut r);
    let cheio = r.pixmap.data().to_vec();
    assert!(r.begin_parcial(TAM, 1.25, Color::WHITE, Rect::new(Point::new(19.0, 10.5), Size::new(100.0, 100.0))));
    d(&mut r);
    r.end();
    assert_eq!(diferentes(r.pixmap.data(), &cheio), 0);
}
