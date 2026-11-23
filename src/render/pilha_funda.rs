//! Pilhas fundas: as partes de objetos empilhados desenhadas com profundidade, para um objeto feito de várias partes
//! que mexem (o braço girando no ombro, a cabeça virando) e para objetos que se cruzam. Cada [`StackPart`] é uma
//! [`Stack`] com o giro 3D da parte em volta de um pivô e um deslocamento; [`Renderer::draw_stack_parts`] desenha uma
//! cena inteira de partes de uma vez, com um buffer de profundidade só dela (na GPU, uma passada à parte; na CPU, um
//! buffer de profundidade da chamada). O contorno de cada fatia fica bem atrás da cena (`ATRAS` texels): só aparece
//! contra o que está longe, como o contorno de fora das pilhas de antes (um pouco atrás só, ele ganhava da própria
//! parte embaixo das curvas e listrava o corpo).
//!
//! Fica de fora: o balanço (`Stack::sway`, ignorado aqui) e a ordem entre a cena funda e o resto (ela vai inteira na
//! ordem em que foi chamada, como qualquer desenho). O [`Renderer::draw_stacks`] de antes continua igual.
use super::*;

/// Quantos texels o contorno fica atrás da parte.
const ATRAS: f32 = 48.0;

/// Floats por instância: a pilha (13 vec4, com a espessura das fatias e a profundidade da base em 2 casas que ela
/// deixa livres: `grade.y` e `mt.z`) e mais 2 (o giro em quatérnio; a translação). Até 63: a placa pode aceitar no
/// máximo 255 bytes por instância (com 256, o pipeline falhava e todas as janelas caíam para a CPU).
pub(crate) const FUNDA: usize = PILHA + 8;
const _: () = assert!(FUNDA * 4 <= 255);

/// Uma parte de um objeto empilhado, com profundidade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackPart {
    /// Onde, como, com que luz e cores (o `sway` não vale aqui).
    pub stack: Stack,
    /// O giro da parte, do espaço dela para o do objeto (x para a direita, y para a frente, z para cima), em linhas.
    pub rot: [[f32; 3]; 3],
    /// Em volta de onde gira, em texels no espaço da parte (x e y a partir do meio da célula, z a partir da base).
    pub pivot: [f32; 3],
    /// Quanto anda depois do giro, em texels no espaço do objeto.
    pub offset: [f32; 3],
}

impl StackPart {
    pub const NO_ROTATION: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    /// A parte sem giro nem deslocamento: igual à pilha de antes, com profundidade.
    pub fn still(stack: Stack) -> StackPart {
        StackPart { stack, rot: Self::NO_ROTATION, pivot: [0.0; 3], offset: [0.0; 3] }
    }

    /// O giro de `ang` radianos em volta do eixo x (o braço indo para a frente com `ang` positivo leva +z para +y).
    pub fn rot_x(ang: f32) -> [[f32; 3]; 3] {
        let (s, c) = ang.sin_cos();
        [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]]
    }

    /// O giro de `ang` radianos em volta do eixo z (de cima, de x para y).
    pub fn rot_z(ang: f32) -> [[f32; 3]; 3] {
        let (s, c) = ang.sin_cos();
        [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
    }

    /// `a` depois de `b` (a matriz `a × b`).
    pub fn then(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
        std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
    }
}

/// O giro em quatérnio (x, y, z, w), como o shader usa.
fn quaternio(m: [[f32; 3]; 3]) -> [f32; 4] {
    let t = m[0][0] + m[1][1] + m[2][2];
    let q = if t > 0.0 {
        let s = (t + 1.0).sqrt() * 2.0;
        [(m[2][1] - m[1][2]) / s, (m[0][2] - m[2][0]) / s, (m[1][0] - m[0][1]) / s, 0.25 * s]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [0.25 * s, (m[0][1] + m[1][0]) / s, (m[0][2] + m[2][0]) / s, (m[2][1] - m[1][2]) / s]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [(m[0][1] + m[1][0]) / s, 0.25 * s, (m[1][2] + m[2][1]) / s, (m[0][2] - m[2][0]) / s]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [(m[0][2] + m[2][0]) / s, (m[1][2] + m[2][1]) / s, 0.25 * s, (m[1][0] - m[0][1]) / s]
    };
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    q.map(|v| v / n)
}

/// `v` girado pelo quatérnio `q` (a mesma conta do shader).
fn girar(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let cruz = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let u = [q[0], q[1], q[2]];
    let t = cruz(u, v).map(|x| x * 2.0);
    let c = cruz(u, t);
    std::array::from_fn(|i| v[i] + q[3] * t[i] + c[i])
}

/// A instância funda como a GPU lê: a da pilha com a luz levada para o espaço da parte, e os 3 vec4 a mais.
fn instancia_funda(a: &StackAtlas, p: &StackPart, m: tiny_skia::Transform, rec: &[f32; 12], opacidade: f32) -> [f32; FUNDA] {
    let st = &p.stack;
    let mut d = [0.0; FUNDA];
    d[..PILHA].copy_from_slice(&instancia(a, st, m, rec, opacidade));
    // a luz já está no espaço do objeto: no da parte é o giro ao contrário (a transposta)
    let l = [d[12], d[13], d[14]];
    let r = p.rot;
    for i in 0..3 {
        d[12 + i] = r[0][i] * l[0] + r[1][i] * l[1] + r[2][i] * l[2];
    }
    let (q, sobe) = (st.pitch.cos(), st.pitch.sin());
    // A profundidade (maior = mais perto da câmera), em pontos lógicos: a base no mapa, pela altura dela na tela.
    let base = st.at.y * sobe / q.max(0.05) + st.lift * st.scale * q;
    // girar em volta do pivô e andar = girar em volta da origem e andar `t = pivô + deslocamento − giro·pivô`
    let rp: [f32; 3] = std::array::from_fn(|i| (0..3).map(|k| r[i][k] * p.pivot[k]).sum());
    let t: [f32; 3] = std::array::from_fn(|i| p.pivot[i] + p.offset[i] - rp[i]);
    (d[17], d[30]) = (a.thickness, base);
    d[PILHA..PILHA + 4].copy_from_slice(&quaternio(r));
    d[PILHA + 4..].copy_from_slice(&[t[0], t[1], t[2], 0.0]);
    d
}

/// Vértices de um lote de pilhas fundas: 6 por fatia, pela instância que mais tem.
pub(crate) fn vertices_fundas(dados: &[f32]) -> u32 {
    6 * dados.chunks_exact(FUNDA).map(fatias).max().unwrap_or(0)
}

/// O texel `uv` da fatia `k` na tela, em pixels, e a profundidade dele (maior = mais perto): o `ponto_fundo` do shader.
fn ponto3(d: &[f32], k: u32, extra: f32, contorno: bool, origem: [f32; 2], uv: [f32; 2]) -> [f32; 3] {
    let (u, c, s, cq, pulo, sp) = (d[2], d[4], d[5], d[6], d[7], d[22]);
    let meio = d[16] / 2.0;
    let q = [d[PILHA], d[PILHA + 1], d[PILHA + 2], d[PILHA + 3]];
    let (t, esp, base) = ([d[PILHA + 4], d[PILHA + 5], d[PILHA + 6]], d[17], d[30]);
    let p = [uv[0] - origem[0] - meio, uv[1] - origem[1] - meio, (k as f32 + 0.5) * esp];
    let g = girar(q, p);
    let w: [f32; 3] = std::array::from_fn(|i| g[i] + t[i]);
    let (xr, yr) = (c * w[0] - s * w[1], s * w[0] + c * w[1]);
    let lx = d[0] + u * xr;
    let ly = d[1] + u * (cq * yr - sp * w[2]) - pulo - extra;
    let fundo = base + u * (yr * sp + w[2] * cq) - if contorno { ATRAS * u } else { 0.0 };
    [d[24] * lx + d[26] * ly + d[28], d[25] * lx + d[27] * ly + d[29], fundo]
}

/// Pinta a parte `d` (pixels `area`: x0, y0, x1, y1) na CPU, como o shader, testando e gravando a profundidade em `z`
/// (do tamanho da área, linha a linha; começa em `f32::MIN`). `por(x, y, cor)` como no [`pintar`] da pilha.
pub(crate) fn pintar_funda(d: &[f32], pm: &Pixmap, area: [u32; 4], z: &mut [f32], mut por: impl FnMut(u32, u32, [f32; 4])) {
    let (meta, op) = (d[18] as u32, d[19]);
    let rec = &d[32..44];
    let lz = (area[2] - area[0]) as usize;
    for j in 0..fatias(d) {
        let Some((contorno, k, extra)) = escolha(d, j) else { continue };
        let e = POR_FATIA as u32 * k + if contorno { 4 } else { 0 };
        let (lo, hi) = (dado(pm, meta, CAIXAS + e), dado(pm, meta, CAIXAS + e + 1));
        let origem = [2, 3].map(|i| {
            let o = dado(pm, meta, CAIXAS + e + i);
            (o[0] as i32 + 256 * o[1] as i32 - DESVIO) as f32
        });
        if hi[0] <= lo[0] || hi[1] <= lo[1] {
            continue;
        }
        let (lo, hi) = ([origem[0] + lo[0] as f32, origem[1] + lo[1] as f32], [origem[0] + hi[0] as f32, origem[1] + hi[1] as f32]);
        let f = |uv: [f32; 2]| ponto3(d, k, extra, contorno, origem, uv);
        // texel → pixel (e profundidade) é afim: a origem e as duas colunas, e a volta pela inversa
        let (o, ex, ey) = (f([0.0, 0.0]), f([1.0, 0.0]), f([0.0, 1.0]));
        let (a, b, c, dd) = (ex[0] - o[0], ex[1] - o[1], ey[0] - o[0], ey[1] - o[1]);
        let det = a * dd - b * c;
        if det.abs() < 1e-9 {
            continue;
        }
        let (zu, zv) = (ex[2] - o[2], ey[2] - o[2]);
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
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                if fx < rec[0] || fy < rec[1] || fx >= rec[2] || fy >= rec[3] {
                    continue;
                }
                let (tx, ty) = (uv[0].floor() as u32, uv[1].floor() as u32);
                let i = ((ty * pm.width() + tx) * 4) as usize;
                let t = &pm.data()[i..i + 4];
                if t[3] < 128 {
                    continue;
                }
                let fundo = o[2] + zu * uv[0] + zv * uv[1];
                let zi = (y - area[1]) as usize * lz + (x - area[0]) as usize;
                if fundo <= z[zi] {
                    continue;
                }
                z[zi] = fundo;
                let cor = dado(pm, meta, tom(contorno, t, [d[12], d[13], d[14]], [d[47], d[51]], d[23] as u32));
                let c = |i: usize| (cor[i] as f32 / 255.0 * d[44 + i] + d[48 + i]).clamp(0.0, 1.0);
                por(x, y, [c(0), c(1), c(2), op]);
            }
        }
    }
}

/// As linhas de contorno por dentro de uma cena funda: o ponto de uma parte que tem, a até `width` texels dele, outra
/// parte mais perto da câmera por mais de `threshold` texels vira a cor `color` (sRGB). A linha fica na parte de
/// trás, colada na da frente.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackLines {
    pub color: [u8; 3],
    pub threshold: f32,
    pub width: f32,
}

/// Floats das linhas por cena: a cor (linear, pré-multiplicada) e o limiar (pontos lógicos), o raio (pixels).
pub(crate) const LINHAS: usize = 8;

/// As linhas na CPU: `z` (do tamanho `w × h`, `f32::MIN` onde não há parte) e `por(x, y)` em cada ponto de linha
/// (na área: a partir do canto dela). `limiar` na unidade de `z`, `raio` em pixels.
pub(crate) fn linhas_cpu(z: &[f32], w: usize, h: usize, limiar: f32, raio: u32, mut por: impl FnMut(usize, usize)) {
    for y in 0..h {
        for x in 0..w {
            let zp = z[y * w + x];
            if zp == f32::MIN {
                continue;
            }
            let perto = (1..=raio as i64).any(|d| {
                [(d, 0), (-d, 0), (0, d), (0, -d)].iter().any(|&(dx, dy)| {
                    let (qx, qy) = ((x as i64 + dx).clamp(0, w as i64 - 1) as usize, (y as i64 + dy).clamp(0, h as i64 - 1) as usize);
                    z[qy * w + qx] > zp + limiar
                })
            });
            if perto {
                por(x, y);
            }
        }
    }
}

/// A cor sRGB em linear pré-multiplicada (opaca), como a GPU mistura.
fn linear_opaca(c: [u8; 3]) -> [f32; 4] {
    let l = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    [l(c[0]), l(c[1]), l(c[2]), 1.0]
}

impl Renderer {
    /// Uma cena de partes empilhadas com profundidade: as partes de todos os objetos dela juntas, em qualquer ordem
    /// (quem está na frente ganha ponto a ponto). Cada chamada é uma cena (a profundidade começa vazia).
    pub fn draw_stack_parts(&mut self, parts: &[(&StackAtlas, StackPart)]) {
        self.partes_fundas(parts, None);
    }

    /// O mesmo, e depois as linhas de contorno por dentro (`StackLines`; a escala é a da primeira parte).
    pub fn draw_stack_parts_lined(&mut self, parts: &[(&StackAtlas, StackPart)], lines: StackLines) {
        self.partes_fundas(parts, Some(lines));
    }

    fn partes_fundas(&mut self, parts: &[(&StackAtlas, StackPart)], linhas: Option<StackLines>) {
        let usuario = self.transformacoes.last().copied().unwrap_or_default().0;
        let s = self.scale;
        let m = usuario.post_translate(-self.origem.x, -self.origem.y).post_scale(s, s);
        let mut z: Vec<f32> = Vec::new();
        for (atlas, p) in parts {
            let st = &p.stack;
            let mut k = Chave::new(16);
            k.h(atlas.image.1).h([st.at.x, st.at.y, st.yaw, st.pitch, st.scale, st.lift].map(f32::to_bits)).h(st.outline_step).h(st.light.map(f32::to_bits)).h(st.tones.map(f32::to_bits)).h(st.tint.map(|t| t.map(f32::to_bits))).h(st.palette);
            k.h(p.rot.map(|l| l.map(f32::to_bits))).h(p.pivot.map(f32::to_bits)).h(p.offset.map(f32::to_bits));
            // A caixa lógica: tudo o que a parte alcança girada em volta do pivô e deslocada, em volta da base.
            let u = st.scale;
            let alcance = (atlas.cel as f32 + atlas.slices as f32 * atlas.thickness) * 1.0 + p.pivot.iter().map(|v| v.abs()).sum::<f32>() + p.offset.iter().map(|v| v.abs()).sum::<f32>();
            let raio = (alcance + atlas.border as f32) * u + st.lift.abs() * u;
            let Some(caixa) = tiny_skia::Rect::from_ltrb(st.at.x - raio, st.at.y - raio, st.at.x + raio, st.at.y + raio).and_then(|r| r.transform(usuario)) else { continue };
            if self.gravando {
                self.gravou(crate::Rect { x: caixa.x(), y: caixa.y(), width: caixa.width(), height: caixa.height() }, k);
                continue;
            }
            if self.oculto(k) {
                continue;
            }
            let Some(px) = tiny_skia::Rect::from_ltrb(st.at.x - raio, st.at.y - raio, st.at.x + raio, st.at.y + raio).and_then(|r| r.transform(m)) else { continue };
            let dst = [px.x(), px.y(), px.width(), px.height()];
            if self.pintando_gpu {
                self.imagem_gpu_usada(atlas.image.1, &atlas.image.0);
                let rec = self.recorte_gpu();
                if cruza(dst, &rec) {
                    let d = instancia_funda(atlas, p, m, &rec, self.opacidade);
                    self.gpu.por(Tipo::PilhaFunda, atlas.image.1, &d);
                }
                continue;
            }
            // Sem GPU: a mesma conta, direto no pixmap, com a profundidade desta chamada.
            let rec = match self.clips.last() {
                Some(c) => {
                    let (o, r) = (self.origem, c.rect);
                    let mut g = super::super::SEM_RECORTE;
                    g[..4].copy_from_slice(&[((r.x - o.x) * s).round(), ((r.y - o.y) * s).round(), ((r.x + r.width - o.x) * s).round(), ((r.y + r.height - o.y) * s).round()]);
                    g
                }
                None => super::super::SEM_RECORTE,
            };
            let d = instancia_funda(atlas, p, m, &rec, self.opacidade);
            let (w, h) = (self.pixmap.width(), self.pixmap.height());
            if z.is_empty() {
                z = vec![f32::MIN; (w * h) as usize];
            }
            let dados = self.pixmap.data_mut();
            pintar_funda(&d, &atlas.image.0, [0, 0, w, h], &mut z, |x, y, cor| {
                let i = (y as usize * w as usize + x as usize) * 4;
                let a = cor[3];
                for c in 0..4 {
                    let v = if c == 3 { 1.0 } else { cor[c] };
                    dados[i + c] = (v * a * 255.0 + dados[i + c] as f32 * (1.0 - a) + 0.5) as u8;
                }
            });
        }
        let (Some(l), Some(primeira)) = (linhas, parts.first()) else { return };
        let u = primeira.1.stack.scale;
        let (limiar, raio) = (l.threshold * u, (l.width * u * s).round().max(1.0) as u32);
        if self.gravando {
            return;
        }
        if self.pintando_gpu {
            let c = linear_opaca(l.color);
            self.gpu.linhas_fundas(&[c[0], c[1], c[2], c[3], limiar, raio as f32, 0.0, 0.0]);
            return;
        }
        if z.is_empty() {
            return;
        }
        let (w, h) = (self.pixmap.width() as usize, self.pixmap.height() as usize);
        let dados = self.pixmap.data_mut();
        linhas_cpu(&z, w, h, limiar, raio, |x, y| dados[(y * w + x) * 4..][..4].copy_from_slice(&[l.color[0], l.color[1], l.color[2], 255]));
    }
}

/// O formato do buffer de profundidade das passadas fundas.
pub(crate) const FORMATO_FUNDO: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// O pipeline das pilhas fundas: o das pilhas com 3 vec4 a mais e a profundidade (mais perto ganha).
pub(crate) fn pipeline_funda(device: &wgpu::Device, modulo: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let attrs: Vec<wgpu::VertexAttribute> = (0..FUNDA as u32 / 4).map(|i| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16 * i as u64, shader_location: i }).collect();
    let mistura = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pilha funda"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: modulo, entry_point: "vs_funda", buffers: &[wgpu::VertexBufferLayout { array_stride: 4 * FUNDA as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs }] },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: FORMATO_FUNDO, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState { module: modulo, entry_point: "fs_pilha", targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState { color: mistura, alpha: mistura }), write_mask: wgpu::ColorWrites::ALL })] }),
        multiview: None,
    })
}

/// O shader das linhas: um triângulo que cobre a tela, lendo a profundidade da passada funda.
const SHADER_LINHAS: &str = r#"
struct Globais { tela: vec4<f32> }
@group(0) @binding(0) var<uniform> g: Globais;
@group(1) @binding(0) var fundo: texture_depth_2d;
struct LinIn { @location(0) cor: vec4<f32>, @location(1) p: vec4<f32> }
struct LinOut { @builtin(position) pos: vec4<f32>, @location(0) @interpolate(flat) cor: vec4<f32>, @location(1) @interpolate(flat) p: vec4<f32> }
@vertex fn vs_linhas(@builtin(vertex_index) vi: u32, q: LinIn) -> LinOut {
    var o: LinOut;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.cor = q.cor;
    o.p = q.p;
    return o;
}
@fragment fn fs_linhas(q: LinOut) -> @location(0) vec4<f32> {
    let p = vec2<i32>(q.pos.xy);
    let dim = vec2<i32>(textureDimensions(fundo)) - vec2<i32>(1);
    let zp = textureLoad(fundo, p, 0);
    if zp >= 1.0 { discard; }
    // (o limiar está em pontos lógicos; a profundidade guardada é 0,5 − pontos × 1e-5)
    let limiar = q.p.x * 1e-5;
    let r = max(i32(q.p.y + 0.5), 1);
    for (var d = 1; d <= r; d++) {
        if perto(p + vec2<i32>(d, 0), dim, zp - limiar) || perto(p - vec2<i32>(d, 0), dim, zp - limiar) || perto(p + vec2<i32>(0, d), dim, zp - limiar) || perto(p - vec2<i32>(0, d), dim, zp - limiar) {
            return q.cor;
        }
    }
    discard;
}
// (um vizinho mais perto que `lim`; preso na tela)
fn perto(s: vec2<i32>, dim: vec2<i32>, lim: f32) -> bool {
    return textureLoad(fundo, clamp(s, vec2<i32>(0), dim), 0) < lim;
}
"#;

/// O grupo da profundidade para as linhas e o pipeline delas (num módulo de shader próprio).
pub(crate) fn pipeline_linhas(device: &wgpu::Device, globais: &wgpu::BindGroupLayout, format: wgpu::TextureFormat) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let modulo = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("linhas fundas"), source: wgpu::ShaderSource::Wgsl(SHADER_LINHAS.into()) });
    let camada = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("fundo"),
        entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[globais, &camada], push_constant_ranges: &[] });
    let attrs = [0u32, 1].map(|i| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16 * i as u64, shader_location: i });
    let mistura = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
    let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("linhas fundas"),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: &modulo, entry_point: "vs_linhas", buffers: &[wgpu::VertexBufferLayout { array_stride: 4 * LINHAS as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs }] },
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState { module: &modulo, entry_point: "fs_linhas", targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState { color: mistura, alpha: mistura }), write_mask: wgpu::ColorWrites::ALL })] }),
        multiview: None,
    });
    (p, camada)
}

/// Vai junto do `SHADER` da pilha (usa o `PilhaOut`, o `dado`, o `ndc` e o `fs_pilha` dele).
pub(crate) const SHADER: &str = r#"
struct FundaIn {
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
    @location(11) tk: vec4<f32>,
    @location(12) to: vec4<f32>,
    // o giro da parte (quatérnio) e a translação (a espessura está em grade.y; a profundidade da base, em mt.z)
    @location(13) gq: vec4<f32>,
    @location(14) tr: vec4<f32>,
}

fn girar_q(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

// O texel `uv` da fatia `k` na tela, em pixels, com a profundidade no z (o `ponto3` da CPU).
fn ponto_fundo(q: FundaIn, k: f32, extra: f32, contorno: bool, origem: vec2<f32>, uv: vec2<f32>) -> vec4<f32> {
    let u = q.base.z;
    let c = q.giro.x;
    let s = q.giro.y;
    let cq = q.giro.z;
    let sp = q.faixa.z;
    let o = uv - origem - vec2<f32>(q.grade.x / 2.0);
    let p = vec3<f32>(o.x, o.y, (k + 0.5) * q.grade.y);
    let w = girar_q(q.gq, p) + q.tr.xyz;
    let xr = c * w.x - s * w.y;
    let yr = s * w.x + c * w.y;
    let lx = q.base.x + u * xr;
    let ly = q.base.y + u * (cq * yr - sp * w.z) - q.giro.w - extra;
    var fundo = q.mt.z + u * (yr * sp + w.z * cq);
    if contorno { fundo -= 48.0 * u; }
    var r = ndc(vec2<f32>(q.m.x * lx + q.m.z * ly + q.mt.x, q.m.y * lx + q.m.w * ly + q.mt.y));
    r.z = clamp(0.5 - fundo * 1e-5, 0.0, 1.0);
    return r;
}

@vertex fn vs_funda(@builtin(vertex_index) vi: u32, q: FundaIn) -> PilhaOut {
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
    let ox = floor(dado(e + 2u, linha).xy * 255.0 + 0.5);
    let oy = floor(dado(e + 3u, linha).xy * 255.0 + 0.5);
    let origem = vec2<f32>(ox.x + 256.0 * ox.y, oy.x + 256.0 * oy.y) - vec2<f32>(4096.0);
    let lo = origem + floor(dado(e, linha).xy * 255.0 + 0.5);
    let hi = origem + floor(dado(e + 1u, linha).xy * 255.0 + 0.5);
    if hi.x <= lo.x || hi.y <= lo.y { return o; }
    let cx = v == 1u || v == 4u || v == 5u;
    let cy = v == 2u || v == 3u || v == 5u;
    let uv = select(lo, hi, vec2<bool>(cx, cy));
    o.pos = ponto_fundo(q, f32(k), extra, contorno, origem, uv);
    o.uv = uv;
    o.caixa = vec4<f32>(lo, hi);
    o.luz = vec4<f32>(q.luz.xyz, select(0.0, 1.0, contorno));
    o.linha = vec4<f32>(q.grade.z, q.grade.w, q.faixa.w, 0.0);
    return o;
}
"#;

#[cfg(test)]
mod testes {
    use super::*;

    /// O quatérnio gira igual à matriz.
    #[test]
    fn quaternio_gira_como_a_matriz() {
        for m in [StackPart::rot_x(0.7), StackPart::rot_z(-2.1), StackPart::then(StackPart::rot_z(1.2), StackPart::rot_x(-0.4)), StackPart::rot_x(3.1)] {
            let q = quaternio(m);
            for v in [[1.0, 0.0, 0.0], [0.3, -2.0, 0.5], [0.0, 0.0, 1.0]] {
                let a = girar(q, v);
                let b: [f32; 3] = std::array::from_fn(|i| (0..3).map(|k| m[i][k] * v[k]).sum());
                assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-4), "{m:?} {v:?}: {a:?} × {b:?}");
            }
        }
    }

    /// Duas caixas cruzadas na mesma cena: onde uma está na frente da outra, ganha ela, chamando em qualquer ordem.
    #[test]
    fn a_da_frente_ganha_em_qualquer_ordem() {
        let paleta = [[[0, 0, 0]; 3], [[255, 0, 0]; 3], [[0, 0, 255]; 3]];
        let caixa = |m: u8| StackAtlas::new(8, 8, 1.0, 1, &paleta, move |_, _, _| (m, [0.0, 0.0, 1.0]));
        let (verm, azul) = (caixa(1), caixa(2));
        let st = |y: f32| Stack { at: Point::new(32.0, y), yaw: 0.0, pitch: 0.9, scale: 2.0, sway: (0.0, 0.0), lift: 0.0, outline_step: 0, light: [0.0, 0.0, 1.0], tones: Stack::TONES, tint: Stack::NO_TINT, palette: 0 };
        // a azul mais embaixo na tela: mais perto da câmera
        let pintar = |ordem: [usize; 2]| {
            let partes = [(&verm, StackPart::still(st(30.0))), (&azul, StackPart::still(st(36.0)))];
            let mut r = Renderer::new();
            r.capture_bands(crate::Size::new(64.0, 64.0), 1.0, crate::Color::WHITE, |r| r.draw_stack_parts(&[partes[ordem[0]], partes[ordem[1]]])).expect("na lista")
        };
        let (a, b) = (pintar([0, 1]), pintar([1, 0]));
        assert_eq!(a, b, "a ordem da chamada não muda a imagem");
        // no meio da sobreposição, o azul
        let p = &a[(36 * 64 + 32) * 4..][..4];
        assert!(p[2] > 200 && p[0] < 50, "{p:?}");
    }
}
