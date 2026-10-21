//! A GPU desenha o mesmo que a CPU: cada cena sai pelos dois e os pixels são comparados.
use pelicano::*;

const FUNDO: Color = Color { r: 0.1, g: 0.11, b: 0.13, a: 1.0 };

/// A cena pelos dois caminhos; falha se mais que `fracao` dos pixels difere mais que `tol` num canal.
fn comparar(nome: &str, tam: Size, escala: f32, tol: u8, fracao: f64, cena: impl Fn(&mut Renderer)) {
    let Some(mut g) = GpuOffscreen::new() else {
        eprintln!("{nome}: sem adaptador gráfico, pulando");
        return;
    };
    let mut r = Renderer::new();
    r.begin(tam, escala, FUNDO);
    cena(&mut r);
    let cpu = r.pixmap.data().to_vec();
    let gpu = g.capture(&mut r, tam, escala, FUNDO, &cena).unwrap_or_else(|m| panic!("{nome}: caiu na CPU ({m})"));
    assert_eq!(cpu.len(), gpu.len());
    let (mut ruins, mut pior) = (0usize, 0u8);
    for (a, b) in cpu.chunks(4).zip(gpu.chunks(4)) {
        let d = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        pior = pior.max(d);
        ruins += (d > tol) as usize;
    }
    let n = cpu.len() / 4;
    eprintln!("{nome}: {ruins} de {n} pixels passam de {tol} (pior {pior})");
    assert!((ruins as f64) <= fracao * n as f64, "{nome}: {ruins} de {n} pixels diferem mais que {tol} (pior {pior})");
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
        let (mut cpu, mut gpu) = (Vec::new(), Vec::new());
        for _ in 0..30 {
            let t = std::time::Instant::now();
            r.begin(tam, escala, tema.background);
            widget::draw(&el, &n, &mut r, &st, &tema, None, false);
            cpu.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = std::time::Instant::now();
            g.capture(&mut r, tam, escala, tema.background, |r| {
                widget::draw(&el, &n, r, &st, &tema, None, false);
            })
            .expect("tabela inteira na GPU");
            gpu.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let med = |v: &mut Vec<f64>| { v.sort_by(f64::total_cmp); v[v.len() / 2] };
        eprintln!("escala {escala}: CPU {:.2} ms · GPU (com leitura de volta) {:.2} ms", med(&mut cpu), med(&mut gpu));
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
