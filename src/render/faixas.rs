//! A [`Lista`] pintada na CPU: o mesmo que o `SHADER` faz, pixel a pixel, dividida em faixas de
//! linhas, uma thread cada. Serve quando não há GPU (ou ela não pinta) e o quadro cabe na lista.
use super::{Lista, Tipo, DO_ATLAS, GLIFO, QUAD, TRACO};
use std::collections::HashMap;
use std::sync::OnceLock;
use tiny_skia::Pixmap;

type V4 = [f32; 4];

/// Pixels pintados por caminho desde a última [`contagem`] (`PELICANO_DANO=1` mostra no `[quadro]`).
static CONTAS: [std::sync::atomic::AtomicU64; 8] = [const { std::sync::atomic::AtomicU64::new(0) }; 8];
const QUAD_RAPIDO: usize = 0;
const QUAD_LENTO: usize = 1;
const SOMBRA: usize = 2;
const IMAGEM_RAPIDA: usize = 3;
const IMAGEM_LENTA: usize = 4;
const TEXTO: usize = 5;
const OUTRO: usize = 6;
const OCULTO: usize = 7;

fn contar(i: usize, n: u64) {
    if n > 0 {
        CONTAS[i].fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Os pixels por caminho (milhares) desde a última chamada, e zera.
pub(crate) fn contagem() -> String {
    let n: Vec<u64> = CONTAS.iter().map(|c| c.swap(0, std::sync::atomic::Ordering::Relaxed) / 1000).collect();
    format!("[px mil: quad {}+{}lento sombra {} imagem {}+{}lenta texto {} outro {} pulados {}]", n[0], n[1], n[2], n[3], n[4], n[5], n[6], n[7])
}

/// Um desenho opaco grande: no retângulo `px` (pixels x0, y0, x1, y1, fim exclusivo), dentro do
/// recorte `rec`, ele escreve todo pixel com cor opaca. O que vem antes dele ali não aparece.
struct Oclusor {
    ordem: usize,
    px: [i64; 4],
    rec: [f32; 12],
}

/// Imagens já conferidas: todo pixel opaco? (pelo número da imagem, que não muda de conteúdo).
fn textura_opaca(id: u64, pm: &Pixmap) -> bool {
    static VISTAS: std::sync::Mutex<Option<HashMap<u64, bool>>> = std::sync::Mutex::new(None);
    let mut v = VISTAS.lock().unwrap_or_else(|e| e.into_inner());
    let mapa = v.get_or_insert_with(HashMap::new);
    if mapa.len() > 4096 {
        mapa.clear();
    }
    *mapa.entry(id).or_insert_with(|| pm.pixels().iter().all(|p| p.alpha() == 255))
}

/// Os desenhos opacos grandes da lista (os 64 maiores, na ordem): o que eles cobrem não é pintado
/// pelos de baixo. A folha debaixo da página do PDF e o fundo esticado debaixo do trecho nítido
/// eram metade dos pixels de um quadro.
fn oclusores(l: &Lista, texturas: &HashMap<u64, &Pixmap>) -> Vec<Oclusor> {
    let mut v = Vec::new();
    let (mut q, mut g, mut ordem) = (0, 0, 0);
    for &(tipo, tex, n) in &l.lotes {
        for _ in 0..n {
            match tipo {
                Tipo::Quad => {
                    let d = &l.quads[q * QUAD..(q + 1) * QUAD];
                    let (rect, raios, params) = (v4(&d[0..]), v4(&d[4..]), v4(&d[32..]));
                    if params[1] != 1.0 && params[1] != 2.0 && d[11] >= 1.0 {
                        // Como o miolo liso do `quad`: longe dos cantos e da borda.
                        let mg = raios.iter().fold(params[0], |a, r| a.max(*r)) + 1.0;
                        let px = [(rect[0] + mg - 0.5).ceil() as i64, (rect[1] + mg - 0.5).ceil() as i64, (rect[0] + rect[2] - mg - 0.5).floor() as i64 + 1, (rect[1] + rect[3] - mg - 0.5).floor() as i64 + 1];
                        v.push(Oclusor { ordem, px, rec: std::array::from_fn(|i| d[40 + i]) });
                    }
                    q += 1;
                }
                Tipo::Glifo => {
                    let d = &l.glifos[g * GLIFO..(g + 1) * GLIFO];
                    let (dst, cor, raios, tp) = (v4(&d[0..]), v4(&d[8..]), v4(&d[12..]), d[16]);
                    if tp == 2.0 && cor[3] >= 1.0 && raios == [0.0; 4] && texturas.get(&tex).is_some_and(|pm| textura_opaca(tex, pm)) {
                        // Como o miolo do atalho da imagem: o pixel todo dentro.
                        let px = [dst[0].ceil() as i64, dst[1].ceil() as i64, (dst[0] + dst[2]).floor() as i64, (dst[1] + dst[3]).floor() as i64];
                        v.push(Oclusor { ordem, px, rec: std::array::from_fn(|i| d[20 + i]) });
                    }
                    g += 1;
                }
                _ => {}
            }
            ordem += 1;
        }
    }
    v.retain(|o| o.px[2] > o.px[0] && o.px[3] > o.px[1] && (o.px[2] - o.px[0]) * (o.px[3] - o.px[1]) >= 4096);
    if v.len() > 64 {
        v.sort_by_key(|o| std::cmp::Reverse((o.px[2] - o.px[0]) * (o.px[3] - o.px[1])));
        v.truncate(64);
        v.sort_by_key(|o| o.ordem);
    }
    v
}

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
    let opacos = oclusores(l, &texturas);
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
        let mut f = Faixa { px, w, x0, x1, y0: fy0, y1, dec, enc, opacos: &opacos, ordem: 0 };
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
    opacos: &'a [Oclusor],
    /// O número do desenho sendo pintado (na ordem dos lotes).
    ordem: usize,
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
                    // A lista com vidro não vem para as faixas (`App::redraw`, `capture_bands`).
                    Tipo::Vidro => {}
                }
                self.ordem += 1;
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

    /// As partes de `xs` na linha `y` que nenhum desenho opaco posterior cobre (até 8 trechos;
    /// passando disso, o resto conta como descoberto: pinta a mais, nunca a menos).
    fn lacunas(&self, y: u32, xs: std::ops::Range<u32>) -> ([(u32, u32); 9], usize) {
        let mut cobre = [(0u32, 0u32); 8];
        let mut n = 0;
        let i = self.opacos.partition_point(|o| o.ordem <= self.ordem);
        for o in &self.opacos[i..] {
            if n == 8 {
                break;
            }
            if (y as i64) < o.px[1] || (y as i64) >= o.px[3] {
                continue;
            }
            let (l0, l1) = linha_lisa(&o.rec, y as f32 + 0.5);
            let (a, b) = ((o.px[0].max(l0 as i64).max(xs.start as i64)) as u32, (o.px[2].min(l1 as i64).min(xs.end as i64).max(0)) as u32);
            if a < b {
                cobre[n] = (a, b);
                n += 1;
            }
        }
        let mut saida = [(0u32, 0u32); 9];
        if n == 0 {
            saida[0] = (xs.start, xs.end);
            return (saida, 1);
        }
        cobre[..n].sort_unstable();
        let (mut m, mut x) = (0, xs.start);
        for &(a, b) in &cobre[..n] {
            if a > x {
                saida[m] = (x, a);
                m += 1;
            }
            x = x.max(b);
        }
        if x < xs.end {
            saida[m] = (x, xs.end);
            m += 1;
        }
        contar(OCULTO, (xs.len() as u64).saturating_sub(saida[..m].iter().map(|(a, b)| (b - a) as u64).sum()));
        (saida, m)
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
        let liso = params[1] != 2.0;
        let mg = raios.iter().fold(params[0], |a, r| a.max(*r)) + 1.0;
        let cor = pre(v4(&q[8..]));
        // Cor sólida e opaca (fundo, painel, página): misturar dá a própria cor, então o miolo é
        // copiado linha a linha em bytes já codificados, sem a conta por pixel. Igual ao `misturar`
        // com k = 0, e é a maior parte dos pixels de um quadro.
        let opaca = (params[1] != 1.0 && cor[3] >= 1.0).then(|| [self.enc[idx(cor[0])], self.enc[idx(cor[1])], self.enc[idx(cor[2])], 255u8]);
        let (mut rapidos, mut lentos) = (0u64, 0u64);
        for y in ys {
            // Só o que nenhum opaco posterior cobre.
            let (trechos, nt) = self.lacunas(y, xs.clone());
            for &(t0, t1) in &trechos[..nt] {
            let xs = t0..t1;
            let cy = y as f32 + 0.5;
            let (i0, i1) = if liso && cy >= rect[1] + mg && cy <= rect[1] + rect[3] - mg {
                let (l0, l1) = linha_lisa(rec, cy);
                let a = (rect[0] + mg - 0.5).ceil().max(xs.start.max(l0) as f32).min(xs.end as f32) as u32;
                (a, ((rect[0] + rect[2] - mg - 0.5).floor() + 1.0).min(l1 as f32).max(a as f32).min(xs.end as f32) as u32)
            } else {
                (xs.end, xs.end)
            };
            // Sombra: debaixo da caixa que a projeta ela é zero (`1 - cobertura` = 0); só a volta conta.
            let (j0, j1) = match params[1] == 2.0 {
                true => {
                    let (b, mb) = (v4(&q[36..]), raios.iter().fold(0.0f32, |a, r| a.max(*r)) + 1.0);
                    if cy >= b[1] + mb && cy <= b[1] + b[3] - mb {
                        let a = (b[0] + mb - 0.5).ceil().max(xs.start as f32).min(xs.end as f32) as u32;
                        (a, ((b[0] + b[2] - mb - 0.5).floor() + 1.0).max(a as f32).min(xs.end as f32) as u32)
                    } else {
                        (xs.end, xs.end)
                    }
                }
                false => (xs.end, xs.end),
            };
            rapidos += (i1 - i0) as u64;
            for x in (xs.start..i0).chain(i1..xs.end) {
                if x >= j0 && x < j1 {
                    continue;
                }
                lentos += 1;
                let saida = quad_pixel(q, [x as f32 + 0.5, cy]);
                self.misturar(x, y, saida);
            }
            if let Some(bytes) = opaca.filter(|_| i1 > i0) {
                let l = (y - self.y0) as usize * self.w * 4;
                for p in self.px[l + i0 as usize * 4..l + i1 as usize * 4].chunks_exact_mut(4) {
                    p.copy_from_slice(&bytes);
                }
                continue;
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
        contar(QUAD_RAPIDO, rapidos);
        contar(if params[1] == 2.0 { SOMBRA } else { QUAD_LENTO }, lentos);
    }

    fn glifo(&mut self, q: &[f32], tex: u64, l: &Lista, texturas: &HashMap<u64, &Pixmap>) {
        let (dst, uv, cor, raios, tipo, rec) = (v4(&q[0..]), v4(&q[4..]), v4(&q[8..]), v4(&q[12..]), q[16], &q[20..32]);
        let pm = texturas.get(&tex);
        if tex != DO_ATLAS && pm.is_none() {
            return;
        }
        let (xs, ys) = self.pixels(caixa_lo(dst, 0.0, rec), caixa_hi(dst, 0.0, rec));
        // O caso de quase todo texto: máscara do atlas. Sem o laço geral
        // (recorte e busca do bitmap por pixel), que custava várias vezes mais.
        let mascara = (tex == DO_ATLAS && tipo == 0.0).then(|| l.atlas.bitmaps.get(uv[2] as usize)).flatten();
        if let Some(b) = mascara.filter(|b| b.len() == dst[2] as usize * dst[3] as usize) {
            contar(TEXTO, (xs.len() * ys.len()) as u64);
            let gw = dst[2] as usize;
            let c = [cor[0] * cor[3], cor[1] * cor[3], cor[2] * cor[3], cor[3]];
            // floor(x + 0,5 - dst.x) = x + floor(0,5 - dst.x): o deslocamento sai do laço.
            let (ox, oy) = ((0.5 - dst[0]).floor() as i64, (0.5 - dst[1]).floor() as i64);
            for y in ys {
                let linha = (y as i64 + oy) as usize * gw;
                let (l0, l1) = linha_lisa(rec, y as f32 + 0.5);
                for x in xs.clone() {
                    let cob = b[linha + (x as i64 + ox) as usize];
                    if cob == 0 {
                        continue;
                    }
                    // Perto de um canto arredondado do recorte, a cobertura dele entra.
                    let corte = if x >= l0 && x < l1 { 1.0 } else { recorte([x as f32 + 0.5, y as f32 + 0.5], rec) };
                    if corte > 0.0 {
                        self.misturar(x, y, mul(c, cob as f32 / 255.0 * corte));
                    }
                }
            }
            return;
        }
        // Imagem sem cantos nem opacidade (a página do PDF, em qualquer zoom): no miolo, com os 4
        // texels opacos, o bilinear em bytes já é o resultado (decodificar e codificar de volta dá o
        // mesmo nível), sem a conta em linear por pixel. As colunas são as mesmas em toda linha:
        // calculadas uma vez. A borda e os texels translúcidos seguem pela conta geral.
        // A nítida (tipo 4) pelo mesmo caminho: os dois texels são o mesmo e o peso é 0.
        let nitida = tipo == 4.0;
        if let Some(pm) = pm.filter(|_| (tipo == 2.0 || nitida) && cor[3] >= 1.0 && raios == [0.0; 4]) {
            let (iw, ih) = (pm.width() as i64, pm.height() as i64);
            // Como o `bilinear`: u = (c - dst.x) / dst.w; t = u * w - 0,5 (a nítida: o texel de u * w).
            let eixo = |c: f32, o: f32, tam: f32, n: i64| {
                if nitida {
                    let i = ((c - o) / tam * n as f32).floor().clamp(0.0, (n - 1) as f32) as usize;
                    return (i, i, 0.0);
                }
                let t = (c - o) / tam * n as f32 - 0.5;
                let i = t.floor() as i64;
                (i.clamp(0, n - 1) as usize, (i + 1).clamp(0, n - 1) as usize, t - t.floor())
            };
            let amostrar = |pm: &Pixmap, u: f32, v: f32| if nitida { nitido(pm, u, v) } else { bilinear(pm, u, v) };
            let colunas: Vec<(usize, usize, f32)> = xs.clone().map(|x| eixo(x as f32 + 0.5, dst[0], dst[2], iw)).collect();
            // Miolo: o pixel todo dentro da imagem (cobertura 1).
            let (mx0, mx1) = ((dst[0].ceil() as i64).max(xs.start as i64), ((dst[0] + dst[2]).floor() as i64).min(xs.end as i64));
            let (my0, my1) = ((dst[1].ceil() as i64).max(ys.start as i64), ((dst[1] + dst[3]).floor() as i64).min(ys.end as i64));
            let d = pm.data();
            let (mut rapidos, mut lentos) = (0u64, 0u64);
            for y in ys.clone() {
                let miolo_y = (y as i64) >= my0 && (y as i64) < my1;
                let (ty0, ty1, fy) = eixo(y as f32 + 0.5, dst[1], dst[3], ih);
                let (l0, l1) = linha_lisa(rec, y as f32 + 0.5);
                let (trechos, nt) = self.lacunas(y, xs.clone());
                for x in trechos[..nt].iter().flat_map(|&(t0, t1)| t0..t1) {
                    let c = [x as f32 + 0.5, y as f32 + 0.5];
                    if miolo_y && (x as i64) >= mx0 && (x as i64) < mx1 && x >= l0 && x < l1 {
                        let (tx0, tx1, fx) = colunas[(x - xs.start) as usize];
                        let ix = [(ty0, tx0), (ty0, tx1), (ty1, tx0), (ty1, tx1)].map(|(ty, tx)| (ty * iw as usize + tx) * 4);
                        if ix.iter().all(|i| d[i + 3] == 255) {
                            let o = ((y - self.y0) as usize * self.w + x as usize) * 4;
                            for k in 0..3 {
                                let (a, b, c, e) = (d[ix[0] + k] as f32, d[ix[1] + k] as f32, d[ix[2] + k] as f32, d[ix[3] + k] as f32);
                                let v = (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + e * fx) * fy;
                                self.px[o + k] = (v + 0.5) as u8;
                            }
                            self.px[o + 3] = 255;
                            rapidos += 1;
                            continue;
                        }
                    }
                    lentos += 1;
                    let corte = recorte(c, rec);
                    if corte > 0.0 {
                        let s = mul(linear_pre(amostrar(pm, (c[0] - dst[0]) / dst[2], (c[1] - dst[1]) / dst[3])), cobertura(dst, raios, c));
                        self.misturar(x, y, mul(s, corte));
                    }
                }
            }
            contar(IMAGEM_RAPIDA, rapidos);
            contar(IMAGEM_LENTA, lentos);
            return;
        }
        contar(OUTRO, (xs.len() * ys.len()) as u64);
        for y in ys {
            for x in xs.clone() {
                let c = [x as f32 + 0.5, y as f32 + 0.5];
                let corte = recorte(c, rec);
                if corte == 0.0 {
                    continue;
                }
                let saida = if tipo == 2.0 || nitida {
                    let Some(pm) = pm else { return };
                    let (u, v) = ((c[0] - dst[0]) / dst[2], (c[1] - dst[1]) / dst[3]);
                    let t = if nitida { nitido(pm, u, v) } else { bilinear(pm, u, v) };
                    mul(linear_pre(t), cor[3] * cobertura(dst, raios, c))
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
        contar(OUTRO, (xs.len() * ys.len()) as u64);
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

/// As colunas `x0..x1` da linha de centro `cy` em que o recorte vale 1 (o [`recorte`] sem conta).
/// Recorte arredondado: só longe dos cantos (fora da faixa deles, vazio) e um pixel para dentro da
/// borda suavizada. A mesa e os painéis têm cantos redondos: sem isso nada lá dentro ia pelo atalho.
fn linha_lisa(rec: &[f32], cy: f32) -> (u32, u32) {
    if cy < rec[1] || cy >= rec[3] {
        return (0, 0);
    }
    let rk = v4(&rec[8..]);
    let (lo, hi) = if rk == [0.0; 4] {
        (rec[0], rec[2])
    } else {
        let (rr, k) = (v4(&rec[4..]), rk.iter().fold(0.0f32, |a, r| a.max(*r)) + 1.0);
        if cy < rr[1] + k || cy > rr[1] + rr[3] - k {
            return (0, 0);
        }
        (rec[0].max(rr[0] + 1.0), rec[2].min(rr[0] + rr[2] - 1.0))
    };
    let x0 = (lo - 0.5).ceil().max(0.0) as u32;
    (x0, ((hi - 0.5).ceil().max(0.0) as u32).max(x0))
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

/// O texel em que (u, v) cai (`u`/`v` de 0 a 1), sem suavizar: a imagem nítida.
fn nitido(pm: &Pixmap, u: f32, v: f32) -> V4 {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    texel(pm, (u * w).floor().clamp(0.0, w - 1.0) as i64, (v * h).floor().clamp(0.0, h - 1.0) as i64)
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
