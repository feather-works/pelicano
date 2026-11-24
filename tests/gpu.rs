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

/// Imagem nítida (pixel art) esticada em blocos: a mesma na CPU, nas faixas e na GPU, e sem cair na CPU.
#[test]
fn imagem_nitida() {
    let px: Vec<u8> = (0..16 * 9).flat_map(|i| [(i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 13 % 256) as u8, 255]).collect();
    let img = Image::from_rgba(16, 9, px).expect("tamanho confere");
    comparar("imagem nítida", Size::new(160.0, 120.0), 1.0, 2, 0.0, |r| {
        r.draw_image_nearest(&img, Rect { x: 0.0, y: 0.0, width: 64.0, height: 36.0 });
        r.draw_image_nearest(&img, Rect { x: 70.0, y: 40.0, width: 80.0, height: 45.0 });
    });
}

/// Imagem girada e escalada (o chão de um mapa que gira): vai para a GPU como um quadrilátero, nítida e suavizada.
#[test]
fn imagem_transformada() {
    let px: Vec<u8> = (0..16 * 9).flat_map(|i| [(i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 13 % 256) as u8, 255]).collect();
    let img = Image::from_rgba(16, 9, px).expect("tamanho confere");
    comparar("imagem transformada", Size::new(160.0, 120.0), 1.5, 8, 0.02, |r| {
        r.push_transform(Transform::from_row(1.2, 0.3, -0.5, 0.8, 0.0, 0.0).then(Transform::rotate_around(25.0, Point::new(60.0, 50.0))));
        r.draw_image_nearest(&img, Rect { x: 20.0, y: 10.0, width: 64.0, height: 36.0 });
        r.pop_transform();
        r.push_transform(Transform::rotate_around(-40.0, Point::new(120.0, 80.0)));
        r.draw_image(&img, Rect { x: 90.0, y: 60.0, width: 48.0, height: 27.0 }, [0.0; 4]);
        r.pop_transform();
    });
}

/// Pedaços de um atlas, cada um girado e achatado (as fatias de um objeto empilhado): só o pedaço
/// aparece (sem vazar os vizinhos do atlas), igual nas três pinturas. O lote único: `gpu::testes`.
#[test]
fn pedacos_de_imagem() {
    // 4 pedaços de 8×8 lado a lado, cada um de uma cor com um furo transparente no meio
    let px: Vec<u8> = (0..32 * 8)
        .flat_map(|i| {
            let (x, y) = (i % 32, i / 32);
            let furo = (3..5).contains(&(x % 8)) && (3..5).contains(&y);
            let cor = [[220, 40, 40], [40, 200, 60], [50, 80, 230], [240, 200, 30]][x / 8];
            if furo { [0; 4] } else { [cor[0], cor[1], cor[2], 255] }
        })
        .collect();
    let img = Image::from_rgba(32, 8, px).expect("tamanho confere");
    let pecas: Vec<(Rect, Transform)> = (0..4)
        .map(|i| {
            let src = Rect { x: i as f32 * 8.0, y: 0.0, width: 8.0, height: 8.0 };
            let t = Transform::rotate_around(20.0 + i as f32 * 25.0, Point::new(4.0, 4.0)).then(Transform::scale(5.0, 3.0)).then(Transform::translate(20.0 + i as f32 * 30.0, 50.0 - i as f32 * 6.0));
            (src, t)
        })
        .collect();
    comparar("pedaços de imagem", Size::new(160.0, 120.0), 1.5, 8, 0.02, |r| {
        r.push_transform(Transform::rotate_around(10.0, Point::new(80.0, 60.0)));
        r.draw_image_pieces_nearest(&img, &pecas);
        r.pop_transform();
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

/// A imagem 1:1 (o atalho das faixas) sai igual ao caminho geral, também fora do pixel inteiro.
#[test]
fn imagem_um_para_um_igual_ao_geral() {
    let px: Vec<u8> = (0..64 * 48).flat_map(|i| [(i * 7 % 256) as u8, (i * 13 % 256) as u8, (i / 64 * 5) as u8, if i % 97 == 0 { 128 } else { 255 }]).collect();
    let img = Image::from_rgba(64, 48, px).expect("tamanho confere");
    let tam = Size::new(120.0, 90.0);
    let mut r = Renderer::new();
    for (x, y) in [(10.0, 10.0), (10.3, 7.6), (-5.5, 20.25)] {
        let rect = Rect { x, y, width: 64.0, height: 48.0 };
        let sombra = Some(Shadow { color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.4 }, offset: Point::new(0.0, 2.0), blur: 6.0 });
        let rapido = r.capture_bands(tam, 1.0, FUNDO, |r| { r.fill_quad(rect, Some(&Background::Color(Color::WHITE)), Border::default(), sombra); r.draw_image(&img, rect, [0.0; 4]) }).expect("na lista");
        let geral = r.capture_bands(tam, 1.0, FUNDO, |r| { r.fill_quad(rect, Some(&Background::Color(Color::WHITE)), Border { radius: [0.001; 4], ..Border::default() }, sombra); r.draw_image(&img, rect, [0.001; 4]) }).expect("na lista");
        // Os cantos mudam com o raio de 0,001 que força o caminho geral: até 8 pixels.
        diferenca(&format!("imagem 1:1 em {x},{y}"), &geral, &rapido, 1, 8.0 / (120.0 * 90.0));
    }
}

/// Objetos empilhados (`draw_stacks`): uma bola com orelhas e uma caixa, girados, inclinados, com
/// contorno, dentro de um recorte e de uma transformação; igual nas três pinturas.
#[test]
fn pilhas() {
    let paleta = [[[58, 38, 42]; 3], [[250, 190, 120], [230, 150, 60], [170, 100, 40]], [[240, 240, 230], [210, 210, 200], [150, 150, 140]]];
    let (lado, fatias) = (16, 24);
    let bola = StackAtlas::new(lado, fatias, 0.5, 1, &paleta, |x, y, z| {
        let (x, y, z) = (x as f32 + 0.5 - 8.0, y as f32 + 0.5 - 8.0, (z as f32 + 0.5) * 0.5 - 6.0);
        let orelha = z > 3.0 && z < 6.0 && (x.abs() - 3.5).abs() < 6.0 - z && y.abs() < 1.5;
        if x * x + y * y + z * z <= 36.0 || orelha {
            (if y > 3.0 { 2 } else { 1 }, [x, y, z])
        } else {
            (0, [0.0; 3])
        }
    });
    let caixa = StackAtlas::new(lado, fatias, 0.5, 1, &paleta, |x, y, z| ((x > 3 && x < 12 && y > 5 && y < 11 && z < 16) as u8 * 2, [0.0, 0.0, 1.0]));
    let cena = |r: &mut Renderer| {
        r.push_clip(Rect { x: 4.0, y: 4.0, width: 150.0, height: 110.0 }, [0.0; 4]);
        r.push_transform(Transform::translate(3.0, 2.0));
        for (i, giro) in [0.0f32, 1.3, 2.6, 4.2].into_iter().enumerate() {
            let st = Stack { at: Point::new(25.0 + i as f32 * 35.0, 70.0), yaw: giro, pitch: 0.9, scale: 1.7, sway: (1.0, 0.0), lift: i as f32, outline_step: 4, light: [-0.51, 0.5, 0.7], tones: Stack::TONES, tint: Stack::NO_TINT, palette: 0 };
            r.draw_stacks(&caixa, &[Stack { at: Point::new(st.at.x + 8.0, 40.0), outline_step: 0, ..st }]);
            r.draw_stacks(&bola, &[st]);
        }
        r.pop_transform();
        r.pop_clip();
    };
    comparar("pilhas", Size::new(160.0, 120.0), 1.5, 8, 0.02, cena);
    // e desenhou mesmo: os tons de luz, base e sombra do laranja e o contorno aparecem
    let px = Renderer::new().capture_bands(Size::new(160.0, 120.0), 1.5, FUNDO, cena).expect("na lista");
    for cor in [[250, 190, 120], [230, 150, 60], [170, 100, 40], [58, 38, 42]] {
        let n = px.chunks(4).filter(|p| p[..3] == cor).count();
        assert!(n > 20, "{cor:?}: só {n} pixels");
    }
}

/// A pilha com `palette` pinta com a outra paleta do mesmo atlas (contorno e material), na GPU igual à CPU.
#[test]
fn pilha_troca_de_paleta() {
    let paleta = [[[58, 38, 42]; 3], [[250, 190, 120], [230, 150, 60], [170, 100, 40]], [[255, 255, 255]; 3], [[120, 190, 250], [60, 150, 230], [40, 100, 170]]];
    let bola = StackAtlas::new(16, 24, 0.5, 1, &paleta, |x, y, z| {
        let (x, y, z) = (x as f32 + 0.5 - 8.0, y as f32 + 0.5 - 8.0, (z as f32 + 0.5) * 0.5 - 6.0);
        (if x * x + y * y + z * z <= 36.0 { 1 } else { 0 }, [x, y, z])
    });
    let cena = |r: &mut Renderer| {
        for (i, palette) in [0, 2].into_iter().enumerate() {
            let st = Stack { at: Point::new(40.0 + i as f32 * 60.0, 80.0), yaw: 0.7, pitch: 0.9, scale: 2.0, sway: (0.0, 0.0), lift: 0.0, outline_step: 4, light: [-0.51, 0.5, 0.7], tones: Stack::TONES, tint: Stack::NO_TINT, palette };
            r.draw_stacks(&bola, &[st]);
        }
    };
    comparar("pilha_paleta", Size::new(160.0, 120.0), 1.5, 8, 0.02, cena);
    let px = Renderer::new().capture_bands(Size::new(160.0, 120.0), 1.5, FUNDO, cena).expect("na lista");
    let conta = |cor: [u8; 3], x0: usize, x1: usize| px.chunks(4).enumerate().filter(|(i, p)| (x0..x1).contains(&(i % 240)) && p[..3] == cor).count();
    // a da esquerda com a paleta 0, a da direita com a 2 (azul e contorno branco)
    for (cor, x0, x1) in [([230, 150, 60], 0, 120), ([58, 38, 42], 0, 120), ([60, 150, 230], 120, 240), ([255, 255, 255], 120, 240)] {
        assert!(conta(cor, x0, x1) > 20, "{cor:?} em {x0}..{x1}: só {}", conta(cor, x0, x1));
    }
    assert_eq!(conta([230, 150, 60], 120, 240), 0, "a da direita não usa a paleta 0");
}

/// Uma versão de outra imagem ([`Image::changed_from`]) sobe só o que mudou na mesma textura: sai igual à CPU, e o
/// que mudou fora dos retângulos declarados fica como estava (prova que a textura foi reaproveitada). Pulando uma
/// versão (não desenhada), os retângulos das duas sobem.
#[test]
fn imagem_em_versoes() {
    fn desenhar(img: &Image) -> impl Fn(&mut Renderer) + '_ {
        move |r| r.draw_image_nearest(img, Rect { x: 0.0, y: 0.0, width: 64.0, height: 48.0 })
    }
    let Some(mut g) = GpuOffscreen::new() else {
        eprintln!("sem adaptador gráfico, pulando");
        return;
    };
    let tam = Size::new(64.0, 48.0);
    let (w, h) = (64u32, 48u32);
    let cor = |px: &mut Vec<u8>, [x0, y0, x1, y1]: [u32; 4], c: [u8; 4]| {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&c);
            }
        }
    };
    let mut px: Vec<u8> = (0..w * h).flat_map(|i| [(i % 64 * 4) as u8, (i / 64 * 5) as u8, 90, 255]).collect();
    let mut r = Renderer::new();
    let a = Image::from_rgba(w, h, px.clone()).unwrap();
    let ga = g.capture(&mut r, tam, 1.0, FUNDO, desenhar(&a)).unwrap();
    // b muda em dois lugares, mas só declara o primeiro
    cor(&mut px, [4, 4, 20, 12], [255, 0, 0, 255]);
    let so_declarado = px.clone();
    cor(&mut px, [40, 30, 60, 40], [0, 255, 0, 255]);
    let b = Image::from_rgba(w, h, px.clone()).unwrap().changed_from(&a, &[[4, 4, 16, 8]]);
    let gb = g.capture(&mut r, tam, 1.0, FUNDO, desenhar(&b)).unwrap();
    let esperado = Image::from_rgba(w, h, so_declarado).unwrap();
    let ge = g.capture(&mut r, tam, 1.0, FUNDO, desenhar(&esperado)).unwrap();
    assert_ne!(ga, gb, "o retângulo declarado subiu");
    assert_eq!(gb, ge, "só o retângulo declarado subiu (a textura de a foi reaproveitada)");
    // c vem de d, que vem de b e nunca foi desenhada: os retângulos de d e de c sobem na textura de b
    cor(&mut px, [0, 40, 10, 48], [0, 0, 255, 255]);
    let d = Image::from_rgba(w, h, px.clone()).unwrap().changed_from(&b, &[[40, 30, 20, 10]]);
    cor(&mut px, [30, 0, 34, 4], [255, 255, 0, 255]);
    let c = Image::from_rgba(w, h, px.clone()).unwrap().changed_from(&d, &[[0, 40, 10, 8], [30, 0, 4, 4]]);
    let gc = g.capture(&mut r, tam, 1.0, FUNDO, desenhar(&c)).unwrap();
    let faixas = r.capture_bands(tam, 1.0, FUNDO, desenhar(&c)).unwrap();
    diferenca("imagem em versões", &faixas, &gc, 0, 0.0);
    // em pedaços (os ladrilhos de um jogo) a versão também sobe só o que mudou
    cor(&mut px, [50, 2, 60, 8], [255, 0, 255, 255]);
    let f = Image::from_rgba(w, h, px.clone()).unwrap().changed_from(&c, &[[50, 2, 10, 6]]);
    let pecas = |img: &Image| {
        let img = img.clone();
        move |r: &mut Renderer| r.draw_image_pieces_nearest(&img, &[(Rect { x: 0.0, y: 0.0, width: 64.0, height: 48.0 }, Transform::IDENTITY)])
    };
    let gf = g.capture(&mut r, tam, 1.0, FUNDO, pecas(&f)).unwrap();
    let faixas = r.capture_bands(tam, 1.0, FUNDO, pecas(&f)).unwrap();
    diferenca("imagem em versões, em pedaços", &faixas, &gf, 0, 0.0);
}

/// As partes com profundidade (`draw_stack_parts_lined`): um corpo e um braço girado na frente dele, com as linhas por
/// dentro; o pipeline cria na placa e desenha igual à CPU.
#[test]
fn pilhas_fundas_com_linhas() {
    let paleta = [[[58, 38, 42]; 3], [[250, 190, 120], [230, 150, 60], [170, 100, 40]], [[240, 240, 230], [210, 210, 200], [150, 150, 140]]];
    let corpo = StackAtlas::new(16, 24, 0.5, 1, &paleta, |x, y, z| {
        let (x, y, z) = (x as f32 + 0.5 - 8.0, y as f32 + 0.5 - 8.0, (z as f32 + 0.5) * 0.5 - 6.0);
        ((x * x + y * y + z * z <= 36.0) as u8, [x, y, z])
    });
    let braco = StackAtlas::new(16, 24, 0.5, 1, &paleta, |x, y, z| ((x >= 10 && x < 13 && y >= 7 && y < 10 && z < 20) as u8 * 2, [0.0, 1.0, 0.3]));
    let cena = |r: &mut Renderer| {
        let partes: Vec<(&StackAtlas, StackPart)> = [0.0f32, 1.9]
            .into_iter()
            .enumerate()
            .flat_map(|(i, giro)| {
                let st = Stack { at: Point::new(45.0 + i as f32 * 70.0, 80.0), yaw: giro, pitch: 0.9, scale: 2.0, sway: (0.0, 0.0), lift: 0.0, outline_step: 4, light: [-0.51, 0.5, 0.7], tones: Stack::TONES, tint: Stack::NO_TINT, palette: 0 };
                [(&corpo, StackPart::still(st)), (&braco, StackPart { stack: st, rot: StackPart::rot_x(1.1), pivot: [3.5, 0.0, 9.0], offset: [0.0, 2.0, 0.0] })]
            })
            .collect();
        r.draw_stack_parts_lined(&partes, StackLines { color: [20, 10, 10], threshold: 2.0, width: 1.0 });
    };
    comparar("pilhas_fundas", Size::new(160.0, 120.0), 1.5, 8, 0.03, cena);
    // e as linhas por dentro aparecem
    let px = Renderer::new().capture_bands(Size::new(160.0, 120.0), 1.5, FUNDO, cena).expect("na lista");
    let n = px.chunks(4).filter(|p| p[..3] == [20, 10, 10]).count();
    assert!(n > 10, "só {n} pixels de linha");
}
