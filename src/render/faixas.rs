//! A [`Lista`] pintada na CPU: o mesmo que o `SHADER` faz, pixel a pixel, dividida em faixas de
//! linhas, uma thread cada. Serve quando não há GPU (ou ela não pinta) e o quadro cabe na lista.
use super::{Lista, Tipo, DO_ATLAS, GLIFO, QUAD, TRACO};
use std::collections::HashMap;
use std::sync::OnceLock;
use tiny_skia::Pixmap;

type V4 = [f32; 4];

/// Pinta `l` em `pm` (do tamanho da lista) só dentro de `area` (x0, y0, x1, y1, pixels):
/// limpa com o fundo e pinta tudo o que cai ali, na ordem dos lotes.
pub(super) fn pintar(l: &Lista, pm: &mut Pixmap, area: [u32; 4]) {
    let [x0, y0, x1, y1] = area;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let w = pm.width() as usize;
    let texturas: HashMap<u64, &Pixmap> = l.usadas.iter().map(|(id, p)| (*id, &**p)).collect();
    let (dec, enc) = tabelas();
    let f = pre(rgba(l.fundo));
    let fundo = [enc[idx(f[0])], enc[idx(f[1])], enc[idx(f[2])], (f[3] * 255.0).round() as u8];
    let linhas = (y1 - y0) as usize;
    // Faixas de pelo menos 32 linhas: abaixo disso, criar a thread custa mais que pintar.
    let n = std::thread::available_parallelism().map_or(1, |n| n.get()).min(linhas.div_ceil(32)).max(1);
    let por = linhas.div_ceil(n);
    let dados = &mut pm.data_mut()[y0 as usize * w * 4..y1 as usize * w * 4];
    let faixa = |i: usize, px: &mut [u8]| {
        let fy0 = y0 + (i * por) as u32;
        let y1 = fy0 + (px.len() / (w * 4)) as u32;
        let mut f = Faixa { px, w, x0, x1, y0: fy0, y1, dec, enc };
        f.limpar(fundo);
        f.lista(l, &texturas);
    };
    if n == 1 {
        return faixa(0, dados);
    }
    std::thread::scope(|s| {
        for (i, px) in dados.chunks_mut(por * w * 4).enumerate() {
            let faixa = &faixa;
            s.spawn(move || faixa(i, px));
        }
    });
}

/// As linhas `y0..y1` do pixmap (largura `w`), pintadas só nas colunas `x0..x1`.
struct Faixa<'a> {
    px: &'a mut [u8],
    w: usize,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    dec: &'a [f32; 256],
    enc: &'a [u8; ENC],
}

/// Passos da tabela linear → sRGB: perto do preto um passo dá menos de 1 nível de sRGB.
const ENC: usize = 4096;

/// sRGB (byte) → linear e linear → sRGB (byte): o que o alvo "Srgb" da GPU faz ao misturar.
fn tabelas() -> (&'static [f32; 256], &'static [u8; ENC]) {
    static T: OnceLock<([f32; 256], [u8; ENC])> = OnceLock::new();
    let t = T.get_or_init(|| {
        (std::array::from_fn(|i| super::linear(i as f32 / 255.0)), std::array::from_fn(|i| (super::srgb(i as f32 / (ENC - 1) as f32) * 255.0 + 0.5) as u8))
    });
    (&t.0, &t.1)
}

fn idx(v: f32) -> usize {
    (v.clamp(0.0, 1.0) * (ENC - 1) as f32 + 0.5) as usize
}

impl Faixa<'_> {
    fn limpar(&mut self, fundo: [u8; 4]) {
        for y in 0..(self.y1 - self.y0) as usize {
            let l = y * self.w * 4;
            for p in self.px[l + self.x0 as usize * 4..l + self.x1 as usize * 4].chunks_exact_mut(4) {
                p.copy_from_slice(&fundo);
            }
        }
    }

    fn lista(&mut self, l: &Lista, texturas: &HashMap<u64, &Pixmap>) {
        let (mut q, mut g, mut t) = (0, 0, 0);
        for &(tipo, tex, n) in &l.lotes {
            for _ in 0..n {
                match tipo {
                    Tipo::Quad => {
                        self.quad(&l.quads[q * QUAD..(q + 1) * QUAD]);
                        q += 1;
                    }
                    Tipo::Glifo => {
                        self.glifo(&l.glifos[g * GLIFO..(g + 1) * GLIFO], tex, l, texturas);
                        g += 1;
                    }
                    Tipo::Traco => {
                        self.traco(&l.tracos[t * TRACO..(t + 1) * TRACO]);
                        t += 1;
                    }
                }
            }
        }
    }

    /// Os pixels cujo centro cai em `lo..hi` (a regra da rasterização da GPU), nesta faixa.
    fn pixels(&self, lo: [f32; 2], hi: [f32; 2]) -> (std::ops::Range<u32>, std::ops::Range<u32>) {
        let r = |a: f32, b: f32, min: u32, max: u32| {
            let i = (a - 0.5).ceil().max(min as f32).min(max as f32) as u32;
            i..((b - 0.5).ceil().max(i as f32).min(max as f32) as u32)
        };
        (r(lo[0], hi[0], self.x0, self.x1), r(lo[1], hi[1], self.y0, self.y1))
    }

    /// `src` pré-multiplicado por cima do pixel: o `One, OneMinusSrcAlpha` do pipeline.
    fn misturar(&mut self, x: u32, y: u32, src: V4) {
        if src[3] <= 0.0 && src[..3].iter().all(|v| *v <= 0.0) {
            return;
        }
        let i = ((y - self.y0) as usize * self.w + x as usize) * 4;
        let p = &mut self.px[i..i + 4];
        let k = 1.0 - src[3].clamp(0.0, 1.0);
        // Em espaço linear: o pixel guarda sRGB, decodifica, mistura e codifica de novo.
        for c in 0..3 {
            p[c] = self.enc[idx(src[c] + self.dec[p[c] as usize] * k)];
        }
        // `+ 0.5` e truncar: o `round` vira chamada de função sem SSE4.1.
        p[3] = ((src[3] * 255.0 + p[3] as f32 * k).clamp(0.0, 255.0) + 0.5) as u8;
    }

    fn quad(&mut self, q: &[f32]) {
        let (rect, raios, rec, params) = (v4(&q[0..]), v4(&q[4..]), &q[40..52], v4(&q[32..]));
        let m = if params[1] == 2.0 { params[3] + 1.0 } else { 1.0 };
        let (xs, ys) = self.pixels(caixa_lo(rect, m, rec), caixa_hi(rect, m, rec));
        // O miolo (longe dos cantos e da borda) tem cobertura 1 e borda 0: só a cor, sem a conta
        // por pixel, e nada a fazer se ela é transparente (o contorno de uma linha da tabela).
        let liso = params[1] != 2.0 && rec[8..12] == [0.0; 4];
        let mg = raios.iter().fold(params[0], |a, r| a.max(*r)) + 1.0;
        let cor = pre(v4(&q[8..]));
        for y in ys {
            let cy = y as f32 + 0.5;
            let (i0, i1) = if liso && cy >= rect[1] + mg && cy <= rect[1] + rect[3] - mg {
                let a = (rect[0] + mg - 0.5).ceil().max(xs.start as f32).min(xs.end as f32) as u32;
                (a, ((rect[0] + rect[2] - mg - 0.5).floor() + 1.0).max(a as f32).min(xs.end as f32) as u32)
            } else {
                (xs.end, xs.end)
            };
            for x in (xs.start..i0).chain(i1..xs.end) {
                let saida = quad_pixel(q, [x as f32 + 0.5, cy]);
                self.misturar(x, y, saida);
            }
            for x in i0..i1 {
                if params[1] == 1.0 {
                    let e = v4(&q[36..]);
                    let t = ((x as f32 + 0.5 - e[0]) * e[2] + (cy - e[1]) * e[3]).clamp(0.0, 1.0);
                    self.misturar(x, y, pre(degrade(&q[8..24], v4(&q[24..]), params[2] as usize, t)));
                } else if cor[3] > 0.0 {
                    self.misturar(x, y, cor);
                }
            }
        }
    }

    fn glifo(&mut self, q: &[f32], tex: u64, l: &Lista, texturas: &HashMap<u64, &Pixmap>) {
        let (dst, uv, cor, raios, tipo, rec) = (v4(&q[0..]), v4(&q[4..]), v4(&q[8..]), v4(&q[12..]), q[16], &q[20..32]);
        let pm = texturas.get(&tex);
        if tex != DO_ATLAS && pm.is_none() {
            return;
        }
        let (xs, ys) = self.pixels(caixa_lo(dst, 0.0, rec), caixa_hi(dst, 0.0, rec));
        // O caso de quase todo texto: máscara do atlas, sem recorte arredondado. Sem o laço geral
        // (recorte e busca do bitmap por pixel), que custava várias vezes mais.
        let mascara = (tex == DO_ATLAS && tipo == 0.0 && rec[8..12] == [0.0; 4]).then(|| l.atlas.bitmaps.get(uv[2] as usize)).flatten();
        if let Some(b) = mascara.filter(|b| b.len() == dst[2] as usize * dst[3] as usize) {
            let gw = dst[2] as usize;
            let c = [cor[0] * cor[3], cor[1] * cor[3], cor[2] * cor[3], cor[3]];
            // floor(x + 0,5 - dst.x) = x + floor(0,5 - dst.x): o deslocamento sai do laço.
            let (ox, oy) = ((0.5 - dst[0]).floor() as i64, (0.5 - dst[1]).floor() as i64);
            for y in ys {
                let linha = (y as i64 + oy) as usize * gw;
                for x in xs.clone() {
                    let cob = b[linha + (x as i64 + ox) as usize];
                    if cob != 0 {
                        self.misturar(x, y, mul(c, cob as f32 / 255.0));
                    }
                }
            }
            return;
        }
        for y in ys {
            for x in xs.clone() {
                let c = [x as f32 + 0.5, y as f32 + 0.5];
                let corte = recorte(c, rec);
                if corte == 0.0 {
                    continue;
                }
                let saida = if tipo == 2.0 {
                    let Some(pm) = pm else { return };
                    mul(linear_pre(bilinear(pm, (c[0] - dst[0]) / dst[2], (c[1] - dst[1]) / dst[3])), cor[3] * cobertura(dst, raios, c))
                } else {
                    let (lx, ly) = ((c[0] - dst[0]).floor() as i64, (c[1] - dst[1]).floor() as i64);
                    let t = if tex == DO_ATLAS { do_atlas(l, uv, dst, lx, ly) } else { texel(pm.expect("conferido acima"), uv[0] as i64 + lx, uv[1] as i64 + ly) };
                    if tipo == 1.0 {
                        mul(linear_pre(t), cor[3])
                    } else {
                        mul([cor[0], cor[1], cor[2], 1.0], t[3] * cor[3])
                    }
                };
                self.misturar(x, y, mul(saida, corte));
            }
        }
    }

    fn traco(&mut self, q: &[f32]) {
        let (ab, meia, cor, rec) = (v4(&q[0..]), q[4], pre(v4(&q[8..])), &q[12..24]);
        let e = meia + 1.0;
        let caixa = [ab[0].min(ab[2]) - e, ab[1].min(ab[3]) - e, (ab[0] - ab[2]).abs() + 2.0 * e, (ab[1] - ab[3]).abs() + 2.0 * e];
        let (xs, ys) = self.pixels(caixa_lo(caixa, 0.0, rec), caixa_hi(caixa, 0.0, rec));
        let ba = [ab[2] - ab[0], ab[3] - ab[1]];
        let bb = (ba[0] * ba[0] + ba[1] * ba[1]).max(1e-6);
        for y in ys {
            for x in xs.clone() {
                let c = [x as f32 + 0.5, y as f32 + 0.5];
                let corte = recorte(c, rec);
                if corte == 0.0 {
                    continue;
                }
                let pa = [c[0] - ab[0], c[1] - ab[1]];
                let h = ((pa[0] * ba[0] + pa[1] * ba[1]) / bb).clamp(0.0, 1.0);
                let d = ((pa[0] - ba[0] * h).powi(2) + (pa[1] - ba[1] * h).powi(2)).sqrt();
                let cob = (meia - d + 0.5).clamp(0.0, 1.0);
                self.misturar(x, y, mul(cor, cob * corte));
            }
        }
    }
}

/// O `fs_quad` num pixel de centro `c`, já com o recorte.
fn quad_pixel(q: &[f32], c: [f32; 2]) -> V4 {
    let (rect, raios, rec, params) = (v4(&q[0..]), v4(&q[4..]), &q[40..52], v4(&q[32..]));
    let corte = recorte(c, rec);
    if corte == 0.0 {
        return [0.0; 4];
    }
    let saida = if params[1] == 2.0 {
        let blur = params[3];
        let a = if blur < 0.5 {
            cobertura(rect, raios, c)
        } else {
            let meio = [rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5];
            let k = match (c[0] > meio[0], c[1] > meio[1]) {
                (false, false) => raios[0],
                (true, false) => raios[1],
                (true, true) => raios[2],
                (false, true) => raios[3],
            };
            sombra(rect, k.min(rect[2].min(rect[3]) / 2.0), c, blur)
        };
        mul(pre(v4(&q[8..])), a * (1.0 - cobertura(v4(&q[36..]), raios, c)))
    } else {
        let cor = if params[1] == 1.0 {
            let e = v4(&q[36..]);
            let t = ((c[0] - e[0]) * e[2] + (c[1] - e[1]) * e[3]).clamp(0.0, 1.0);
            degrade(&q[8..24], v4(&q[24..]), params[2] as usize, t)
        } else {
            v4(&q[8..])
        };
        let fora = cobertura(rect, raios, c);
        let mut saida = mul(pre(cor), fora);
        let w = params[0];
        if w > 0.0 {
            let dentro = [rect[0] + w, rect[1] + w, rect[2] - 2.0 * w, rect[3] - 2.0 * w];
            let cd = if dentro[2] > 0.0 && dentro[3] > 0.0 { cobertura(dentro, raios.map(|r| (r - w).max(0.0)), c) } else { 0.0 };
            let b = mul(pre(v4(&q[28..])), (fora - cd).clamp(0.0, 1.0));
            saida = std::array::from_fn(|i| b[i] + saida[i] * (1.0 - b[3]));
        }
        saida
    };
    mul(saida, corte)
}

fn v4(s: &[f32]) -> V4 {
    [s[0], s[1], s[2], s[3]]
}

/// O fundo como a lista leva as cores: rgb linear.
fn rgba(c: crate::Color) -> V4 {
    let [r, g, b, a] = [c.r, c.g, c.b, c.a].map(|v| v.clamp(0.0, 1.0));
    [super::linear(r), super::linear(g), super::linear(b), a]
}

/// O `linear_pre` do shader: texel pré-multiplicado em sRGB para pré-multiplicado linear.
fn linear_pre(t: V4) -> V4 {
    if t[3] <= 0.0 {
        return [0.0; 4];
    }
    // Pela tabela (um powf por pixel pesava 22% da pintura); erro de meio nível de sRGB.
    let lin = tabelas().0;
    let l = |v: f32| lin[((v / t[3]).clamp(0.0, 1.0) * 255.0 + 0.5) as usize] * t[3];
    [l(t[0]), l(t[1]), l(t[2]), t[3]]
}

fn pre(c: V4) -> V4 {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

fn mul(c: V4, k: f32) -> V4 {
    c.map(|v| v * k)
}

/// O `vertice` do shader: a caixa crescida `m` e cortada pelo retângulo do recorte.
fn caixa_lo(r: V4, m: f32, rec: &[f32]) -> [f32; 2] {
    [(r[0] - m).max(rec[0]), (r[1] - m).max(rec[1])]
}

fn caixa_hi(r: V4, m: f32, rec: &[f32]) -> [f32; 2] {
    let lo = caixa_lo(r, m, rec);
    [(r[0] + r[2] + m).min(rec[2]).max(lo[0]), (r[1] + r[3] + m).min(rec[3]).max(lo[1])]
}

fn cobertura(r: V4, k0: V4, c: [f32; 2]) -> f32 {
    let k = k0.map(|v| v.clamp(0.0, r[2].min(r[3]) / 2.0));
    let (l, t, d, b) = (r[0], r[1], r[0] + r[2], r[1] + r[3]);
    let fx = (c[0] + 0.5 - l).min(d - c[0] + 0.5).clamp(0.0, 1.0);
    let fy = (c[1] + 0.5 - t).min(b - c[1] + 0.5).clamp(0.0, 1.0);
    let mut cob = fx * fy;
    let (raio, o) = if c[0] < l + k[0] && c[1] < t + k[0] {
        (k[0], [l + k[0], t + k[0]])
    } else if c[0] > d - k[1] && c[1] < t + k[1] {
        (k[1], [d - k[1], t + k[1]])
    } else if c[0] > d - k[2] && c[1] > b - k[2] {
        (k[2], [d - k[2], b - k[2]])
    } else if c[0] < l + k[3] && c[1] > b - k[3] {
        (k[3], [l + k[3], b - k[3]])
    } else {
        (0.0, [0.0; 2])
    };
    if raio > 0.0 {
        let dist = ((c[0] - o[0]).powi(2) + (c[1] - o[1]).powi(2)).sqrt();
        cob *= (raio - dist + 0.5).clamp(0.0, 1.0);
    }
    cob
}

fn recorte(c: [f32; 2], rec: &[f32]) -> f32 {
    if c[0] < rec[0] || c[1] < rec[1] || c[0] >= rec[2] || c[1] >= rec[3] {
        return 0.0;
    }
    let rk = v4(&rec[8..]);
    if rk == [0.0; 4] {
        return 1.0;
    }
    cobertura(v4(&rec[4..]), rk, c)
}


/// O `sombra` do shader: a do Iced, 0,5 na borda até zero a `b` dela.
fn sombra(r: V4, k: f32, c: [f32; 2], b: f32) -> f32 {
    let meia = [r[2] * 0.5, r[3] * 0.5];
    let p = [(c[0] - r[0] - meia[0]).abs() - meia[0] + k, (c[1] - r[1] - meia[1]).abs() - meia[1] + k];
    let d = (p[0].max(0.0).hypot(p[1].max(0.0)) - k).max(0.0);
    let t = ((d + b) / (2.0 * b)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

fn degrade(cores: &[f32], paradas: V4, n: usize, t: f32) -> V4 {
    let cor = |i: usize| v4(&cores[4 * i..]);
    let mut i = 0;
    while i < n && paradas[i] < t {
        i += 1;
    }
    if i == 0 {
        return cor(0);
    }
    if i == n {
        return cor(n - 1);
    }
    let (p0, p1) = (paradas[i - 1], paradas[i]);
    let k = if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 };
    let (a, b) = (cor(i - 1), cor(i));
    std::array::from_fn(|j| a[j] + (b[j] - a[j]) * k)
}

/// O pixel `(x, y)` de um pixmap (pré-multiplicado), transparente fora dele (o `textureLoad`).
fn texel(pm: &Pixmap, x: i64, y: i64) -> V4 {
    if x < 0 || y < 0 || x >= pm.width() as i64 || y >= pm.height() as i64 {
        return [0.0; 4];
    }
    let i = (y as usize * pm.width() as usize + x as usize) * 4;
    let d = pm.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]].map(|v| v as f32 / 255.0)
}

/// Bilinear com a borda repetida (o amostrador padrão), `u`/`v` de 0 a 1.
fn bilinear(pm: &Pixmap, u: f32, v: f32) -> V4 {
    let (w, h) = (pm.width() as i64, pm.height() as i64);
    let (tx, ty) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
    let (fx, fy) = (tx - tx.floor(), ty - ty.floor());
    let (x0, y0) = (tx.floor() as i64, ty.floor() as i64);
    let px = |x: i64, y: i64| texel(pm, x.clamp(0, w - 1), y.clamp(0, h - 1));
    let (a, b, c, d) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
    std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy)
}

/// O texel de um glifo como a textura do atlas guarda: cobertura vira branco pré-multiplicado,
/// emoji (RGBA do swash) vira pré-multiplicado.
fn do_atlas(l: &Lista, uv: V4, dst: V4, x: i64, y: i64) -> V4 {
    let (w, h) = (dst[2] as i64, dst[3] as i64);
    let Some(b) = l.atlas.bitmaps.get(uv[2] as usize) else { return [0.0; 4] };
    if x < 0 || y < 0 || x >= w || y >= h {
        return [0.0; 4];
    }
    let i = (y * w + x) as usize;
    if b.len() as i64 == w * h * 4 {
        let a = b[i * 4 + 3] as f32 / 255.0;
        let c = |k: usize| b[i * 4 + k] as f32 / 255.0 * a;
        [c(0), c(1), c(2), a]
    } else {
        [b.get(i).copied().unwrap_or(0) as f32 / 255.0; 4]
    }
}
