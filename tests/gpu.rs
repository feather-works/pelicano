//! A GPU desenha o mesmo que a CPU: cada cena sai pelos dois e os pixels são comparados.
use pelicano::*;

const FUNDO: Color = Color { r: 0.1, g: 0.11, b: 0.13, a: 1.0 };

/// A cena pela lista pintada em faixas na CPU e pela GPU; falha se mais que `fracao` dos pixels
/// difere mais que `tol` num canal. As duas misturam em espaço linear; o tiny-skia mistura em
/// sRGB (bordas suavizadas e translúcidos mudam), então contra ele só se confere a geometria.
fn comparar(nome: &str, tam: Size, escala: f32, tol: u8, fracao: f64, cena: impl Fn(&mut Renderer)) {
    let mut r = Renderer::new();
    r.begin(tam, escala, FUNDO);
    cena(&mut r);
    let cpu = r.pixmap.data().to_vec();
    let faixas = r.capture_bands(tam, escala, FUNDO, &cena).unwrap_or_else(|m| panic!("{nome}: a lista não representa a cena ({m})"));
    diferenca(&format!("{nome}: tiny-skia × faixas"), &cpu, &faixas, 128, 0.01);
    let Some(mut g) = GpuOffscreen::new() else {
        eprintln!("{nome}: sem adaptador gráfico, pulando a GPU");
        return;
    };
    let gpu = g.capture(&mut r, tam, escala, FUNDO, &cena).unwrap_or_else(|m| panic!("{nome}: caiu na CPU ({m})"));
    diferenca(nome, &faixas, &gpu, tol, fracao);
}

fn diferenca(nome: &str, cpu: &[u8], outro: &[u8], tol: u8, fracao: f64) {
    assert_eq!(cpu.len(), outro.len());
    let (mut ruins, mut pior) = (0usize, 0u8);
    for (a, b) in cpu.chunks(4).zip(outro.chunks(4)) {
        let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        pior = pior.max(d);
        ruins += (d > tol) as usize;
    }
    let n = cpu.len() / 4;
    eprintln!("{nome}: {ruins} de {n} pixels passam de {tol} (pior {pior})");
    assert!((ruins as f64) <= fracao * n as f64, "{nome}: {ruins} de {n} pixels diferem mais que {tol} (pior {pior})");
}

#[test]
fn vidro() {
    // O vidro não passa pelas faixas: compara direto o tiny-skia com a GPU. Os dois desfocam em
    // sRGB; sobra o arredondamento de 8 bits em cada passe e a borda suavizada dos cantos.
    let cena = |r: &mut Renderer| {
        for i in 0..12 {
            let cor = if i % 2 == 0 { Color::rgb8(0xf5, 0x9e, 0x0b) } else { Color::rgb8(0x3b, 0x82, 0xf6) };
            r.fill_quad(Rect { x: i as f32 * 20.0, y: 0.0, width: 20.0, height: 160.0 }, Some(&Background::Color(cor)), Border::default(), None);
        }
        // Só o vidro: um translúcido por cima já difere pela mistura em sRGB do tiny-skia.
        r.blur(Rect { x: 30.0, y: 30.0, width: 170.0, height: 90.0 }, 20.0, [12.0; 4]);
    };
    let (tam, escala) = (Size::new(240.0, 160.0), 1.25);
    let mut r = Renderer::new();
    r.begin(tam, escala, FUNDO);
    cena(&mut r);
    let cpu = r.pixmap.data().to_vec();
    let Some(mut g) = GpuOffscreen::new() else {
        eprintln!("vidro: sem adaptador gráfico, pulando a GPU");
        return;
    };
    let gpu = g.capture(&mut r, tam, escala, FUNDO, cena).unwrap_or_else(|m| panic!("vidro: caiu na CPU ({m})"));
    diferenca("vidro", &cpu, &gpu, 6, 0.01);
}

#[test]
fn quadros_com_borda_e_sombra() {
    comparar("quadros", Size::new(240.0, 160.0), 1.0, 6, 0.01, |r| {
        let caixa = Rect { x: 20.0, y: 20.0, width: 120.0, height: 70.0 };
        r.fill_quad(caixa, Some(&Background::Color(Color::rgb8(0x3b, 0x82, 0xf6))), Border { color: Color::WHITE, width: 2.0, radius: [12.0, 4.0, 20.0, 0.0] }, Some(Shadow { color: Color::BLACK.alpha(0.6), offset: Point::new(0.0, 4.0), blur: 12.0 }));
        let g = Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(0x8b, 0x5c, 0xf6)), (1.0, Color::rgb8(0xec, 0x48, 0x99))] };
        r.fill_quad(Rect { x: 150.0, y: 30.0, width: 70.0, height: 100.0 }, Some(&g), Border::rounded(8.0), None);
    });
}

#[test]
fn recorte_arredondado_aninhado() {
    comparar("recorte", Size::new(200.0, 200.0), 2.0, 8, 0.01, |r| {
        r.push_clip(Rect { x: 20.0, y: 20.0, width: 160.0, height: 160.0 }, [30.0; 4]);
        r.push_clip(Rect { x: 0.0, y: 0.0, width: 120.0, height: 200.0 }, [0.0; 4]);
        r.fill_quad(Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 }, Some(&Background::Color(Color::rgb8(0xf5, 0x9e, 0x0b))), Border::default(), None);
        r.pop_clip();
        r.pop_clip();
    });
}

#[test]
fn texto() {
    comparar("texto", Size::new(260.0, 80.0), 1.0, 2, 0.001, |r| {
        r.draw_text("Pelicano na GPU — ação 123", 16.0, Font::DEFAULT, Color::WHITE, Point::new(10.0, 10.0), None);
        r.draw_text("negrito translúcido", 20.0, Font::BOLD, Color::rgb8(0x22, 0xc5, 0x5e).alpha(0.7), Point::new(10.0, 40.0), None);
    });
}

#[test]
fn imagem() {
    let px: Vec<u8> = (0..32 * 32).flat_map(|i| [(i % 32 * 8) as u8, (i / 32 * 8) as u8, 128, 255]).collect();
    let img = Image::from_rgba(32, 32, px).expect("tamanho confere");
    comparar("imagem", Size::new(160.0, 120.0), 1.0, 6, 0.02, |r| {
        r.draw_image(&img, Rect { x: 10.0, y: 10.0, width: 32.0, height: 32.0 }, [0.0; 4]);
        r.draw_image(&img, Rect { x: 50.0, y: 10.0, width: 100.0, height: 100.0 }, [16.0; 4]);
    });
}

#[test]
fn linha_grossa() {
    comparar("linha", Size::new(200.0, 150.0), 1.0, 40, 0.01, |r| {
        let pontos: Vec<Point> = (0..20).map(|i| Point::new(10.0 + i as f32 * 9.0, 75.0 + 50.0 * (i as f32 * 0.6).sin())).collect();
        r.stroke(&pontos, 8.0, Color::rgb8(0x06, 0xb6, 0xd4));
        let p = Path::polyline(&[Point::new(20.0, 20.0), Point::new(180.0, 30.0), Point::new(100.0, 130.0)], true).unwrap();
        r.stroke_path(&p, &Stroke::new(3.0).cap(LineCap::Round).join(LineJoin::Round), Color::WHITE);
    });
}

#[test]
fn caminhos_com_curva() {
    comparar("caminhos", Size::new(240.0, 180.0), 1.5, 8, 0.01, |r| {
        let estrela = Path::new(|b| {
            b.move_to(Point::new(60.0, 10.0)).quadratic_to(Point::new(110.0, 40.0), Point::new(100.0, 90.0));
            b.bezier_to(Point::new(80.0, 120.0), Point::new(30.0, 110.0), Point::new(20.0, 60.0)).close();
            b.circle(Point::new(60.0, 60.0), 18.0);
        })
        .unwrap();
        r.fill_path(&estrela, Color::rgb8(0x3b, 0x82, 0xf6), FillRule::EvenOdd);
        r.push_transform(Transform::rotate_around(30.0, Point::new(170.0, 60.0)));
        let g = Background::Linear { angle: 0.0, stops: vec![(0.0, Color::rgb8(0x8b, 0x5c, 0xf6)), (1.0, Color::rgb8(0xec, 0x48, 0x99))] };
        r.fill_path(&Path::rect(Rect { x: 130.0, y: 30.0, width: 80.0, height: 60.0 }).unwrap(), g, FillRule::NonZero);
        r.pop_transform();
        r.push_opacity(0.6);
        let onda = Path::new(|b| {
            b.move_to(Point::new(10.0, 150.0));
            for i in 0..8 {
                let x = 10.0 + i as f32 * 28.0;
                b.quadratic_to(Point::new(x + 7.0, 130.0), Point::new(x + 14.0, 150.0)).quadratic_to(Point::new(x + 21.0, 170.0), Point::new(x + 28.0, 150.0));
            }
        })
        .unwrap();
        r.stroke_path(&onda, &Stroke::new(3.0).dash(vec![9.0, 4.0]), Color::rgb8(0xef, 0x44, 0x44));
        r.pop_opacity();
    });
}

/// Com zoom, o contorno do slide passa da janela (e do maior lado de textura): só a parte
/// visível vai à GPU, em vez de o quadro inteiro cair na CPU.
#[test]
fn contorno_maior_que_a_janela() {
    comparar("contorno grande", Size::new(200.0, 120.0), 1.0, 8, 0.01, |r| {
        let slide = Path::rect(Rect { x: -2100.0, y: -1500.0, width: 2250.0, height: 1600.0 }).unwrap();
        r.stroke_path(&slide, &Stroke::new(1.0), Color::rgb8(0xd0, 0xd4, 0xdc));
    });
}

/// Quadro cheio de uma tabela (como o exemplo csv): `cargo test --release --test gpu -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_tabela_cheia() {
    let Some(mut g) = GpuOffscreen::new() else { return };
    let linhas: Vec<Vec<String>> = (0..40).map(|l| (0..12).map(|c| format!("célula {l}·{c} {}", l * 7919 % 1000)).collect()).collect();
    let tabela = || -> Element<'_, ()> {
        let filas = linhas.iter().map(|l| {
            let cels = l.iter().map(|s| container(text(s.as_str()).size(13.0).no_wrap()).width(104.0).height(20.0).padding([0, 8]).clip(true).into());
            container(row(cels)).style(|_| Style::default().border(Border { color: Color::rgb8(0xd0, 0xd7, 0xde), width: 1.0, radius: [0.0; 4] })).into()
        });
        column(filas).into()
    };
    let (tema, st) = (Theme::light(), States::default());
    for escala in [1.0, 2.0] {
        let tam = Size::new(1280.0, 800.0);
        let mut r = Renderer::new();
        let el = tabela();
        let n = widget::layout(&el, tam, &mut r, &st, &tema);
        let (mut cpu, mut gpu, mut faixas) = (Vec::new(), Vec::new(), Vec::new());
        for _ in 0..30 {
            let t = std::time::Instant::now();
            r.begin(tam, escala, tema.background);
            widget::draw(&el, &n, &mut r, &st, &tema, None, false);
            cpu.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = std::time::Instant::now();
            r.capture_bands(tam, escala, tema.background, |r| { widget::draw(&el, &n, r, &st, &tema, None, false); }).expect("tabela inteira na lista");
            faixas.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = std::time::Instant::now();
            g.capture(&mut r, tam, escala, tema.background, |r| {
                widget::draw(&el, &n, r, &st, &tema, None, false);
            })
            .expect("tabela inteira na GPU");
            gpu.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let med = |v: &mut Vec<f64>| { v.sort_by(f64::total_cmp); v[v.len() / 2] };
        eprintln!("escala {escala}: CPU {:.2} ms · CPU em faixas {:.2} ms · GPU (com leitura de volta) {:.2} ms", med(&mut cpu), med(&mut faixas), med(&mut gpu));
    }
}

/// 100 mil segmentos de linha (o teste de estresse das cordas), CPU contra GPU.
#[test]
#[ignore]
fn bench_cem_mil_segmentos() {
    let Some(mut g) = GpuOffscreen::new() else { return };
    let tam = Size::new(1280.0, 800.0);
    let cordas: Vec<Vec<Point>> = (0..1000).map(|c| (0..101).map(|i| Point::new(10.0 + i as f32 * 12.5, 20.0 + c as f32 * 0.76 + 8.0 * (i as f32 * 0.3 + c as f32).sin())).collect()).collect();
    let cena = |r: &mut Renderer| {
        for (i, c) in cordas.iter().enumerate() {
            r.stroke(c, 1.5, Color::rgb(0.2 + (i % 5) as f32 * 0.15, 0.5, 0.8));
        }
    };
    let mut r = Renderer::new();
    let (mut cpu, mut gpu) = (Vec::new(), Vec::new());
    for _ in 0..10 {
        let t = std::time::Instant::now();
        r.begin(tam, 1.0, Color::BLACK);
        cena(&mut r);
        cpu.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = std::time::Instant::now();
        g.capture(&mut r, tam, 1.0, Color::BLACK, cena).expect("tudo na GPU");
        gpu.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let med = |v: &mut Vec<f64>| { v.sort_by(f64::total_cmp); v[v.len() / 2] };
    eprintln!("100k segmentos: CPU {:.2} ms · GPU (com leitura de volta) {:.2} ms", med(&mut cpu), med(&mut gpu));
}
