//! Objetos empilhados (sprite stacking) desenhados pela GPU: um [`StackAtlas`] guarda as fatias de
//! um volume (material e normal por texel, a silhueta engordada de cada fatia para o contorno, a
//! paleta de 3 tons e a caixa cheia de cada fatia), e [`Renderer::draw_stacks`] desenha cada
//! [`Stack`] como uma instância só: o vertex shader abre as fatias pelo número do vértice, então o
//! trabalho da CPU é por objeto, não por fatia. A luz fica presa à tela: o fragment shader gira a
//! normal pelo giro do objeto e escolhe o tom de luz, base ou sombra do material.
//!
//! Aqui também estão a mesma conta na CPU (as faixas e o tiny-skia, para a captura sem GPU) e o
//! pipeline. Fica de fora: ordenar os objetos (quem chama já manda de trás para a frente), as linhas
//! de contorno por dentro (uma parte na frente da outra) e mais de 3 tons por material.
//!
//! Cada pilha também escolhe os limiares dos 3 tons, uma tinta (multiplica e soma na cor, em sRGB) e
//! onde começa a paleta dela no atlas: a luz do horário, a névoa da borda de um cenário e as cores de
//! um mesmo bicho gelado ou piscando saem certas sem montar o atlas de novo.
use super::super::{Chave, Renderer};
use super::{cruza, Tipo};
use crate::{Image, Point};
use tiny_skia::Pixmap;

#[path = "pilha_funda.rs"]
pub(crate) mod funda;
pub use funda::{StackLines, StackPart};

/// Floats por instância (13 vec4).
pub(crate) const PILHA: usize = 52;
/// Onde começa a tabela de caixas nos texels de dados: antes dela, 256 materiais × 3 tons.
const CAIXAS: u32 = 256 * 3;

/// O volume pronto para a GPU. Uma imagem só: o recorte cheio de cada fatia (o de corpo e o da
/// silhueta engordada do contorno) empacotados em prateleiras, e embaixo as linhas de dados (a
/// paleta e, de cada recorte, a caixa dele na célula da fatia e onde a célula começaria no atlas).
/// Guardar só o recorte, e não a célula inteira de cada fatia, ocupa umas 7 vezes menos.
///
/// Texel de corpo: R = material (1 a 255), G e B = normal (octaedro), A = 255; vazio é tudo 0.
#[derive(Clone)]
pub struct StackAtlas {
    pub(crate) image: Image,
    cel: u32,
    slices: u32,
    thickness: f32,
    border: u32,
    /// A primeira linha de dados.
    meta: u32,
    /// A primeira e a última fatia com alguma coisa.
    first: u32,
    last: u32,
}

/// Quantos texels de dados cada fatia tem: do corpo e do contorno, a caixa (lo, hi) e a origem (x, y), em
/// texels opacos (a imagem é pré-multiplicada: um alfa menor mudaria os outros bytes).
const POR_FATIA: usize = 8;
/// O quanto a origem (que pode ser negativa) é somada para caber em 16 bits.
const DESVIO: i32 = 1 << 12;

impl StackAtlas {
    /// `voxel(x, y, z)`: x e y de 0 a `side`, z de 0 (embaixo) a `slices`; o material (0 = vazio) e
    /// a normal (no espaço do objeto: x para a direita, y para a frente, z para cima; não precisa ter
    /// tamanho 1). `palette[m]` = os tons de luz, base e sombra do material `m`; `palette[0][0]` é a
    /// cor do contorno. `border`: texels de contorno de cada lado. `thickness`: quanto cada fatia
    /// sobe, em texels.
    pub fn new(side: u32, slices: u32, thickness: f32, border: u32, palette: &[[[u8; 3]; 3]], voxel: impl Fn(u32, u32, u32) -> (u8, [f32; 3])) -> StackAtlas {
        let cel = side + 2 * border;
        assert!(cel <= 255 && slices > 0 && palette.len() <= 256, "célula de até 255 texels, alguma fatia, até 256 materiais");
        let (n, l, c) = (slices as usize, side as usize, cel as usize);
        let b = border as i32;
        let mut disco: Vec<(i32, i32)> = (-b..=b).flat_map(|dy| (-b..=b).map(move |dx| (dx, dy))).filter(|&(dx, dy)| dx * dx + dy * dy <= b * b + b).collect();
        disco.sort_by_key(|&(dx, dy)| dx * dx + dy * dy);
        // cada fatia: os texels de corpo e de contorno na célula dela (c × c) e a caixa cheia de cada um
        let caixa = |m: &[u32], lado: usize| {
            let (mut a0, mut b0, mut a1, mut b1) = (lado, lado, 0, 0);
            for (i, _) in m.iter().enumerate().filter(|e| *e.1 != 0) {
                let (x, y) = (i % lado, i / lado);
                (a0, b0, a1, b1) = (a0.min(x), b0.min(y), a1.max(x + 1), b1.max(y + 1));
            }
            (a1 > a0).then_some([a0, b0, a1, b1])
        };
        let fatias: Vec<[(Vec<u32>, Option<[usize; 4]>); 2]> = (0..n)
            .map(|z| {
                let mut corpo = vec![0u32; c * c];
                for y in 0..l {
                    for x in 0..l {
                        let (m, nrm) = voxel(x as u32, y as u32, z as u32);
                        if m != 0 {
                            let [g, bb] = octaedro(nrm);
                            corpo[(y + border as usize) * c + x + border as usize] = u32::from_le_bytes([m, g, bb, 255]);
                        }
                    }
                }
                // Engorda em disco: acende onde algum texel cheio está a até `border` dali. Só na caixa do corpo mais a
                // borda, e o disco do mais perto ao mais longe (dentro da forma acha no primeiro).
                let mut contorno = vec![0u32; c * c];
                if let Some([x0, y0, x1, y1]) = caixa(&corpo, c) {
                    let cheio = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < c && (y as usize) < c && corpo[y as usize * c + x as usize] != 0;
                    for y in y0 as i32 - b..y1 as i32 + b {
                        for x in x0 as i32 - b..x1 as i32 + b {
                            if x >= 0 && y >= 0 && disco.iter().any(|&(dx, dy)| cheio(x + dx, y + dy)) {
                                contorno[y as usize * c + x as usize] = u32::from_le_bytes([0, 0, 0, 255]);
                            }
                        }
                    }
                }
                let (cc, kc) = (caixa(&corpo, c), caixa(&contorno, c));
                [(corpo, cc), (contorno, kc)]
            })
            .collect();
        // as prateleiras: os recortes do mais alto ao mais baixo, lado a lado até a largura
        let mut recortes: Vec<(usize, usize, [usize; 4])> = (fatias.iter().enumerate()).flat_map(|(z, f)| f.iter().enumerate().filter_map(move |(k, r)| Some((z, k, r.1?)))).collect();
        recortes.sort_by_key(|r| std::cmp::Reverse(r.2[3] - r.2[1]));
        let area: usize = recortes.iter().map(|r| (r.2[2] - r.2[0]) * (r.2[3] - r.2[1])).sum();
        let largo = recortes.iter().map(|r| r.2[2] - r.2[0]).max().unwrap_or(1);
        let w = (((area as f32 * 1.15).sqrt().ceil() as usize).max(largo)).max(64);
        let mut onde = vec![[(0usize, 0usize); 2]; n];
        let (mut x, mut y, mut alto) = (0, 0, 0);
        for &(z, k, [x0, y0, x1, y1]) in &recortes {
            if x + x1 - x0 > w {
                (x, y, alto) = (0, y + alto, 0);
            }
            onde[z][k] = (x, y);
            alto = alto.max(y1 - y0);
            x += x1 - x0;
        }
        let meta = y + alto;
        let dados = CAIXAS as usize + POR_FATIA * n;
        let h = meta + dados.div_ceil(w);
        let mut px = vec![0u8; w * h * 4];
        let dado = |px: &mut [u8], j: usize, v: [u8; 4]| {
            let (x, y) = (j % w, meta + j / w);
            px[(y * w + x) * 4..][..4].copy_from_slice(&v);
        };
        for (m, tons) in palette.iter().enumerate() {
            for (t, cor) in tons.iter().enumerate() {
                dado(&mut px, 3 * m + t, [cor[0], cor[1], cor[2], 255]);
            }
        }
        let (mut first, mut last) = (u32::MAX, 0);
        for (z, f) in fatias.iter().enumerate() {
            for (k, (texels, cx)) in f.iter().enumerate() {
                let j = CAIXAS as usize + POR_FATIA * z + 4 * k;
                let Some([x0, y0, x1, y1]) = *cx else {
                    dado(&mut px, j, [0; 4]);
                    dado(&mut px, j + 1, [0; 4]);
                    continue;
                };
                if k == 0 {
                    (first, last) = (first.min(z as u32), z as u32);
                }
                let (ax, ay) = onde[z][k];
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        px[((ay + yy - y0) * w + ax + xx - x0) * 4..][..4].copy_from_slice(&texels[yy * c + xx].to_le_bytes());
                    }
                }
                dado(&mut px, j, [x0 as u8, y0 as u8, 0, 255]);
                dado(&mut px, j + 1, [x1 as u8, y1 as u8, 0, 255]);
                // a célula da fatia começaria em (ax - x0, ay - y0): a caixa dela cai bem no recorte
                let [ox, oy] = [ax as i32 - x0 as i32 + DESVIO, ay as i32 - y0 as i32 + DESVIO].map(|v| v as u16);
                dado(&mut px, j + 2, [ox as u8, (ox >> 8) as u8, 0, 255]);
                dado(&mut px, j + 3, [oy as u8, (oy >> 8) as u8, 0, 255]);
            }
        }
        if first == u32::MAX {
            (first, last) = (0, 0);
        }
        let image = Image::from_rgba(w as u32, h as u32, px).expect("tamanho certo");
        StackAtlas { image, cel, slices, thickness, border, meta: meta as u32, first, last }
    }

    /// Lado da célula (o volume mais o contorno), em texels.
    pub fn cell(&self) -> u32 {
        self.cel
    }

    /// Quanto a imagem dele ocupa (na memória e na placa de vídeo), em bytes.
    pub fn bytes(&self) -> usize {
        self.image.0.data().len()
    }

    pub fn slices(&self) -> u32 {
        self.slices
    }

    pub fn thickness(&self) -> f32 {
        self.thickness
    }
}

/// A normal em dois bytes (octaedro), como o shader lê.
fn octaedro(n: [f32; 3]) -> [u8; 2] {
    let s = n[0].abs() + n[1].abs() + n[2].abs();
    if s <= 0.0 {
        return [128, 128];
    }
    let (mut x, mut y) = (n[0] / s, n[1] / s);
    if n[2] < 0.0 {
        (x, y) = ((1.0 - y.abs()) * x.signum(), (1.0 - x.abs()) * y.signum());
    }
    [x, y].map(|v| ((v * 0.5 + 0.5) * 255.0).round() as u8)
}

fn normal(g: f32, b: f32) -> [f32; 3] {
    let (x, y) = (g * 2.0 - 1.0, b * 2.0 - 1.0);
    let z = 1.0 - x.abs() - y.abs();
    let t = (-z).max(0.0);
    let n = [x - t * x.signum(), y - t * y.signum(), z];
    let m = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    n.map(|v| v / m)
}

/// Um objeto na tela. `yaw` e `pitch` em radianos: o giro (o da câmera mais o do objeto) e a
/// inclinação da câmera (0 = olhando reto de cima).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stack {
    /// Onde cai o meio da base, em pontos lógicos.
    pub at: Point,
    pub yaw: f32,
    pub pitch: f32,
    /// Pontos por texel.
    pub scale: f32,
    /// A pose: o topo deslocado (texels, no espaço do objeto), crescendo da base até lá.
    pub sway: (f32, f32),
    /// Quanto o objeto inteiro sobe (pulo), em texels.
    pub lift: f32,
    /// A silhueta de uma fatia a cada `outline_step` vai por baixo, de contorno (0 = sem contorno).
    pub outline_step: u32,
    /// De onde vem a luz, na tela: x para a direita, y para quem olha, z para cima.
    pub light: [f32; 3],
    /// Os limiares dos 3 tons, no produto da normal com a luz: luz acima de `tones[0]`, base acima
    /// de `tones[1]`, sombra no resto ([`Stack::TONES`] é a regra do gato 2.5D do jogo).
    pub tones: [f32; 2],
    /// A cor de cada ponto (o contorno também), em sRGB de 0 a 1: `cor × tint[0] + tint[1]`.
    /// [`Stack::NO_TINT`] deixa como está.
    pub tint: [[f32; 3]; 2],
    /// Onde começa a paleta desta pilha: o material `m` pinta com as cores de `m + palette` e o
    /// contorno com a de `palette` (0 = a paleta como veio). Um atlas com várias paletas seguidas
    /// troca as cores de um objeto inteiro sem outro atlas.
    pub palette: u32,
}

impl Stack {
    pub const TONES: [f32; 2] = [0.52, 0.04];
    pub const NO_TINT: [[f32; 3]; 2] = [[1.0; 3], [0.0; 3]];
}

impl Renderer {
    /// As pilhas de `atlas`, na ordem dada (de trás para a frente), cada uma com a sua chave na
    /// comparação de quadros. Na GPU são um lote só enquanto o atlas não muda.
    pub fn draw_stacks(&mut self, atlas: &StackAtlas, stacks: &[Stack]) {
        let usuario = self.transformacoes.last().copied().unwrap_or_default().0;
        let s = self.scale;
        let m = usuario.post_translate(-self.origem.x, -self.origem.y).post_scale(s, s);
        if self.pintando_gpu && !stacks.is_empty() {
            self.imagem_gpu_usada(atlas.image.1, &atlas.image.0);
        }
        for st in stacks {
            let mut k = Chave::new(15);
            k.h(atlas.image.1).h([st.at.x, st.at.y, st.yaw, st.pitch, st.scale, st.sway.0, st.sway.1, st.lift].map(f32::to_bits)).h(st.outline_step).h(st.light.map(f32::to_bits)).h(st.tones.map(f32::to_bits)).h(st.tint.map(|t| t.map(f32::to_bits))).h(st.palette);
            // A caixa lógica: o raio da célula em volta da base e a altura toda acima dela.
            let u = st.scale;
            let raio = (atlas.cel as f32 * 0.75 + st.sway.0.hypot(st.sway.1)) * u;
            let alto = ((atlas.slices as f32 * atlas.thickness + st.lift) * st.pitch.sin() + atlas.border as f32) * u + raio;
            let Some(caixa) = tiny_skia::Rect::from_ltrb(st.at.x - raio, st.at.y - alto, st.at.x + raio, st.at.y + raio).and_then(|r| r.transform(usuario)) else { continue };
            if self.gravando {
                self.gravou(crate::Rect { x: caixa.x(), y: caixa.y(), width: caixa.width(), height: caixa.height() }, k);
                continue;
            }
            if self.oculto(k) {
                continue;
            }
            let Some(px) = tiny_skia::Rect::from_ltrb(st.at.x - raio, st.at.y - alto, st.at.x + raio, st.at.y + raio).and_then(|r| r.transform(m)) else { continue };
            let dst = [px.x(), px.y(), px.width(), px.height()];
            if self.pintando_gpu {
                let rec = self.recorte_gpu();
                if cruza(dst, &rec) {
                    let d = instancia(atlas, st, m, &rec, self.opacidade);
                    self.gpu.por(Tipo::Pilha, atlas.image.1, &d);
                }
                continue;
            }
            // Sem GPU: a mesma conta, direto no pixmap (mistura em sRGB, como o resto do tiny-skia).
            let rec = match self.clips.last() {
                Some(c) => {
                    let (o, r) = (self.origem, c.rect);
                    let mut g = super::SEM_RECORTE;
                    g[..4].copy_from_slice(&[((r.x - o.x) * s).round(), ((r.y - o.y) * s).round(), ((r.x + r.width - o.x) * s).round(), ((r.y + r.height - o.y) * s).round()]);
                    g
                }
                None => super::SEM_RECORTE,
            };
            let d = instancia(atlas, st, m, &rec, self.opacidade);
            let (w, h) = (self.pixmap.width(), self.pixmap.height());
            let dados = self.pixmap.data_mut();
            pintar(&d, &atlas.image.0, [0, 0, w, h], |x, y, cor| {
                let i = (y as usize * w as usize + x as usize) * 4;
                let a = cor[3];
                for c in 0..4 {
                    let v = if c == 3 { 1.0 } else { cor[c] };
                    dados[i + c] = (v * a * 255.0 + dados[i + c] as f32 * (1.0 - a) + 0.5) as u8;
                }
            });
        }
    }
}

/// A instância como a GPU lê (os vec4 do `PilhaIn`). `m`: dos pontos lógicos para os pixels.
fn instancia(a: &StackAtlas, st: &Stack, m: tiny_skia::Transform, rec: &[f32; 12], opacidade: f32) -> [f32; PILHA] {
    let u = st.scale;
    let (q, sobe) = (st.pitch.cos(), st.pitch.sin());
    let (s, c) = st.yaw.sin_cos();
    // A luz da tela levada para o espaço do objeto (o giro ao contrário).
    let l = st.light;
    let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt().max(1e-6);
    let l = l.map(|v| v / n);
    let luz = [c * l[0] + s * l[1], -s * l[0] + c * l[1], l[2]];
    let mut d = [0.0; PILHA];
    d[..32].copy_from_slice(&[
        st.at.x, st.at.y, u, a.thickness * u * sobe,
        c, s, q, st.lift * u * sobe,
        st.sway.0, st.sway.1, a.slices as f32, st.outline_step as f32,
        luz[0], luz[1], luz[2], a.border as f32,
        a.cel as f32, 0.0, a.meta as f32, opacidade,
        a.first as f32, a.last as f32, sobe, st.palette as f32,
        m.sx, m.ky, m.kx, m.sy,
        m.tx, m.ty, 0.0, 0.0,
    ]);
    d[32..44].copy_from_slice(rec);
    let [k, o] = st.tint;
    d[44..].copy_from_slice(&[k[0], k[1], k[2], st.tones[0], o[0], o[1], o[2], st.tones[1]]);
    d
}

/// Quantas fatias (contorno mais corpo) a instância desenha.
fn fatias(d: &[f32]) -> u32 {
    let (passo, first, last) = (d[11] as u32, d[20] as u32, d[21] as u32);
    let contorno = if passo > 0 { 3 + (last - first) / passo } else { 0 };
    contorno + last - first + 1
}

/// Vértices de um lote de pilhas: 6 por fatia, pela instância que mais tem.
pub(crate) fn vertices(dados: &[f32]) -> u32 {
    6 * dados.chunks_exact(PILHA).map(fatias).max().unwrap_or(0)
}

/// A fatia `j` na ordem de desenhar: (é contorno, número da fatia, subida a mais em pontos).
/// Primeiro o contorno (a de baixo descida, uma a cada passo, a de cima subida) e depois o corpo.
fn escolha(d: &[f32], j: u32) -> Option<(bool, u32, f32)> {
    let (passo, first, last) = (d[11] as u32, d[20] as u32, d[21] as u32);
    let (u, q, sobe, borda) = (d[2], d[6], d[22], d[15]);
    let n = if passo > 0 { 3 + (last - first) / passo } else { 0 };
    if j < n {
        return Some(match j {
            0 => (true, first, -borda * u * q),
            _ if j == n - 1 => (true, last, borda * u * sobe),
            _ => (true, first + (j - 1) * passo, 0.0),
        });
    }
    let k = first + j - n;
    (k <= last).then_some((false, k, 0.0))
}

/// O texel `j` das linhas de dados.
fn dado(pm: &Pixmap, meta: u32, j: u32) -> [u8; 4] {
    let w = pm.width();
    let i = (((meta + j / w) * w + j % w) * 4) as usize;
    let p = pm.data();
    [p[i], p[i + 1], p[i + 2], p[i + 3]]
}

/// Pinta a instância `d` (pixels `area`: x0, y0, x1, y1) na CPU, como o shader: `por(x, y, cor)` com a
/// cor em sRGB sem pré-multiplicar e o alfa já com a opacidade e o recorte.
pub(crate) fn pintar(d: &[f32], pm: &Pixmap, area: [u32; 4], mut por: impl FnMut(u32, u32, [f32; 4])) {
    let (meta, op) = (d[18] as u32, d[19]);
    let rec = &d[32..44];
    for j in 0..fatias(d) {
        let Some((contorno, k, extra)) = escolha(d, j) else { continue };
        let e = POR_FATIA as u32 * k + if contorno { 4 } else { 0 };
        let (lo, hi) = (dado(pm, meta, CAIXAS + e), dado(pm, meta, CAIXAS + e + 1));
        let origem = [2, 3].map(|i| { let o = dado(pm, meta, CAIXAS + e + i); (o[0] as i32 + 256 * o[1] as i32 - DESVIO) as f32 });
        if hi[0] <= lo[0] || hi[1] <= lo[1] {
            continue;
        }
        let (lo, hi) = ([origem[0] + lo[0] as f32, origem[1] + lo[1] as f32], [origem[0] + hi[0] as f32, origem[1] + hi[1] as f32]);
        let f = |uv: [f32; 2]| ponto(d, k, extra, origem, uv);
        // texel → pixel é afim: a origem e as duas colunas, e a volta pela inversa
        let (o, ex, ey) = (f([0.0, 0.0]), f([1.0, 0.0]), f([0.0, 1.0]));
        let (a, b, c, dd) = (ex[0] - o[0], ex[1] - o[1], ey[0] - o[0], ey[1] - o[1]);
        let det = a * dd - b * c;
        if det.abs() < 1e-9 {
            continue;
        }
        let cantos = [f(lo), f([hi[0], lo[1]]), f([lo[0], hi[1]]), f(hi)];
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in cantos {
            (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
        }
        let px = |v: f32, min: u32, max: u32| ((v - 0.5).ceil().max(min as f32) as u32).min(max);
        for y in px(y0, area[1], area[3])..px(y1, area[1], area[3]) {
            for x in px(x0, area[0], area[2])..px(x1, area[0], area[2]) {
                let (cx, cy) = (x as f32 + 0.5 - o[0], y as f32 + 0.5 - o[1]);
                let uv = [(dd * cx - c * cy) / det, (a * cy - b * cx) / det];
                if uv[0] < lo[0] || uv[1] < lo[1] || uv[0] >= hi[0] || uv[1] >= hi[1] {
                    continue;
                }
                let cx = x as f32 + 0.5;
                let cy = y as f32 + 0.5;
                if cx < rec[0] || cy < rec[1] || cx >= rec[2] || cy >= rec[3] {
                    continue;
                }
                let (tx, ty) = (uv[0].floor() as u32, uv[1].floor() as u32);
                let i = ((ty * pm.width() + tx) * 4) as usize;
                let t = &pm.data()[i..i + 4];
                if t[3] < 128 {
                    continue;
                }
                let cor = dado(pm, meta, tom(contorno, t, [d[12], d[13], d[14]], [d[47], d[51]], d[23] as u32));
                let c = |i: usize| (cor[i] as f32 / 255.0 * d[44 + i] + d[48 + i]).clamp(0.0, 1.0);
                por(x, y, [c(0), c(1), c(2), op]);
            }
        }
    }
}

/// O índice da cor na paleta que começa em `paleta`: o contorno, ou o material no tom que a normal
/// pede (limiares `tons`).
fn tom(contorno: bool, t: &[u8], luz: [f32; 3], tons: [f32; 2], paleta: u32) -> u32 {
    if contorno {
        return paleta * 3;
    }
    let n = normal(t[1] as f32 / 255.0, t[2] as f32 / 255.0);
    let d = n[0] * luz[0] + n[1] * luz[1] + n[2] * luz[2];
    (t[0] as u32 + paleta) * 3 + if d > tons[0] { 0 } else if d > tons[1] { 1 } else { 2 }
}

/// O texel `uv` (do atlas) da fatia `k` na tela, em pixels: o `ponto` do shader.
fn ponto(d: &[f32], k: u32, extra: f32, origem: [f32; 2], uv: [f32; 2]) -> [f32; 2] {
    let (u, sobe_fatia, c, s, q, pulo) = (d[2], d[3], d[4], d[5], d[6], d[7]);
    let h = k as f32 / d[10];
    let (bx, by) = (d[8] * h, d[9] * h);
    let meio = d[16] / 2.0;
    let (ox, oy) = (uv[0] - origem[0] - meio, uv[1] - origem[1] - meio);
    let lx = d[0] + (c * bx - s * by) * u + u * (c * ox - s * oy);
    let ly = d[1] + (s * bx + c * by) * u * q - (k as f32 + 0.5) * sobe_fatia - pulo - extra + u * q * (s * ox + c * oy);
    [d[24] * lx + d[26] * ly + d[28], d[25] * lx + d[27] * ly + d[29]]
}

/// O pipeline das pilhas: triângulos soltos (6 vértices por fatia), mistura pré-multiplicada.
pub(crate) fn pipeline(device: &wgpu::Device, modulo: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let attrs: Vec<wgpu::VertexAttribute> = (0..PILHA as u32 / 4).map(|i| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16 * i as u64, shader_location: i }).collect();
    let mistura = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pilha"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: modulo, entry_point: "vs_pilha", buffers: &[wgpu::VertexBufferLayout { array_stride: 4 * PILHA as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs }] },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState { module: modulo, entry_point: "fs_pilha", targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState { color: mistura, alpha: mistura }), write_mask: wgpu::ColorWrites::ALL })] }),
        multiview: None,
    })
}

/// Vai junto do `SHADER` do pelicano (usa o `ndc`, o `recorte` e o `linear_pre` dele).
pub(crate) const SHADER: &str = r#"
struct PilhaIn {
    @location(0) base: vec4<f32>,
    @location(1) giro: vec4<f32>,
    @location(2) pose: vec4<f32>,
    @location(3) luz: vec4<f32>,
    @location(4) grade: vec4<f32>,
    @location(5) faixa: vec4<f32>,
    @location(6) m: vec4<f32>,
    @location(7) mt: vec4<f32>,
    @location(8) ret: vec4<f32>,
    @location(9) rr: vec4<f32>,
    @location(10) rk: vec4<f32>,
    // tinta: multiplica (rgb) e o limiar da luz; soma (rgb) e o limiar da base
    @location(11) tk: vec4<f32>,
    @location(12) to: vec4<f32>,
}
struct PilhaOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // a caixa cheia da fatia no atlas (lo, hi)
    @location(1) @interpolate(flat) caixa: vec4<f32>,
    // a luz no espaço do objeto e 1 se é contorno
    @location(2) @interpolate(flat) luz: vec4<f32>,
    // linha dos dados, opacidade e onde começa a paleta
    @location(3) @interpolate(flat) linha: vec4<f32>,
    @location(4) @interpolate(flat) ret: vec4<f32>,
    @location(5) @interpolate(flat) rr: vec4<f32>,
    @location(6) @interpolate(flat) rk: vec4<f32>,
    @location(7) @interpolate(flat) tk: vec4<f32>,
    @location(8) @interpolate(flat) to: vec4<f32>,
}

fn dado(j: u32, linha: u32) -> vec4<f32> {
    let w = textureDimensions(tex).x;
    return textureLoad(tex, vec2<i32>(i32(j % w), i32(linha + j / w)), 0);
}

// O texel `uv` da fatia `k` na tela, em pixels (o `ponto` da CPU).
fn ponto(q: PilhaIn, k: f32, extra: f32, origem: vec2<f32>, uv: vec2<f32>) -> vec2<f32> {
    let u = q.base.z;
    let c = q.giro.x;
    let s = q.giro.y;
    let a = q.giro.z;
    let h = k / q.pose.z;
    let b = q.pose.xy * h;
    let o = uv - origem - vec2<f32>(q.grade.x / 2.0);
    let lx = q.base.x + (c * b.x - s * b.y) * u + u * (c * o.x - s * o.y);
    let ly = q.base.y + (s * b.x + c * b.y) * u * a - (k + 0.5) * q.base.w - q.giro.w - extra + u * a * (s * o.x + c * o.y);
    return vec2<f32>(q.m.x * lx + q.m.z * ly + q.mt.x, q.m.y * lx + q.m.w * ly + q.mt.y);
}

@vertex fn vs_pilha(@builtin(vertex_index) vi: u32, q: PilhaIn) -> PilhaOut {
    var o: PilhaOut;
    o.pos = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    o.uv = vec2<f32>(0.0);
    o.caixa = vec4<f32>(0.0);
    o.luz = vec4<f32>(0.0);
    o.linha = vec4<f32>(0.0);
    o.ret = q.ret; o.rr = q.rr; o.rk = q.rk; o.tk = q.tk; o.to = q.to;
    let j = vi / 6u;
    let v = vi % 6u;
    let passo = u32(q.pose.w);
    let first = u32(q.faixa.x);
    let last = u32(q.faixa.y);
    var n = 0u;
    if passo > 0u { n = 3u + (last - first) / passo; }
    var contorno = false;
    var k = 0u;
    var extra = 0.0;
    if j < n {
        contorno = true;
        if j == 0u {
            k = first; extra = -q.luz.w * q.base.z * q.giro.z;
        } else if j == n - 1u {
            k = last; extra = q.luz.w * q.base.z * q.faixa.z;
        } else {
            k = first + (j - 1u) * passo;
        }
    } else {
        k = first + j - n;
        if k > last { return o; }
    }
    let linha = u32(q.grade.z);
    var e = 768u + 8u * k;
    if contorno { e += 4u; }
    // onde a célula da fatia começaria no atlas (16 bits por eixo, com o desvio de 4096)
    let ox = floor(dado(e + 2u, linha).xy * 255.0 + 0.5);
    let oy = floor(dado(e + 3u, linha).xy * 255.0 + 0.5);
    let origem = vec2<f32>(ox.x + 256.0 * ox.y, oy.x + 256.0 * oy.y) - vec2<f32>(4096.0);
    let lo = origem + floor(dado(e, linha).xy * 255.0 + 0.5);
    let hi = origem + floor(dado(e + 1u, linha).xy * 255.0 + 0.5);
    if hi.x <= lo.x || hi.y <= lo.y { return o; }
    // triângulos (0,1,2) e (2,1,3) dos cantos
    let cx = v == 1u || v == 4u || v == 5u;
    let cy = v == 2u || v == 3u || v == 5u;
    let uv = select(lo, hi, vec2<bool>(cx, cy));
    o.pos = ndc(ponto(q, f32(k), extra, origem, uv));
    o.uv = uv;
    o.caixa = vec4<f32>(lo, hi);
    o.luz = vec4<f32>(q.luz.xyz, select(0.0, 1.0, contorno));
    o.linha = vec4<f32>(q.grade.z, q.grade.w, q.faixa.w, 0.0);
    return o;
}

fn normal_oct(e: vec2<f32>) -> vec3<f32> {
    let v = e * 2.0 - 1.0;
    var n = vec3<f32>(v, 1.0 - abs(v.x) - abs(v.y));
    let t = max(-n.z, 0.0);
    n.x -= t * sign(n.x);
    n.y -= t * sign(n.y);
    return normalize(n);
}

@fragment fn fs_pilha(q: PilhaOut) -> @location(0) vec4<f32> {
    let c = q.pos.xy;
    let corte = recorte(c, q.ret, q.rr, q.rk);
    if corte <= 0.0 { discard; }
    let p = clamp(floor(q.uv), q.caixa.xy, q.caixa.zw - vec2<f32>(1.0));
    let t = textureLoad(tex, vec2<i32>(p), 0);
    if t.a < 0.5 { discard; }
    let paleta = u32(q.linha.z + 0.5);
    var j = paleta * 3u;
    if q.luz.w < 0.5 {
        let d = dot(normal_oct(t.gb), q.luz.xyz);
        var tom = 2u;
        if d > q.tk.w { tom = 0u; } else if d > q.to.w { tom = 1u; }
        j = (u32(t.r * 255.0 + 0.5) + paleta) * 3u + tom;
    }
    let cor = clamp(dado(j, u32(q.linha.x)).rgb * q.tk.rgb + q.to.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return linear_pre(vec4<f32>(cor, 1.0)) * q.linha.y * corte;
}
"#;

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normal_vai_e_volta() {
        for n in [[0.0, 0.0, 1.0], [0.6, -0.8, 0.0], [-0.3, 0.4, -0.866], [0.577, 0.577, 0.577]] {
            let [g, b] = octaedro(n);
            let v = normal(g as f32 / 255.0, b as f32 / 255.0);
            let dot: f32 = (0..3).map(|i| v[i] * n[i]).sum();
            assert!(dot > 0.99, "{n:?} voltou {v:?}");
        }
    }

    /// Uma coluna 2×2 de 3 fatias, a do meio vazia: as caixas, a faixa cheia, a paleta nos dados e os
    /// recortes empacotados (a origem de cada um leva a caixa dele até os texels certos no atlas).
    #[test]
    fn atlas_guarda_caixas_e_paleta() {
        let paleta = [[[58, 38, 42]; 3], [[250, 200, 100], [200, 150, 60], [120, 80, 30]]];
        let a = StackAtlas::new(4, 3, 1.0, 1, &paleta, |x, y, z| (if z != 1 && (1..3).contains(&x) && (1..3).contains(&y) { 1 } else { 0 }, [0.0, 0.0, 1.0]));
        assert_eq!((a.cel, a.first, a.last), (6, 0, 2));
        let pm = &a.image.0;
        assert_eq!(dado(pm, a.meta, 4)[..3], [200, 150, 60]);
        // fatia 0: cheia de 1 a 3 no volume, 2 a 4 na célula (o contorno de 1 texel em volta)
        let xy = |j: u32| { let d = dado(pm, a.meta, CAIXAS + j); [d[0], d[1]] };
        assert_eq!((xy(0), xy(1)), ([2, 2], [4, 4]));
        assert_eq!(xy(POR_FATIA as u32 + 1), [0, 0], "a fatia 1 é vazia");
        // o contorno da fatia 0 engorda 1 de cada lado
        assert_eq!((xy(4), xy(5)), ([1, 1], [5, 5]));
        // a caixa do corpo da fatia 2, levada pela origem dela, cai em texels de corpo (material 1)
        let j = CAIXAS + 2 * POR_FATIA as u32;
        let origem = [2, 3].map(|i| { let o = dado(pm, a.meta, j + i); o[0] as i32 + 256 * o[1] as i32 - DESVIO });
        for (x, y) in [(2, 2), (3, 3)] {
            let t = pm.pixel((origem[0] + x) as u32, (origem[1] + y) as u32).expect("dentro do atlas");
            assert_eq!((t.red(), t.alpha()), (1, 255));
        }
        // guarda só os recortes: 2 de corpo (2×2) e 2 de contorno (4×4), não 6 células de 6×6
        assert!(a.meta as usize * pm.width() as usize <= 64 * 4, "{}×{}", pm.width(), a.meta);
    }
}
