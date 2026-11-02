//! Quanto custa pintar um quadro de PDF (página com sombra e imagem 1:1) nas faixas.
//! `cargo test --release --test medir_faixas -- --ignored --nocapture`
use pelicano::*;

#[test]
#[ignore]
fn pagina_pdf() {
    let (pw, ph) = (816u32, 1056u32);
    let px: Vec<u8> = (0..pw * ph).flat_map(|i| { let v = if (i / pw) % 20 < 2 && i % pw > 80 && i % pw < 700 { 40 } else { 255 }; [v, v, v, 255] }).collect();
    let img = Image::from_rgba(pw, ph, px).expect("tamanho");
    let tam = Size::new(1920.0, 1040.0);
    let mut r = Renderer::new();
    for passo in 0..12 {
        let y = -200.0 - passo as f32 * 37.3;
        let cena = |r: &mut Renderer| {
            r.fill_quad(Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1040.0 }, Some(&Background::Color(Color { r: 0.2, g: 0.2, b: 0.22, a: 1.0 })), Border::default(), None);
            if std::env::var_os("CANTO").is_some() { r.push_clip(Rect { x: 300.0, y: 40.0, width: 1600.0, height: 990.0 }, [8.0; 4]); }
            for p in 0..2 {
                let z = if std::env::var_os("ESTICADA").is_some() { 0.87 } else { 1.0 };
                let pr = Rect { x: 552.0, y: y + p as f32 * (ph as f32 * z + 16.0), width: pw as f32 * z, height: ph as f32 * z };
                r.fill_quad(pr, Some(&Background::Color(Color::WHITE)), Border::default(), std::env::var_os("SEM_SOMBRA").is_none().then_some(Shadow { color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.3 }, offset: Point::new(0.0, 2.0), blur: 8.0 }));
                if std::env::var_os("SEM_IMG").is_none() { r.draw_image(&img, pr, [0.0; 4]); }
                //r.draw_image(&img, pr, [0.0; 4]);
            }
        };
        let t = std::time::Instant::now();
        r.capture_bands(tam, 1.0, Color::BLACK, cena).expect("na lista");
        if passo > 1 {
            eprintln!("quadro {:.1?}", t.elapsed());
        }
    }
}

/// Grava os pixels de uma cena com recorte arredondado em `$CENA_SAIDA` (para comparar versões).
#[test]
#[ignore]
fn cena_para_comparar() {
    let Some(saida) = std::env::var_os("CENA_SAIDA") else { return };
    let px: Vec<u8> = (0..200 * 150).flat_map(|i| [(i * 7 % 256) as u8, (i * 13 % 256) as u8, (i / 200) as u8, if i % 97 == 0 { 128 } else { 255 }]).collect();
    let img = Image::from_rgba(200, 150, px).expect("tamanho");
    let mut r = Renderer::new();
    let mut tudo = Vec::new();
    for (dx, dy) in [(0.0, 0.0), (0.3, 0.6), (-7.5, -20.25)] {
        let b = r.capture_bands(Size::new(320.0, 240.0), 1.0, Color::BLACK, |r| {
            r.push_clip(Rect { x: 10.5, y: 8.0, width: 300.0, height: 220.0 }, [12.0; 4]);
            r.fill_quad(Rect { x: 0.0, y: 0.0, width: 320.0, height: 240.0 }, Some(&Background::Color(Color { r: 0.3, g: 0.3, b: 0.35, a: 1.0 })), Border::default(), None);
            let pr = Rect { x: 40.0 + dx, y: 20.0 + dy, width: 200.0, height: 150.0 };
            r.fill_quad(pr, Some(&Background::Color(Color::WHITE)), Border { color: Color::BLACK, width: 1.0, radius: [0.0; 4] }, Some(Shadow { color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.3 }, offset: Point::new(0.0, 2.0), blur: 6.0 }));
            r.draw_image(&img, pr, [0.0; 4]);
            r.draw_image(&img, Rect { x: 150.0 + dx * 3.0, y: 60.0 + dy, width: 151.3, height: 170.7 }, [0.0; 4]);
            for i in 0..12 {
                r.draw_text("Texto de teste ÁÉ gy", 14.0, Font::default(), Color::WHITE, Point::new(5.0 + dx, i as f32 * 19.0 + dy), None);
            }
            r.pop_clip();
        }).expect("na lista");
        tudo.extend(b);
    }
    std::fs::write(saida, tudo).expect("grava");
}
