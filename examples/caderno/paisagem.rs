//! Imagens geradas: paisagens de montanha, floresta, lago e umas cenas simples, pixel a pixel.
use pelicano::Image;

type Rgb = [f32; 3];

fn rgb(c: u32) -> Rgb {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn mul(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn add(a: Rgb, b: Rgb, k: f32) -> Rgb {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}
fn liso(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn hash(i: i32, j: i32, s: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(374761393) ^ (j as u32).wrapping_mul(668265263) ^ s.wrapping_mul(2246822519);
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    ((h ^ (h >> 16)) & 0xffff) as f32 / 65535.0
}
fn ruido(x: f32, s: u32) -> f32 {
    let i = x.floor();
    let f = liso(x - i);
    let a = hash(i as i32, 0, s);
    a + (hash(i as i32 + 1, 0, s) - a) * f
}
fn ruido2(x: f32, y: f32, s: u32) -> f32 {
    let (i, j) = (x.floor(), y.floor());
    let (fx, fy) = (liso(x - i), liso(y - j));
    let (i, j) = (i as i32, j as i32);
    let a = hash(i, j, s) + (hash(i + 1, j, s) - hash(i, j, s)) * fx;
    let b = hash(i, j + 1, s) + (hash(i + 1, j + 1, s) - hash(i, j + 1, s)) * fx;
    a + (b - a) * fy
}
/// Cumes pontudos: ruído "dobrado" em várias oitavas.
fn cume(u: f32, freq: f32, s: u32) -> f32 {
    let (mut h, mut a, mut f) = (0.0, 0.55, freq);
    for o in 0..5 {
        let n = 1.0 - (2.0 * ruido(u * f, s + o) - 1.0).abs();
        h += a * n * n;
        a *= 0.48;
        f *= 2.1;
    }
    h
}

struct Camada {
    cor: u32,
    base: f32,
    amp: f32,
    freq: f32,
    seed: u32,
    neve: bool,
    nevoa: f32,
    /// Pinheiros: altura relativa (0 = cume de montanha).
    arvores: f32,
}

const fn cam(cor: u32, base: f32, amp: f32, freq: f32, seed: u32, neve: bool, nevoa: f32, arvores: f32) -> Camada {
    Camada { cor, base, amp, freq, seed, neve, nevoa, arvores }
}

struct Cena {
    ceu: &'static [(f32, u32)],
    sol: Option<(f32, f32, u32)>,
    nuvens: Option<u32>,
    camadas: &'static [Camada],
    agua: Option<f32>,
    torii: bool,
}

const POR_DO_SOL: Cena = Cena {
    ceu: &[(0.0, 0x2c2a5e), (0.3, 0x8e4f8f), (0.5, 0xe2777a), (0.62, 0xf6a76a), (0.7, 0xffd08a)],
    sol: Some((0.86, 0.56, 0xffe3a8)),
    nuvens: Some(0xf08aa0),
    camadas: &[
        cam(0x5d4f8e, 0.64, 0.5, 1.6, 3, true, 0.3, 0.0),
        cam(0x34356a, 0.74, 0.3, 2.6, 9, true, 0.15, 0.0),
        cam(0x1c2046, 0.86, 0.14, 4.0, 21, false, 0.08, 0.0),
        cam(0x0d1226, 0.98, 0.0, 1.0, 33, false, 0.0, 0.14),
    ],
    agua: None,
    torii: false,
};

const LAGO: Cena = Cena {
    ceu: &[(0.0, 0x3a3a78), (0.3, 0x9a6aa6), (0.5, 0xf08c8c), (0.6, 0xf8b98a)],
    sol: None,
    nuvens: Some(0xf09ab0),
    camadas: &[
        cam(0x8a86b8, 0.52, 0.42, 2.6, 5, true, 0.3, 0.0),
        cam(0x4e5590, 0.6, 0.22, 4.0, 11, true, 0.15, 0.0),
        cam(0x152238, 0.66, 0.0, 1.0, 17, false, 0.0, 0.3),
    ],
    agua: Some(0.68),
    torii: false,
};

const FLORESTA: Cena = Cena {
    ceu: &[(0.0, 0xdfe9d2), (0.5, 0xb6cfa6), (1.0, 0x7fa877)],
    sol: Some((0.55, 0.15, 0xfffbe0)),
    nuvens: None,
    camadas: &[
        cam(0x9cbb92, 0.55, 0.0, 1.0, 2, false, 0.4, 0.5),
        cam(0x5f8a58, 0.75, 0.0, 1.0, 7, false, 0.2, 0.6),
        cam(0x2c4a2a, 0.92, 0.0, 1.0, 13, false, 0.0, 0.8),
    ],
    agua: Some(0.86),
    torii: false,
};

const TORII: Cena = Cena {
    ceu: &[(0.0, 0x52305e), (0.35, 0xc65a5a), (0.55, 0xf59a5c), (0.62, 0xffc98a)],
    sol: Some((0.5, 0.6, 0xfff0c0)),
    nuvens: Some(0xf5a080),
    camadas: &[cam(0x6a3a5a, 0.6, 0.12, 2.0, 4, false, 0.4, 0.0), cam(0x2a1828, 0.62, 0.0, 1.0, 8, false, 0.0, 0.12)],
    agua: Some(0.64),
    torii: true,
};

const VALE: Cena = Cena {
    ceu: &[(0.0, 0x9bc4c4), (0.45, 0xe6dcc0), (0.6, 0xf2d6a0)],
    sol: None,
    nuvens: None,
    camadas: &[
        cam(0x7aa0a0, 0.55, 0.45, 3.0, 14, false, 0.4, 0.0),
        cam(0x3e6b5c, 0.7, 0.3, 4.0, 19, false, 0.2, 0.0),
        cam(0x1d3a2c, 0.95, 0.0, 1.0, 23, false, 0.0, 0.45),
    ],
    agua: None,
    torii: false,
};

fn cor_ceu(c: &Cena, v: f32) -> Rgb {
    let p = c.ceu;
    if v <= p[0].0 {
        return rgb(p[0].1);
    }
    for w in p.windows(2) {
        if v <= w[1].0 {
            return mix(rgb(w[0].1), rgb(w[1].1), (v - w[0].0) / (w[1].0 - w[0].0));
        }
    }
    rgb(p[p.len() - 1].1)
}

/// Altura do relevo em `u`: picos largos com um pouco de rugosidade.
fn relevo(l: &Camada, u: f32, asp: f32) -> f32 {
    if l.amp <= 0.0 {
        return 0.0;
    }
    let freq = l.freq * asp.max(1.0).powf(0.7);
    let passo = 1.0 / freq;
    let k = (u / passo).floor() as i32;
    let mut h: f32 = 0.0;
    for j in k - 2..=k + 2 {
        let c = (j as f32 + 0.5 + (hash(j, 4, l.seed) - 0.5) * 0.6) * passo;
        let alto = 0.25 + 0.75 * hash(j, 5, l.seed).powf(1.6);
        let meia = passo * (0.8 + 0.7 * hash(j, 6, l.seed));
        let d = 1.0 - (u - c).abs() / meia;
        if d > 0.0 {
            h = h.max(alto * d.powf(1.7));
        }
    }
    l.amp * (h + 0.22 * (cume(u, freq * 3.0, l.seed) - 0.3) * h.max(0.15))
}

/// Topo da camada em `u` (0 a 1, de cima para baixo).
fn topo(l: &Camada, u: f32, asp: f32) -> f32 {
    let mut t = l.base - relevo(l, u, asp);
    if l.arvores > 0.0 {
        // Pinheiros: triângulos de alturas variadas, lado a lado.
        let passo = 0.03 / asp;
        let k = (u / passo).floor() as i32;
        let mut alto: f32 = 0.0;
        for j in k - 3..=k + 3 {
            let c = (j as f32 + 0.5 + (hash(j, 1, l.seed) - 0.5) * 0.8) * passo;
            let h = l.arvores * (0.4 + 0.6 * hash(j, 2, l.seed));
            let meia = h * 0.3 / asp;
            let d = (u - c).abs() / meia;
            if d < 1.0 {
                // Degraus dos galhos.
                let serra = 1.0 - d - 0.1 * ((1.0 - d) * 6.0).fract();
                alto = alto.max(h * serra.max(0.0));
            }
        }
        t -= alto;
    }
    t
}

fn pixel(c: &Cena, u: f32, v: f32, asp: f32, px: f32) -> Rgb {
    let mut cor = cor_ceu(c, v);
    if let Some((su, sv, sc)) = c.sol {
        let d = ((u - su) * asp).hypot(v - sv);
        cor = add(cor, rgb(sc), (0.5 * (-d * 6.0).exp()).min(0.6));
        cor = mix(cor, rgb(sc), liso((0.06 - d) / 0.006));
    }
    if let Some(nc) = c.nuvens {
        let n = ruido2(u * 2.5 * asp, v * 22.0, 41) * 0.65 + ruido2(u * 9.0 * asp, v * 50.0, 42) * 0.35;
        let k = liso((n - 0.6) / 0.15) * (1.0 - liso((v - 0.1) / 0.45)) * 0.5;
        cor = mix(cor, rgb(nc), k);
    }
    for l in c.camadas {
        let t = topo(l, u, asp);
        let cobre = ((v - t) / px + 0.5).clamp(0.0, 1.0);
        if cobre <= 0.0 {
            continue;
        }
        let e = 0.004;
        let inclina = (topo(l, u + e, asp) - topo(l, u - e, asp)) / (2.0 * e);
        let mut lc = rgb(l.cor);
        if l.amp > 0.0 {
            lc = mul(lc, 1.0 + 0.22 * (inclina * 2.0).tanh() + 0.06 * (ruido2(u * asp * 30.0, v * 30.0, l.seed) - 0.5));
        }
        if l.neve {
            let alto = relevo(l, u, asp) / l.amp;
            let fundo = (v - t) / l.amp;
            let k = liso((alto - 0.45) / 0.2) * liso(((alto - 0.4) * 0.5 - fundo) / 0.04) * (0.55 + 0.45 * ruido(u * asp * 40.0, l.seed));
            let neve = if inclina > 0.0 { rgb(0xf6d8e4) } else { rgb(0x8f88c4) };
            lc = mix(lc, neve, k);
        }
        lc = mul(lc, 1.0 - 0.25 * liso((v - t) / 0.5));
        lc = mix(lc, cor_ceu(c, t), l.nevoa);
        cor = mix(cor, lc, cobre);
    }
    if c.torii {
        let dentro = |x0: f32, x1: f32, y0: f32, y1: f32| ((u - 0.5) * asp).abs() >= x0 && ((u - 0.5) * asp).abs() <= x1 && v >= y0 && v <= y1;
        let curva = ((u - 0.5) * asp).powi(2) * 0.25;
        let k = dentro(0.12, 0.16, 0.3, 0.64) || dentro(0.0, 0.3, 0.28 - curva, 0.32 - curva) || dentro(0.0, 0.22, 0.38, 0.41);
        if k {
            cor = rgb(0x3a0c14);
        }
    }
    cor
}

fn render(w: u32, h: u32, f: impl Fn(f32, f32, f32, f32) -> Rgb) -> Image {
    let asp = w as f32 / h as f32;
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = f((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32, asp, 1.0 / h as f32);
            px.extend(c.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8));
            px.push(255);
        }
    }
    Image::from_rgba(w, h, px).expect("tamanho certo")
}

fn paisagem(c: &Cena, u: f32, v: f32, asp: f32, px: f32) -> Rgb {
    match c.agua {
        Some(hz) if v > hz => {
            let onda = (v * 300.0).sin() * 0.003 + ruido2(u * 30.0, v * 120.0, 77) * 0.004;
            let r = pixel(c, u + onda, 2.0 * hz - v, asp, px);
            mix(mul(r, 0.8), rgb(0x1a2440), 0.25 + (v - hz) * 0.8)
        }
        _ => pixel(c, u, v, asp, px),
    }
}

fn interior(u: f32, v: f32, asp: f32) -> Rgb {
    let mut c = mix(rgb(0x6a4630), rgb(0x2c1a10), v);
    let janela = u > 0.08 && u < 0.5 && v > 0.08 && v < 0.55;
    if janela {
        c = mix(rgb(0xbfd8c0), rgb(0x6f9a6a), (v - 0.08) / 0.47);
        let gx = ((u - 0.08) / 0.14).fract();
        let gy = ((v - 0.08) / 0.157).fract();
        if !(0.06..=0.94).contains(&gx) || !(0.06..=0.94).contains(&gy) {
            c = rgb(0x3a2416);
        }
    }
    // Luz da luminária.
    let d = ((u - 0.78) * asp).hypot(v - 0.3);
    c = add(c, rgb(0xffc070), 0.35 * (-d * 5.0).exp());
    // Mesa.
    let m = ((u - 0.55) / 0.42).powi(2) + ((v - 0.8) / 0.12).powi(2);
    if m < 1.0 {
        c = mix(rgb(0xa77650), rgb(0x6a4228), m);
    }
    c
}

fn codigo(u: f32, v: f32) -> Rgb {
    let fundo = rgb(0x0e131d);
    let linha = (v * 14.0).floor() as i32;
    let dentro = (v * 14.0).fract();
    if !(0.3..0.7).contains(&dentro) || linha == 0 || linha == 13 {
        return fundo;
    }
    let recuo = 0.08 + 0.06 * (hash(linha, 3, 5) * 3.0).floor();
    let mut x = recuo;
    let cores = [0x6aa8ff, 0xe57fb6, 0x7fd18b, 0xe9c46a, 0x8a93a6];
    for k in 0..4 {
        let larg = 0.06 + 0.16 * hash(linha, k, 9);
        if u >= x && u < x + larg {
            return mix(fundo, rgb(cores[((hash(linha, k, 11) * 5.0) as usize).min(4)]), 0.85);
        }
        x += larg + 0.03;
        if x > 0.9 {
            break;
        }
    }
    fundo
}

fn comida(u: f32, v: f32, asp: f32) -> Rgb {
    let tabua = ruido2(u * 3.0, v * 40.0, 6) * 0.15;
    let mut c = mul(rgb(0x4a2c1c), 0.85 + tabua);
    for (cx, cy, r, cor) in [(0.32, 0.4, 0.26, 0xd9732e), (0.74, 0.62, 0.22, 0xc23b2a), (0.62, 0.18, 0.13, 0x8fbf5a)] {
        let d = ((u - cx) * asp).hypot(v - cy);
        if d < r {
            let dentro = d < r * 0.78;
            c = if dentro { mul(rgb(cor), 0.75 + 0.5 * ruido2(u * 40.0, v * 40.0, 3)) } else { rgb(0xe8e0d4) };
        }
    }
    c
}

/// Cenas: 0 pôr do sol, 1 floresta, 2 quarto, 3 torii, 4 código, 5 vale, 6 comida, 7 lago.
pub fn cena(n: usize, w: u32, h: u32) -> Image {
    match n {
        1 => render(w, h, |u, v, a, p| paisagem(&FLORESTA, u, v, a, p)),
        2 => render(w, h, |u, v, a, _| interior(u, v, a)),
        3 => render(w, h, |u, v, a, p| paisagem(&TORII, u, v, a, p)),
        4 => render(w, h, |u, v, _, _| codigo(u, v)),
        5 => render(w, h, |u, v, a, p| paisagem(&VALE, u, v, a, p)),
        6 => render(w, h, |u, v, a, _| comida(u, v, a)),
        7 => render(w, h, |u, v, a, p| paisagem(&LAGO, u, v, a, p)),
        _ => render(w, h, |u, v, a, p| paisagem(&POR_DO_SOL, u, v, a, p)),
    }
}
