//! Caminhos, transformações e texto pelos contornos das letras.
use pelicano::*;

fn alfa(r: &Renderer, x: u32, y: u32) -> u8 {
    r.pixmap.pixel(x, y).expect("dentro").alpha()
}

#[test]
fn circulo_preenchido_e_contorno() {
    let mut r = Renderer::new();
    r.begin(Size::new(100.0, 100.0), 1.0, Color::TRANSPARENT);
    r.fill_path(&Path::circle(Point::new(50.0, 50.0), 20.0).unwrap(), Color::WHITE, FillRule::NonZero);
    assert_eq!(alfa(&r, 50, 50), 255);
    assert_eq!(alfa(&r, 50, 75), 0);
    r.stroke_path(&Path::line(Point::new(0.0, 90.0), Point::new(100.0, 90.0)), &Stroke::new(4.0), Color::WHITE);
    assert_eq!(alfa(&r, 10, 90), 255);
    assert_eq!(alfa(&r, 10, 95), 0);
}

#[test]
fn arco_de_meia_volta_passa_embaixo() {
    let mut r = Renderer::new();
    r.begin(Size::new(100.0, 100.0), 1.0, Color::TRANSPARENT);
    let p = Path::new(|b| {
        b.arc(Point::new(50.0, 50.0), Point::new(30.0, 30.0), 0.0, std::f32::consts::PI);
    })
    .unwrap();
    r.stroke_path(&p, &Stroke::new(4.0), Color::WHITE);
    assert_eq!(alfa(&r, 50, 80), 255, "sentido horário na tela: de 0 a π passa por baixo");
    assert_eq!(alfa(&r, 50, 20), 0);
}

#[test]
fn rotacao_em_volta_do_centro() {
    let mut r = Renderer::new();
    r.begin(Size::new(100.0, 100.0), 2.0, Color::TRANSPARENT);
    // Barra horizontal de 60x6 no meio; girada 90°, vira vertical.
    r.push_transform(Transform::rotate_around(90.0, Point::new(50.0, 50.0)));
    r.fill_path(&Path::rect(Rect { x: 20.0, y: 47.0, width: 60.0, height: 6.0 }).unwrap(), Color::WHITE, FillRule::NonZero);
    r.pop_transform();
    assert_eq!(alfa(&r, 100, 50), 255, "em cima do centro (pixels físicos, escala 2)");
    assert_eq!(alfa(&r, 50, 100), 0, "à esquerda não tem mais nada");
}

#[test]
fn texto_pelos_contornos_fica_onde_o_comum_fica() {
    let mut r = Renderer::new();
    r.begin(Size::new(200.0, 60.0), 1.0, Color::TRANSPARENT);
    let p = r.text_path("Hi", 40.0, Font::BOLD, Point::new(10.0, 5.0), None).expect("letras");
    let caixa = p.bounds();
    let m = r.measure("Hi", 40.0, Font::BOLD, None);
    assert!(caixa.y > 5.0 && caixa.y < 5.0 + m.height / 2.0, "o topo do H perto do topo da linha: {caixa:?}");
    assert!(caixa.y + caixa.height <= 5.0 + m.height, "e a base dentro da linha: {caixa:?} {m:?}");
    r.fill_text("Hi", 40.0, Font::BOLD, Color::WHITE, Point::new(10.0, 5.0));
    assert_eq!(alfa(&r, (caixa.x + 3.0) as u32, (caixa.y + caixa.height / 2.0) as u32), 255, "a perna do H");
}

#[test]
fn caminho_no_quadro_parcial_bate_com_o_cheio() {
    let cena = |r: &mut Renderer, ang: f32| {
        r.fill_quad(Rect { x: 0.0, y: 0.0, width: 200.0, height: 200.0 }, Some(&Color::rgb8(20, 30, 60).into()), Border::default(), None);
        r.push_transform(Transform::rotate_around(ang, Point::new(100.0, 100.0)));
        let estrela = Path::polyline(&[Point::new(100.0, 40.0), Point::new(120.0, 160.0), Point::new(40.0, 80.0), Point::new(160.0, 80.0), Point::new(80.0, 160.0)], true).unwrap();
        r.fill_path(&estrela, Background::Linear { angle: 0.5, stops: vec![(0.0, Color::WHITE), (1.0, Color::rgb8(255, 0, 0))] }, FillRule::EvenOdd);
        r.stroke_path(&estrela, &Stroke::new(3.0).join(LineJoin::Round).dash(vec![6.0, 4.0]), Color::BLACK);
        r.pop_transform();
    };
    let tela = Size::new(200.0, 200.0);
    let mut r = Renderer::new();
    for (i, ang) in [0.0, 10.0].into_iter().enumerate() {
        r.record();
        cena(&mut r, ang);
        match r.changes() {
            Changes::Regions(rs) => {
                assert!(i > 0);
                for x in rs {
                    assert!(r.begin_parcial(tela, 1.0, Color::BLACK, x));
                    cena(&mut r, ang);
                    r.end();
                }
            }
            _ => {
                r.begin(tela, 1.0, Color::BLACK);
                cena(&mut r, ang);
            }
        }
    }
    let mut cheio = Renderer::new();
    cheio.begin(tela, 1.0, Color::BLACK);
    cena(&mut cheio, 10.0);
    let pior = cheio.pixmap.data().iter().zip(r.pixmap.data()).map(|(a, b)| a.abs_diff(*b)).max();
    assert_eq!(pior, Some(0));
}

#[test]
fn imagem_girada_e_sem_suavizar() {
    // 2x1: esquerda branca, direita transparente.
    let img = Image::from_rgba(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 0]).unwrap();
    let mut r = Renderer::new();
    r.begin(Size::new(100.0, 100.0), 1.0, Color::TRANSPARENT);
    r.draw_image_nearest(&img, Rect { x: 0.0, y: 0.0, width: 100.0, height: 50.0 });
    assert_eq!(alfa(&r, 49, 25), 255, "sem suavizar: a borda é seca");
    assert_eq!(alfa(&r, 51, 25), 0);
    r.begin(Size::new(100.0, 100.0), 1.0, Color::TRANSPARENT);
    r.push_transform(Transform::rotate_around(90.0, Point::new(50.0, 50.0)));
    r.draw_image(&img, Rect { x: 0.0, y: 25.0, width: 100.0, height: 50.0 }, [0.0; 4]);
    r.pop_transform();
    assert!(alfa(&r, 50, 10) > 200, "girada 90°: a metade branca vai para cima");
    assert!(alfa(&r, 50, 90) < 50);
}

/// Arrasta um quadrado; solta e avisa onde ficou.
struct Arrastar;

#[derive(Default)]
struct Pos {
    canto: Point,
    pega: Option<Point>,
}

impl Program<Point> for Arrastar {
    type State = Pos;
    fn draw(&self, st: &Pos, r: &mut Renderer, b: Rect, _: &Theme, _: Option<Point>) {
        r.fill_path(&Path::rect(Rect { x: b.x + st.canto.x, y: b.y + st.canto.y, width: 20.0, height: 20.0 }).unwrap(), Color::WHITE, FillRule::NonZero);
    }
    fn update(&self, st: &mut Pos, ev: &Event, b: Rect, cursor: Option<Point>, publish: &mut dyn FnMut(Point)) -> Captured {
        let Some(p) = cursor else { return Captured::No };
        match ev {
            Event::MousePressed(MouseButton::Left) if (Rect { x: b.x + st.canto.x, y: b.y + st.canto.y, width: 20.0, height: 20.0 }).contains(p) => st.pega = Some(Point::new(p.x - st.canto.x, p.y - st.canto.y)),
            Event::CursorMoved(q) if st.pega.is_some() => {
                let g = st.pega.unwrap();
                st.canto = Point::new(q.x - g.x, q.y - g.y);
            }
            Event::MouseReleased(MouseButton::Left) if st.pega.take().is_some() => publish(st.canto),
            _ => return Captured::No,
        }
        Captured::Yes
    }
    fn mouse_interaction(&self, st: &Pos, _: Rect, _: Option<Point>) -> CursorIcon {
        if st.pega.is_some() { CursorIcon::Grabbing } else { CursorIcon::Grab }
    }
}

#[test]
fn program_arrasta_com_estado_proprio() {
    let el: Element<Point> = program(Arrastar).into();
    let (mut r, mut st, tela) = (Renderer::new(), States::default(), Size::new(100.0, 100.0));
    r.begin(tela, 1.0, Color::TRANSPARENT);
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    let mut msgs = Vec::new();
    for (p, ev) in [(Point::new(5.0, 5.0), Event::MousePressed(MouseButton::Left)), (Point::new(55.0, 45.0), Event::CursorMoved(Point::new(55.0, 45.0))), (Point::new(55.0, 45.0), Event::MouseReleased(MouseButton::Left))] {
        let mut area = MemoryClipboard::default();
        let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: Some(p), messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
        assert_eq!(dispatch(&el, &n, &ev, &mut cx), Captured::Yes);
    }
    assert_eq!(msgs, vec![Point::new(50.0, 40.0)]);
    let icone = draw(&el, &n, &mut r, &st, &Theme::dark(), Some(Point::new(60.0, 50.0)), false);
    assert_eq!(alfa(&r, 60, 50), 255, "desenhado onde parou");
    assert_eq!(icone, CursorIcon::Grab);
}

/// Pega o dedo que encosta: o `Program` recebe o toque mesmo sem mouse na janela.
struct Dedos;

impl Program<Point> for Dedos {
    type State = ();
    fn draw(&self, _: &(), _: &mut Renderer, _: Rect, _: &Theme, _: Option<Point>) {}
    fn update(&self, _: &mut (), ev: &Event, _: Rect, _: Option<Point>, publish: &mut dyn FnMut(Point)) -> Captured {
        match ev {
            Event::FingerPressed { position, .. } => {
                publish(*position);
                Captured::Yes
            }
            _ => Captured::No,
        }
    }
}

#[test]
fn program_recebe_o_dedo_e_o_botao_deixa_passar() {
    let el: Element<Point> = pelicano::column![button("ok").on_press(Point::ORIGIN), program(Dedos)].into();
    let (mut r, mut st, tela) = (Renderer::new(), States::default(), Size::new(100.0, 100.0));
    let n = layout(&el, tela, &mut r, &st, &Theme::dark());
    let (mut msgs, mut area) = (Vec::new(), MemoryClipboard::default());
    let mut cx = EventCx { renderer: &mut r, states: &mut st, cursor: None, messages: &mut msgs, window: &mut Vec::new(), redraw: false, clipboard: &mut area };
    let toque = Event::FingerPressed { id: 1, position: Point::new(30.0, 80.0) };
    assert_eq!(dispatch(&el, &n, &toque, &mut cx), Captured::Yes);
    assert_eq!(dispatch(&el, &n, &Event::FingerLifted { id: 1, position: Point::new(30.0, 80.0) }, &mut cx), Captured::No);
    assert_eq!(msgs, vec![Point::new(30.0, 80.0)]);
}

#[test]
fn segmentos_do_caminho() {
    use pelicano::{Path, Point, Segment};
    let p = Path::new(|b| {
        b.move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).quadratic_to(Point::new(10.0, 10.0), Point::new(0.0, 10.0)).close();
    })
    .unwrap();
    let s: Vec<Segment> = p.segments().collect();
    assert_eq!(s, vec![Segment::MoveTo(Point::new(0.0, 0.0)), Segment::LineTo(Point::new(10.0, 0.0)), Segment::QuadTo(Point::new(10.0, 10.0), Point::new(0.0, 10.0)), Segment::Close]);
}

#[test]
fn imagem_svg_e_camada_enormes_pintam_so_a_parte_visivel() {
    // 2x2: vermelho, verde / azul, branco. Esticado a 100000x100000 lógicos na escala 2.
    let img = Image::from_rgba(2, 2, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]).unwrap();
    let svg = Svg::from_bytes(br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="1" height="1" fill="#f00"/><rect x="1" y="1" width="1" height="1" fill="#fff"/></svg>"##).unwrap();
    let tela = Size::new(200.0, 200.0);
    let cor = |r: &Renderer, x: u32, y: u32| {
        let p = r.pixmap.pixel(x, y).unwrap();
        [p.red(), p.green(), p.blue()]
    };
    // O canto de cima à esquerda em (50, 50) e o de baixo à direita em (150, 150).
    let comeco = Rect { x: 50.0, y: 50.0, width: 100000.0, height: 100000.0 };
    let fim = Rect { x: 150.0 - 100000.0, y: 150.0 - 100000.0, width: 100000.0, height: 100000.0 };
    let mut r = Renderer::new();
    r.begin(tela, 2.0, Color::BLACK);
    r.draw_image(&img, comeco, [0.0; 4]);
    assert_eq!((cor(&r, 20, 20), cor(&r, 200, 200)), ([0, 0, 0], [255, 0, 0]));
    r.begin(tela, 2.0, Color::BLACK);
    r.draw_image(&img, fim, [0.0; 4]);
    assert_eq!((cor(&r, 200, 200), cor(&r, 360, 360)), ([255, 255, 255], [0, 0, 0]));
    r.begin(tela, 2.0, Color::BLACK);
    r.draw_svg(&svg, comeco, None);
    assert_eq!((cor(&r, 20, 20), cor(&r, 200, 200)), ([0, 0, 0], [255, 0, 0]));
    r.begin(tela, 2.0, Color::BLACK);
    r.draw_svg(&svg, fim, None);
    assert_eq!((cor(&r, 200, 200), cor(&r, 360, 360)), ([255, 255, 255], [0, 0, 0]));
    r.begin(tela, 2.0, Color::BLACK);
    r.push_layer(fim, 1.0, 0.0, 1.0);
    r.fill_quad(Rect { x: 90.0, y: 90.0, width: 20.0, height: 20.0 }, Some(&Color::WHITE.into()), Border::default(), None);
    r.pop_layer();
    assert_eq!((cor(&r, 200, 200), cor(&r, 170, 170)), ([255, 255, 255], [0, 0, 0]));
}

#[test]
fn imagem_sem_suavizar_muda_com_a_transformacao() {
    let img = Image::from_rgba(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 0]).unwrap();
    let mut r = Renderer::new();
    for (i, zoom) in [1.0, 2.0].into_iter().enumerate() {
        r.record();
        r.push_transform(Transform::scale(zoom, zoom));
        r.draw_image_nearest(&img, Rect { x: 10.0, y: 10.0, width: 20.0, height: 10.0 });
        r.pop_transform();
        let c = r.changes();
        assert!(i == 0 || c != Changes::None, "o zoom mudou: {c:?}");
    }
}

#[test]
fn quina_do_traco_fica_dentro_da_area_gravada() {
    // Triângulo de 30° no topo: a junção em quina (20 px) sobe ~39 px acima do vértice.
    let tri = Path::polyline(&[Point::new(100.0, 60.0), Point::new(140.2, 210.0), Point::new(59.8, 210.0)], true).unwrap();
    let traco = Stroke::new(20.0);
    let tela = Size::new(200.0, 240.0);
    let mut r = Renderer::new();
    r.begin(tela, 1.0, Color::TRANSPARENT);
    r.stroke_path(&tri, &traco, Color::WHITE);
    let topo = (0..240).find(|y| r.pixmap.pixel(100, *y).unwrap().alpha() > 0).unwrap();
    assert!(topo < 30, "a ponta passa de 1,5× a largura: {topo}");
    let ponto = |r: &mut Renderer| r.fill_quad(Rect { x: 0.0, y: 0.0, width: 2.0, height: 2.0 }, Some(&Color::WHITE.into()), Border::default(), None);
    r.record();
    ponto(&mut r);
    r.changes();
    r.record();
    ponto(&mut r);
    r.stroke_path(&tri, &traco, Color::WHITE);
    let Changes::Regions(rs) = r.changes() else { panic!("só o traço mudou") };
    assert!(rs.iter().any(|a| a.y <= topo as f32 && a.contains(Point::new(100.0, topo as f32 + 0.5))), "{rs:?} não cobre a ponta em y={topo}");
}
