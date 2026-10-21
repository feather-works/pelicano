//! Teste de estresse: uma corda que segue o mouse, de 5 a 100 000 segmentos, para comparar FPS, CPU e RAM com a
//! mesma em Iced 0.13 (`cordas-iced`; o núcleo — física, obstáculos, curva automática e leitura — é o mesmo texto).
//! Teclas 1–7 (ou os botões) escolhem a corda, A liga o modo automático; arraste um ponto, e a roda gira o segmento preso.
//! `cargo run --release --example cordas`; `-- --auto 7` já abre no modo automático da corda 7 e escreve a leitura
//! a cada segundo no terminal; `-- --hz 120` acompanha um monitor de 120 Hz (padrão 60); `-- --medir` roda o modo automático sem janela em cada tamanho.
use nucleo::{lissajous, Corda, Obstaculo, Quadros, Recursos, V, ALTURA, ANGULO_PADRAO, GRAUS_POR_DENTE, LARGURA, OBSTACULOS, TAMANHOS};
use pelicano::column;
use pelicano::*;
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

// ---------- núcleo: igual, linha a linha, no cordas-iced ----------
// Seguir o líder: o ponto preso vai para o mouse e, dele para as duas pontas, cada ponto seguinte fica a um
// segmento do anterior, na direção em que já estava, com a dobra de cada junta limitada. O(N) por passo e
// estável para qualquer N (não há sistema a resolver, só uma passada). Vista de cima, sem gravidade.
mod nucleo {
    use std::ops::{Add, Mul, Sub};
    use std::time::Instant;

    pub const LARGURA: f32 = 1280.0;
    pub const ALTURA: f32 = 800.0;
    /// Toda corda tem o mesmo comprimento: só a resolução muda.
    pub const COMPRIMENTO: f32 = 1400.0;
    pub const TAMANHOS: [usize; 7] = [5, 10, 50, 500, 5_000, 50_000, 100_000];
    pub const RAIO_PEGAR: f32 = 12.0;
    pub const ANGULO_PADRAO: f32 = 35.0;
    /// Quanto gira o segmento preso por dente da roda.
    pub const GRAUS_POR_DENTE: f32 = 5.0;
    /// Solta, a corda continua com esta fração da velocidade a cada passo.
    const AMORTECIMENTO: f32 = 0.85;
    /// Solta, a corda para quando nenhum ponto anda mais que isto (px) num passo.
    const PARADA: f32 = 0.02;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct V {
        pub x: f32,
        pub y: f32,
    }

    impl V {
        pub const fn new(x: f32, y: f32) -> V {
            V { x, y }
        }
        pub fn dot(self, o: V) -> f32 {
            self.x * o.x + self.y * o.y
        }
        pub fn cross(self, o: V) -> f32 {
            self.x * o.y - self.y * o.x
        }
        pub fn len(self) -> f32 {
            self.dot(self).sqrt()
        }
        pub fn unit(self) -> Option<V> {
            let l = self.len();
            (l > 1e-9).then(|| self * (1.0 / l))
        }
        /// Gira pelo ângulo de seno `s` e cosseno `c`.
        pub fn girar(self, s: f32, c: f32) -> V {
            V::new(self.x * c - self.y * s, self.x * s + self.y * c)
        }
    }

    impl Add for V {
        type Output = V;
        fn add(self, o: V) -> V {
            V::new(self.x + o.x, self.y + o.y)
        }
    }

    impl Sub for V {
        type Output = V;
        fn sub(self, o: V) -> V {
            V::new(self.x - o.x, self.y - o.y)
        }
    }

    impl Mul<f32> for V {
        type Output = V;
        fn mul(self, k: f32) -> V {
            V::new(self.x * k, self.y * k)
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Obstaculo {
        Circulo { c: V, r: f32 },
        Caixa { x0: f32, y0: f32, x1: f32, y1: f32 },
    }

    pub const OBSTACULOS: [Obstaculo; 9] = [
        Obstaculo::Circulo { c: V::new(230.0, 140.0), r: 60.0 },
        Obstaculo::Circulo { c: V::new(660.0, 230.0), r: 45.0 },
        Obstaculo::Circulo { c: V::new(1100.0, 150.0), r: 70.0 },
        Obstaculo::Circulo { c: V::new(330.0, 690.0), r: 55.0 },
        Obstaculo::Circulo { c: V::new(960.0, 690.0), r: 65.0 },
        Obstaculo::Circulo { c: V::new(140.0, 480.0), r: 35.0 },
        Obstaculo::Caixa { x0: 470.0, y0: 90.0, x1: 600.0, y1: 160.0 },
        Obstaculo::Caixa { x0: 590.0, y0: 600.0, x1: 720.0, y1: 740.0 },
        Obstaculo::Caixa { x0: 1150.0, y0: 420.0, x1: 1230.0, y1: 600.0 },
    ];

    impl Obstaculo {
        /// `p` posto na borda mais perto, se estiver dentro.
        fn empurrar(&self, p: V) -> V {
            match *self {
                Obstaculo::Circulo { c, r } => {
                    let d = p - c;
                    let l2 = d.dot(d);
                    if l2 >= r * r {
                        return p;
                    }
                    let l = l2.sqrt();
                    if l < 1e-6 {
                        return c + V::new(r, 0.0);
                    }
                    c + d * (r / l)
                }
                Obstaculo::Caixa { x0, y0, x1, y1 } => {
                    if p.x <= x0 || p.x >= x1 || p.y <= y0 || p.y >= y1 {
                        return p;
                    }
                    let (e, d, c, b) = (p.x - x0, x1 - p.x, p.y - y0, y1 - p.y);
                    let m = e.min(d).min(c).min(b);
                    if m == e {
                        V::new(x0, p.y)
                    } else if m == d {
                        V::new(x1, p.y)
                    } else if m == c {
                        V::new(p.x, y0)
                    } else {
                        V::new(p.x, y1)
                    }
                }
            }
        }

        /// Dentro de verdade (com folga de 1e-3 px para o arredondamento da borda).
        #[cfg(test)]
        pub fn dentro(&self, p: V) -> bool {
            const F: f32 = 1e-3;
            match *self {
                Obstaculo::Circulo { c, r } => (p - c).len() < r - F,
                Obstaculo::Caixa { x0, y0, x1, y1 } => p.x > x0 + F && p.x < x1 - F && p.y > y0 + F && p.y < y1 - F,
            }
        }
    }

    fn fora(p: V, obs: &[Obstaculo]) -> V {
        obs.iter().fold(p, |p, o| o.empurrar(p))
    }

    /// O ponto a `seg` de `a` na direção `d`, fora dos obstáculos (`a` já está fora). Caiu dentro: vai para a
    /// borda e volta para a distância certa; numa quina isso não basta, e a direção gira aos poucos até sair.
    fn colocar(a: V, d: V, seg: f32, obs: &[Obstaculo]) -> V {
        let q = a + d * seg;
        let f = fora(q, obs);
        if f == q {
            return q;
        }
        let q = a + (f - a).unit().unwrap_or(d) * seg;
        let f = fora(q, obs);
        // Deslizando na borda de um círculo a volta cai um fio para dentro: o empurrão quase não muda o comprimento.
        if ((f - a).len() - seg).abs() <= 0.01 * seg {
            return f;
        }
        for k in 1..=180 {
            let (s, c) = (k as f32).to_radians().sin_cos();
            for s in [s, -s] {
                let q = a + d.girar(s, c) * seg;
                if fora(q, obs) == q {
                    return q;
                }
            }
        }
        f
    }

    /// `quer`, se a dobra em relação a `dir` couber no limite (cosseno, seno); senão, `dir` girado do limite.
    fn limitar(dir: V, quer: V, (c, s): (f32, f32)) -> V {
        if dir.dot(quer) >= c {
            return quer;
        }
        dir.girar(if dir.cross(quer) >= 0.0 { s } else { -s }, c)
    }

    /// De `de` até a ponta (para a frente ou para trás): cada ponto vai para `seg` do anterior, na direção em
    /// que já estava, dobrando no máximo o limite em relação ao segmento anterior (`dir`).
    fn seguir(p: &mut [V], de: usize, frente: bool, mut dir: V, seg: f32, lim: (f32, f32), obs: &[Obstaculo]) {
        let mut i = de;
        loop {
            // Para trás, 0 - 1 dá a volta e cai fora da faixa.
            let j = if frente { i + 1 } else { i.wrapping_sub(1) };
            if j >= p.len() {
                break;
            }
            let d = limitar(dir, (p[j] - p[i]).unit().unwrap_or(dir), lim);
            let q = colocar(p[i], d, seg, obs);
            dir = (q - p[i]).unit().unwrap_or(d);
            p[j] = q;
            i = j;
        }
    }

    pub struct Corda {
        /// N segmentos: N + 1 pontos.
        pub p: Vec<V>,
        antes: Vec<V>,
        pub seg: f32,
        /// O ponto preso (mouse ou modo automático) e para onde ele vai neste passo.
        pub preso: Option<usize>,
        pub alvo: V,
        /// Quanto girar o segmento preso no próximo passo (radianos, da roda).
        pub giro: f32,
        /// O último ponto preso: solta, a corda segue por inércia a partir dele.
        ancora: usize,
        /// Ainda anda (presa, ou solta e sem ter parado).
        pub movendo: bool,
    }

    impl Corda {
        /// Uma onda suave deitada no meio da janela.
        pub fn new(n: usize) -> Corda {
            let seg = COMPRIMENTO / n as f32;
            let mut p = Vec::with_capacity(n + 1);
            let mut a = V::new(105.0, 310.0);
            for i in 0..=n {
                p.push(a);
                let s = (i as f32 + 0.5) * seg / COMPRIMENTO;
                let th = 0.9 * (s * std::f32::consts::TAU * 2.0).sin();
                a = a + V::new(th.cos(), th.sin()) * seg;
            }
            Corda { antes: p.clone(), p, seg, preso: None, alvo: V::default(), giro: 0.0, ancora: 0, movendo: false }
        }

        pub fn n(&self) -> usize {
            self.p.len() - 1
        }

        /// O ponto mais perto de `m`, até `RAIO_PEGAR`. Varre tudo (só no clique): 100 000 pontos são ~0,1 ms.
        pub fn mais_proximo(&self, m: V) -> Option<usize> {
            let (mut melhor, mut d2) = (None, RAIO_PEGAR * RAIO_PEGAR);
            for (i, q) in self.p.iter().enumerate() {
                let d = *q - m;
                let e = d.dot(d);
                if e <= d2 {
                    (melhor, d2) = (Some(i), e);
                }
            }
            melhor
        }

        /// Um passo: presa, o ponto preso vai ao alvo; solta, cada ponto anda com a velocidade que tinha,
        /// amortecida. Depois a passada de seguir o líder, a partir do ponto preso (ou do último preso).
        pub fn passo(&mut self, limite_graus: f32, obs: &[Obstaculo]) {
            let lim = limite_graus.clamp(0.0, 180.0).to_radians();
            let lim = (lim.cos(), lim.sin());
            let alvo = match self.preso {
                Some(k) => {
                    self.ancora = k;
                    self.antes.copy_from_slice(&self.p);
                    self.alvo
                }
                None => {
                    for (q, a) in self.p.iter_mut().zip(self.antes.iter_mut()) {
                        let v = (*q - *a) * AMORTECIMENTO;
                        *a = *q;
                        *q = *q + v;
                    }
                    self.p[self.ancora]
                }
            };
            let (k, n, seg) = (self.ancora, self.p.len(), self.seg);
            let p = &mut self.p[..];
            p[k] = fora(alvo, obs);
            let (s, c) = std::mem::take(&mut self.giro).sin_cos();
            if k + 1 < n {
                let d = (p[k + 1] - p[k]).unit().unwrap_or(V::new(1.0, 0.0)).girar(s, c);
                p[k + 1] = colocar(p[k], d, seg, obs);
                let d = (p[k + 1] - p[k]).unit().unwrap_or(d);
                seguir(p, k + 1, true, d, seg, lim, obs);
                seguir(p, k, false, d * -1.0, seg, lim, obs);
            } else {
                let d = (p[k - 1] - p[k]).unit().unwrap_or(V::new(-1.0, 0.0)).girar(s, c);
                p[k - 1] = colocar(p[k], d, seg, obs);
                let d = (p[k - 1] - p[k]).unit().unwrap_or(d);
                seguir(p, k - 1, false, d, seg, lim, obs);
            }
            self.movendo = self.preso.is_some() || self.p.iter().zip(&self.antes).any(|(q, a)| (*q - *a).dot(*q - *a) > PARADA * PARADA);
        }
    }

    /// O modo automático: a cabeça (ponto 0) anda por esta curva, um passo por quadro. Depende só do número
    /// do passo, então os dois programas fazem exatamente o mesmo trabalho em cada quadro.
    pub fn lissajous(passo: u64) -> V {
        let t = passo as f32 * 0.004;
        V::new(LARGURA / 2.0 + 520.0 * (3.0 * t + 0.5).sin(), ALTURA / 2.0 + 300.0 * (2.0 * t).sin())
    }

    /// Memória (VmRSS e o pico, VmHWM) e CPU do processo, lidas do /proc a cada segundo. CPU em % de um núcleo (como no `top`).
    #[derive(Default)]
    pub struct Recursos {
        pub rss_mib: f32,
        pub pico_mib: f32,
        pub cpu: f32,
        antes: Option<(u64, Instant)>,
    }

    impl Recursos {
        pub fn amostrar(&mut self) {
            if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
                let kb = |chave: &str| s.lines().find_map(|l| l.strip_prefix(chave).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f32>().ok()));
                self.rss_mib = kb("VmRSS:").unwrap_or(0.0) / 1024.0;
                self.pico_mib = kb("VmHWM:").unwrap_or(0.0) / 1024.0;
            }
            // shortcut: CLK_TCK fixo em 100 (o do Linux comum), para não puxar a libc; outro valor erra a escala da CPU.
            let ticks = std::fs::read_to_string("/proc/self/stat").ok().and_then(|s| {
                let f: Vec<u64> = s[s.rfind(')')? + 2..].split(' ').map(|x| x.parse().unwrap_or(0)).collect();
                Some(f.get(11)? + f.get(12)?)
            });
            let agora = Instant::now();
            if let (Some(t), Some((t0, i0))) = (ticks, self.antes) {
                self.cpu = t.saturating_sub(t0) as f32 / 100.0 / (agora - i0).as_secs_f32().max(1e-3) * 100.0;
            }
            self.antes = ticks.map(|t| (t, agora));
        }
    }

    /// Os núcleos da máquina: o monitor do sistema divide a CPU por eles.
    pub fn nucleos() -> f32 {
        std::thread::available_parallelism().map_or(1, |n| n.get()) as f32
    }

    /// Os quadros apresentados: contados no desenho; a cada segundo viram o FPS e a média.
    #[derive(Default)]
    pub struct Quadros {
        pub fps: u32,
        pub media_ms: f32,
        pub ultimo_ms: f32,
        n: u32,
        soma_ms: f32,
        quando: Option<Instant>,
    }

    impl Quadros {
        pub fn contar(&mut self) {
            let agora = Instant::now();
            if let Some(a) = self.quando.replace(agora) {
                self.ultimo_ms = (agora - a).as_secs_f32() * 1000.0;
                self.soma_ms += self.ultimo_ms;
            }
            self.n += 1;
        }

        pub fn segundo(&mut self) {
            (self.fps, self.media_ms) = (self.n, if self.n > 0 { self.soma_ms / self.n as f32 } else { 0.0 });
            (self.n, self.soma_ms) = (0, 0.0);
        }
    }

    /// As linhas do canto de cima: a mesma leitura nos dois programas.
    pub fn leitura(q: &Quadros, fisica_ms: f32, n: usize, r: &Recursos) -> String {
        format!(
            "FPS {} · quadro {:.1} ms (média {:.1}) · física {fisica_ms:.2} ms · {n} segmentos\nRAM {:.0} MiB (pico {:.0}) · CPU {:.0}% de 1 núcleo ({:.1}% da máquina)",
            q.fps,
            q.ultimo_ms,
            q.media_ms,
            r.rss_mib,
            r.pico_mib,
            r.cpu,
            r.cpu / nucleos()
        )
    }

    #[cfg(test)]
    mod testes {
        use super::*;

        fn angulos_ok(p: &[V], lim_graus: f32) -> bool {
            let c = lim_graus.to_radians().cos() - 1e-3;
            p.windows(3).all(|w| match ((w[1] - w[0]).unit(), (w[2] - w[1]).unit()) {
                (Some(a), Some(b)) => a.dot(b) >= c,
                _ => true,
            })
        }

        fn comprimentos_ok(c: &Corda, tol: f32) -> bool {
            c.p.windows(2).all(|w| ((w[1] - w[0]).len() - c.seg).abs() <= tol * c.seg)
        }

        #[test]
        fn pose_inicial_cabe_e_esta_fora_dos_obstaculos() {
            for n in TAMANHOS {
                let c = Corda::new(n);
                assert!(c.p.iter().all(|q| q.x > 0.0 && q.x < LARGURA && q.y > 0.0 && q.y < ALTURA), "{n} sai da janela");
                assert!(c.p.iter().all(|q| OBSTACULOS.iter().all(|o| !o.dentro(*q))), "{n} começa num obstáculo");
            }
        }

        #[test]
        fn sem_obstaculos_comprimento_e_angulo() {
            for n in [5, 50, 5_000] {
                for lim in [5.0, 35.0, 120.0] {
                    let mut c = Corda::new(n);
                    c.preso = Some(n / 3);
                    for passo in 0..300 {
                        c.alvo = lissajous(passo);
                        c.giro = if passo % 50 == 0 { 0.3 } else { 0.0 };
                        c.passo(lim, &[]);
                        assert!(comprimentos_ok(&c, 1e-3), "n {n} lim {lim}: comprimento");
                        assert!(angulos_ok(&c.p, lim), "n {n} lim {lim}: ângulo");
                    }
                }
            }
        }

        #[test]
        fn com_obstaculos_ninguem_dentro_e_comprimento_quase() {
            for n in [10, 500, 50_000] {
                let mut c = Corda::new(n);
                c.preso = Some(0);
                for passo in 0..1600 {
                    c.alvo = lissajous(passo);
                    c.passo(ANGULO_PADRAO, &OBSTACULOS);
                    assert!(c.p.iter().all(|q| OBSTACULOS.iter().all(|o| !o.dentro(*q))), "n {n} passo {passo}: ponto dentro");
                    assert!(comprimentos_ok(&c, 0.01), "n {n} passo {passo}: comprimento");
                }
                // Solta, para em algum momento.
                c.preso = None;
                let mut parou = false;
                for _ in 0..2000 {
                    c.passo(ANGULO_PADRAO, &OBSTACULOS);
                    if !c.movendo {
                        parou = true;
                        break;
                    }
                }
                assert!(parou, "n {n}: não parou");
            }
        }

        #[test]
        fn pega_o_mais_perto() {
            let c = Corda::new(100_000);
            let m = c.p[77_777] + V::new(3.0, 4.0);
            let k = c.mais_proximo(m).unwrap();
            assert!((c.p[k] - m).len() <= 5.0 + 1e-3);
            assert_eq!(c.mais_proximo(V::new(-100.0, -100.0)), None);
        }
    }
}
// ---------- fim do núcleo ----------

// ---------- interface ----------

#[derive(Clone)]
enum Msg {
    Escolher(usize),
    Auto,
    Angulo(f32),
    Pegar(Point),
    Mover(Point),
    Soltar,
    /// Dentes da roda (positivo = para cima).
    Girar(f32),
    Quadro,
    Tique,
}

struct Cordas {
    i: usize,
    corda: Corda,
    angulo: f32,
    auto: bool,
    passo_auto: u64,
    segurando: bool,
    fisica_ms: f32,
    /// Segundos parados: o relógio da leitura ainda bate dois, para o FPS e a CPU caírem a zero na tela.
    ociosos: u32,
    rec: Recursos,
    /// `--auto`: a leitura também vai para o terminal a cada segundo.
    eco: bool,
    /// O relógio dos quadros (`--hz`).
    intervalo: Duration,
    // O desenho só tem `&self`: os contadores ficam em `Cell`.
    quadros: RefCell<Quadros>,
    /// Houve um quadro desde o último passo: um passo de física por quadro, como o `window::frames()` do Iced.
    desenhou: Cell<bool>,
}

impl Cordas {
    fn new() -> Self {
        let mut rec = Recursos::default();
        rec.amostrar();
        Cordas { i: 3, corda: Corda::new(TAMANHOS[3]), angulo: ANGULO_PADRAO, auto: false, passo_auto: 0, segurando: false, fisica_ms: 0.0, ociosos: 0, rec, eco: false, intervalo: Duration::from_millis(16), quadros: RefCell::default(), desenhou: Cell::new(true) }
    }

    fn animando(&self) -> bool {
        self.auto || self.segurando || self.corda.movendo
    }
}

fn ponto(v: V) -> Point {
    Point::new(v.x, v.y)
}

fn rotulo(n: usize) -> String {
    if n >= 1000 { format!("{}k", n / 1000) } else { n.to_string() }
}

struct Desenho<'a> {
    app: &'a Cordas,
    pts: Vec<Point>,
    chamadas: Cell<u8>,
}

impl Program<Msg> for Desenho<'_> {
    type State = ();

    fn draw(&self, _: &(), r: &mut Renderer, _: Rect, _: &Theme, _: Option<Point>) {
        // Cada quadro desenha duas vezes: grava (para comparar com o anterior) e pinta; sem nada novo só grava
        // e nem apresenta. Então o quadro apresentado conta na segunda chamada.
        let k = self.chamadas.get() + 1;
        self.chamadas.set(k);
        self.app.desenhou.set(true);
        if k == 2 {
            self.app.quadros.borrow_mut().contar();
        }
        // Círculos e caixas como quads (círculo = canto com raio cheio): cabem na GPU, um
        // `fill_path` mandaria o quadro inteiro para a CPU.
        let bola = |r: &mut Renderer, c: Point, raio: f32, cor: Color| {
            let caixa = Rect { x: c.x - raio, y: c.y - raio, width: 2.0 * raio, height: 2.0 * raio };
            r.fill_quad(caixa, Some(&Background::Color(cor)), Border { radius: [raio; 4], ..Default::default() }, None);
        };
        let obstaculo = Color::rgb8(0xc9, 0xd1, 0xd9);
        for o in OBSTACULOS {
            match o {
                Obstaculo::Circulo { c, r: raio } => bola(r, ponto(c), raio, obstaculo),
                Obstaculo::Caixa { x0, y0, x1, y1 } => r.fill_quad(Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }, Some(&Background::Color(obstaculo)), Border::default(), None),
            }
        }
        r.stroke(&self.pts, 6.0, Color::rgb8(0x25, 0x63, 0xeb));
        if self.app.corda.n() <= 500 {
            for p in &self.pts {
                bola(r, *p, 2.5, Color::rgb8(0x1e, 0x3a, 0x8a));
            }
        }
        if let Some(k) = self.app.corda.preso {
            let j = if k + 1 < self.pts.len() { k + 1 } else { k - 1 };
            let destaque = Color::rgb8(0xf9, 0x73, 0x16);
            r.stroke(&[self.pts[k], self.pts[j]], 9.0, destaque);
            bola(r, self.pts[k], 7.0, destaque);
        }
    }

    fn update(&self, _: &mut (), ev: &Event, b: Rect, cursor: Option<Point>, publish: &mut dyn FnMut(Msg)) -> Captured {
        // Só o clique passa por aqui (os botões por cima ficam com o deles); arrastar e soltar vêm do `on_event`.
        if let (Event::MousePressed(MouseButton::Left), Some(c)) = (ev, cursor.filter(|c| b.contains(*c))) {
            publish(Msg::Pegar(c));
            return Captured::Yes;
        }
        Captured::No
    }
}

impl App for Cordas {
    type Msg = Msg;

    fn theme(&self) -> Theme {
        Theme::light()
    }

    fn update(&mut self, m: Msg) -> Task<Msg> {
        match m {
            Msg::Escolher(i) => {
                (self.i, self.corda, self.segurando, self.passo_auto) = (i, Corda::new(TAMANHOS[i]), false, 0);
                if self.auto {
                    self.corda.preso = Some(0);
                }
            }
            Msg::Auto => {
                (self.auto, self.segurando, self.passo_auto) = (!self.auto, false, 0);
                self.corda.preso = self.auto.then_some(0);
            }
            Msg::Angulo(a) => self.angulo = a,
            Msg::Pegar(p) if !self.auto => {
                if let Some(k) = self.corda.mais_proximo(V::new(p.x, p.y)) {
                    (self.corda.preso, self.corda.alvo, self.segurando) = (Some(k), V::new(p.x, p.y), true);
                }
            }
            Msg::Mover(p) if self.segurando => self.corda.alvo = V::new(p.x, p.y),
            Msg::Soltar if self.segurando => (self.segurando, self.corda.preso) = (false, None),
            Msg::Girar(d) if self.segurando => self.corda.giro += d * GRAUS_POR_DENTE.to_radians(),
            Msg::Quadro if self.desenhou.replace(false) => {
                if self.auto {
                    self.corda.alvo = lissajous(self.passo_auto);
                    self.passo_auto += 1;
                }
                let t0 = Instant::now();
                self.corda.passo(self.angulo, &OBSTACULOS);
                self.fisica_ms = t0.elapsed().as_secs_f32() * 1000.0;
            }
            Msg::Tique => {
                self.quadros.borrow_mut().segundo();
                self.rec.amostrar();
                if self.eco {
                    println!("{}", nucleo::leitura(&self.quadros.borrow(), self.fisica_ms, self.corda.n(), &self.rec).replace('\n', " · "));
                }
                self.ociosos = if self.animando() { 0 } else { self.ociosos + 1 };
            }
            _ => {}
        }
        Task::none()
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        match ev {
            Event::CursorMoved(p) if self.segurando => Some(Msg::Mover(*p)),
            Event::MouseReleased(MouseButton::Left) if self.segurando => Some(Msg::Soltar),
            // A roda chega em px lógicos, 48 por dente.
            Event::Wheel { dy, .. } if self.segurando => Some(Msg::Girar(dy / 48.0)),
            Event::Key { key: Key::Character(c), pressed: true, .. } => match c.as_str() {
                "a" | "A" => Some(Msg::Auto),
                d => d.parse::<usize>().ok().filter(|d| (1..=TAMANHOS.len()).contains(d)).map(|d| Msg::Escolher(d - 1)),
            },
            _ => None,
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let animando = self.animando();
        // shortcut: o pelicano não tem um fluxo "a cada quadro" (o `window::frames()` do Iced); um relógio (`--hz`)
        // mais o `desenhou` fazem no máximo um passo por quadro. Trocar quando o pelicano tiver um.
        let quadro = animando.then(|| Subscription::every(self.intervalo).map(|_| Msg::Quadro));
        let tique = (animando || self.ociosos < 2).then(|| Subscription::every(Duration::from_secs(1)).map(|_| Msg::Tique));
        Subscription::batch(quadro.into_iter().chain(tique))
    }

    fn view(&self) -> Element<'_, Msg> {
        let muted = Color::rgb8(0x59, 0x63, 0x6e);
        let tela = program(Desenho { app: self, pts: self.corda.p.iter().map(|v| ponto(*v)).collect(), chamadas: Cell::new(0) });
        let botoes = TAMANHOS.iter().enumerate().map(|(i, n)| {
            let b = button(text(rotulo(*n)).size(13)).padding([4u16, 10]).on_press(Msg::Escolher(i));
            if i == self.i { b.style(button_primary) } else { b }.into()
        });
        let auto = button(text(if self.auto { "A: automático (ligado)" } else { "A: automático" }).size(13)).padding([4u16, 10]).on_press(Msg::Auto);
        let auto = if self.auto { auto.style(button_primary) } else { auto };
        let angulo = row![text(format!("ângulo máx. {:.0}°", self.angulo)).size(13).width(130.0), slider(5.0..=180.0, self.angulo, Msg::Angulo).step(1.0).width(240.0)].spacing(10).align(Align::Center);
        let leitura = nucleo::leitura(&self.quadros.borrow(), self.fisica_ms, self.corda.n(), &self.rec);
        let painel = container(
            column![
                text(leitura).size(13).font(Font::MONO),
                row![row(botoes).spacing(4), auto].spacing(12),
                angulo,
                text("1–7 escolhem · A automático · arraste um ponto · a roda gira o segmento preso").size(12).color(muted),
            ]
            .spacing(8),
        )
        .padding(10)
        .style(|_| Style::default().background(Color::rgb8(0xf6, 0xf8, 0xfa).alpha(0.92)).border(Border::rounded(8.0)));
        stack![tela, container(painel).padding(10)].into()
    }
}

fn mediana(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// `--medir`: o modo automático sem janela, em cada tamanho. Cada quadro é um passo de física e depois
/// view + layout + gravar + pintar inteiro (o que a janela faz quando tudo muda).
fn medir() {
    const QUADROS: usize = 240;
    let tam = Size::new(LARGURA, ALTURA);
    for (i, n) in TAMANHOS.iter().enumerate() {
        let mut app = Cordas::new();
        let tema = app.theme();
        let _ = app.update(Msg::Escolher(i));
        let _ = app.update(Msg::Auto);
        let (mut r, st) = (Renderer::new(), States::default());
        let (mut fisica, mut quadro) = (Vec::new(), Vec::new());
        for f in 0..QUADROS {
            let _ = app.update(Msg::Quadro);
            let t0 = Instant::now();
            let el = app.view();
            let no = layout(&el, tam, &mut r, &st, &tema);
            r.record();
            draw(&el, &no, &mut r, &st, &tema, None, false);
            let _ = r.changes();
            r.begin(tam, 1.0, tema.background);
            draw(&el, &no, &mut r, &st, &tema, None, false);
            drop(el);
            // Os primeiros aquecem os caches de texto.
            if f >= 5 {
                fisica.push(app.fisica_ms as f64);
                quadro.push(t0.elapsed().as_secs_f64() * 1000.0);
            }
        }
        println!("{n:>6} segmentos: física mediana {:.3} ms · view+layout+desenho mediana {:.2} ms (máx {:.2})", mediana(fisica), mediana(quadro.clone()), quadro.iter().cloned().fold(0.0, f64::max));
    }
}

fn main() -> Result<(), Error> {
    if std::env::args().any(|a| a == "--medir") {
        medir();
        return Ok(());
    }
    let mut app = Cordas::new();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--auto") {
        let n = args.get(i + 1).and_then(|d| d.parse::<usize>().ok()).filter(|d| (1..=TAMANHOS.len()).contains(d)).unwrap_or(TAMANHOS.len());
        let _ = app.update(Msg::Escolher(n - 1));
        let _ = app.update(Msg::Auto);
        app.eco = true;
    }
    if let Some(hz) = args.iter().position(|a| a == "--hz").and_then(|i| args.get(i + 1)).and_then(|h| h.parse::<u64>().ok()).filter(|h| *h > 0) {
        app.intervalo = Duration::from_micros(1_000_000 / hz);
    }
    run(Settings { title: "Cordas (pelicano)".into(), size: Size::new(LARGURA, ALTURA), ..Default::default() }, || (app, Task::none()))
}
