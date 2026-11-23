//! Desenho na GPU. Entre o [`Renderer::begin_gpu`] e o `end`, os desenhos só anotam primitivas
//! numa [`Lista`] (recorte, opacidade e escala já resolvidos), e o [`Pintor`] pinta a lista com o
//! wgpu num passe só: retângulos (fundo, borda, degradê, sombra) e cápsulas dos traços por SDF,
//! glifos de um atlas e imagens de uma textura cada. O que ainda não tem caminho aqui (camadas,
//! vidro, caminhos com curva) marca [`Renderer::falta_gpu`] e o quadro inteiro sai na CPU.
use super::{Renderer, Pixmap};
use crate::{Background, Border, Color, Point, Rect, Shadow};
use cosmic_text::{Buffer, CacheKey, FontSystem, SwashCache, SwashContent};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[path = "faixas.rs"]
mod faixas;
#[path = "pilha.rs"]
pub(crate) mod pilha;
pub(crate) use faixas::contagem as contagem_faixas;

/// Lado do atlas de glifos (RGBA: a máscara vai como branco pré-multiplicado, o emoji com as cores).
const ATLAS: u32 = 2048;
/// A "textura" dos glifos nos lotes.
const DO_ATLAS: u64 = u64::MAX;
/// Floats por instância de cada tipo (vec4 cada 4).
const QUAD: usize = 52;
const GLIFO: usize = 32;
const TRACO: usize = 24;

/// O recorte de cada instância, em pixels: o retângulo (x0, y0, x1, y1, já arredondado como a
/// máscara da CPU), o recorte arredondado mais de dentro (x, y, w, h) e os raios dele (zero = nenhum).
pub(crate) const SEM_RECORTE: [f32; 12] = [-1e9, -1e9, 1e9, 1e9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

/// O recorte da GPU de um `push_clip`: `rect` é o cruzamento lógico, `own` o recorte pedido.
/// shortcut: só o arredondado mais de dentro corta os cantos; dois arredondados aninhados que se
/// cruzam nos cantos perdem o de fora (passar dois se aparecer).
pub(crate) fn recorte(fora: &[f32; 12], rect: Rect, own: Rect, raios: [f32; 4], s: f32) -> [f32; 12] {
    let mut g = *fora;
    let fim = |a: f32, b: f32| ((a + b) * s).round();
    g[..4].copy_from_slice(&[(rect.x * s).round(), (rect.y * s).round(), fim(rect.x, rect.width), fim(rect.y, rect.height)]);
    if raios.iter().any(|v| *v > 0.0) {
        g[4..8].copy_from_slice(&[own.x * s, own.y * s, own.width * s, own.height * s]);
        g[8..].copy_from_slice(&raios.map(|v| v * s));
    }
    g
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tipo {
    Quad,
    Glifo,
    Traco,
    /// Objetos empilhados ([`pilha`]): cada instância abre as fatias no vertex shader.
    Pilha,
    /// Partes empilhadas com profundidade ([`pilha::funda`]): as seguidas vão numa passada à parte, com o buffer dela.
    PilhaFunda,
    /// As linhas por dentro da cena funda de antes (`StackLines`): lê a profundidade dela.
    LinhasFundas,
    /// Desfoca o que já foi desenhado: o lote leva o índice em [`Lista::vidros`].
    Vidro,
}

/// Um vidro (`backdrop_blur`), em pixels do quadro: a mesma conta do `Renderer::blur` da CPU.
pub(crate) struct Vidro {
    /// A área copiada e desfocada: x, y, largura, altura.
    pub(crate) area: [u32; 4],
    /// O raio de cada uma das três caixas, em pixels cheios.
    pub(crate) k: u32,
    /// Onde o desfoque volta, com os cantos.
    pub(crate) forma: [f32; 4],
    pub(crate) raios: [f32; 4],
    pub(crate) rec: [f32; 12],
    pub(crate) alfa: f32,
}

impl Vidro {
    /// A redução (como o `desfocar` da CPU) e o raio das caixas na imagem reduzida.
    fn reducao(&self) -> (u32, u32) {
        let d = (self.k / 2).clamp(1, 4);
        (d, (self.k / d).max(1))
    }

    fn reduzida(&self) -> (u32, u32) {
        let d = self.reducao().0;
        (self.area[2].div_ceil(d), self.area[3].div_ceil(d))
    }
}

/// O quadro anotado. Fica no [`Renderer`] (as alocações servem ao próximo).
///
/// Cada instância traz tudo resolvido (pixels, recorte, opacidade já na cor, transformação já
/// nos pontos) e as imagens e glifos são `Arc` só de leitura: a lista é `Send + Sync` e dá para
/// repassar também na CPU, cada thread pintando a sua faixa de linhas do pixmap.
#[derive(Default)]
pub(crate) struct Lista {
    pub(crate) quads: Vec<f32>,
    pub(crate) glifos: Vec<f32>,
    pub(crate) tracos: Vec<f32>,
    pub(crate) pilhas: Vec<f32>,
    pub(crate) fundas: Vec<f32>,
    pub(crate) linhas: Vec<f32>,
    /// Na ordem do desenho: o tipo, a textura (glifos e imagens) e quantas instâncias.
    pub(crate) lotes: Vec<(Tipo, u64, u32)>,
    /// As imagens usadas no quadro, uma vez cada, pelo número que o lote leva: o [`Pintor`] sobe
    /// as que ainda não tem.
    pub(crate) usadas: Vec<(u64, Arc<Pixmap>)>,
    /// As usadas que são versões de outras ([`Image::changed_from`]), pelo número.
    pub(crate) versoes: Vec<(u64, crate::render::Versoes)>,
    pub(crate) vidros: Vec<Vidro>,
    vistas: HashSet<u64>,
    pub(crate) atlas: Atlas,
    tamanho: (u32, u32),
    fundo: Color,
    /// O maior lado de textura do aparelho (imagem maior cai na CPU). Zero até a janela dizer.
    pub(crate) max_textura: u32,
}

impl Lista {
    pub(crate) fn limpar(&mut self, w: u32, h: u32, fundo: Color) {
        self.quads.clear();
        self.glifos.clear();
        self.tracos.clear();
        self.pilhas.clear();
        self.fundas.clear();
        self.linhas.clear();
        self.lotes.clear();
        self.usadas.clear();
        self.versoes.clear();
        self.vidros.clear();
        self.vistas.clear();
        self.tamanho = (w, h);
        self.fundo = fundo;
    }

    /// Anota de que imagens `img` é versão, uma vez por quadro.
    pub(crate) fn versao(&mut self, img: &crate::Image) {
        if let Some(v) = &img.3 {
            if !self.versoes.iter().any(|x| x.0 == img.1) {
                self.versoes.push((img.1, v.clone()));
            }
        }
    }

    pub(crate) fn tamanho(&self) -> (u32, u32) {
        self.tamanho
    }

    pub(crate) fn vidro(&mut self, v: Vidro) {
        if !cruza(v.forma, &v.rec) {
            return;
        }
        // Um lote por vidro: cada um interrompe o passe para ler o que já foi desenhado.
        self.lotes.push((Tipo::Vidro, self.vidros.len() as u64, 1));
        self.vidros.push(v);
    }

    fn por(&mut self, tipo: Tipo, textura: u64, dados: &[f32]) {
        match self.lotes.last_mut() {
            Some((t, x, n)) if *t == tipo && *x == textura => *n += 1,
            _ => self.lotes.push((tipo, textura, 1)),
        }
        match tipo {
            Tipo::Quad => &mut self.quads,
            Tipo::Glifo => &mut self.glifos,
            Tipo::Traco => &mut self.tracos,
            Tipo::Pilha => &mut self.pilhas,
            Tipo::PilhaFunda => &mut self.fundas,
            Tipo::LinhasFundas => &mut self.linhas,
            Tipo::Vidro => unreachable!("o vidro entra por Lista::vidro"),
        }
        .extend_from_slice(dados);
    }

    /// As linhas por dentro da cena funda que acabou de ser anotada.
    pub(crate) fn linhas_fundas(&mut self, dados: &[f32]) {
        if self.lotes.last().is_some_and(|l| l.0 == Tipo::PilhaFunda) {
            self.por(Tipo::LinhasFundas, 0, dados);
        }
    }

    /// Um retângulo texturizado (`tipo`: 0 máscara do atlas, 1 glifo colorido, 2 imagem suavizada, 4 imagem nítida,
    /// 5 e 6 imagem transformada nítida e suavizada).
    #[allow(clippy::too_many_arguments)]
    /// `uv`: onde no atlas e o índice do bitmap do glifo.
    fn textura(&mut self, textura: u64, dst: [f32; 4], uv: [f32; 3], cor: [f32; 4], raios: [f32; 4], tipo: f32, rec: &[f32; 12]) {
        if !cruza(dst, rec) {
            return;
        }
        let mut d = [0.0; GLIFO];
        d[..4].copy_from_slice(&dst);
        d[4..7].copy_from_slice(&uv);
        d[8..12].copy_from_slice(&cor);
        d[12..16].copy_from_slice(&raios);
        d[16] = tipo;
        d[20..].copy_from_slice(rec);
        self.por(Tipo::Glifo, textura, &d);
    }
}

/// `r` (x, y, w, h) toca o retângulo do recorte.
fn cruza(r: [f32; 4], rec: &[f32; 12]) -> bool {
    r[2] > 0.0 && r[3] > 0.0 && r[0] < rec[2] && r[1] < rec[3] && r[0] + r[2] > rec[0] && r[1] + r[3] > rec[1]
}

// A lista vai para as threads que pintam as faixas.
const _: fn() = || {
    fn ok<T: Send + Sync>() {}
    ok::<Lista>();
};

#[cfg(test)]
mod testes {
    use crate::{Color, Image, Point, Rect, Renderer, Size, Transform};

    /// Os pedaços da mesma imagem (as fatias de um objeto empilhado) vão num lote só: uma chamada de desenho.
    #[test]
    fn pedacos_num_lote_so() {
        let img = Image::from_rgba(16, 8, vec![200; 16 * 8 * 4]).expect("tamanho confere");
        let pecas: Vec<(Rect, Transform)> = (0..2).map(|i| (Rect { x: i as f32 * 8.0, y: 0.0, width: 8.0, height: 8.0 }, Transform::rotate_around(30.0, Point::new(4.0, 4.0)).then(Transform::translate(10.0 + i as f32 * 20.0, 10.0)))).collect();
        let mut r = Renderer::new();
        r.capture_bands(Size::new(64.0, 40.0), 1.0, Color::BLACK, |r| r.draw_image_pieces_nearest(&img, &pecas)).expect("na lista");
        assert_eq!(r.gpu.lotes.len(), 1);
        assert_eq!(r.gpu.glifos.len(), 2 * super::GLIFO);
    }
}

#[derive(Clone, Copy)]
struct Glifo {
    /// Em [`Atlas::bitmaps`] (a instância leva este número também).
    idx: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    left: i32,
    top: i32,
    cor: bool,
}

/// Onde cada glifo está no atlas. Prateleiras simples; quando enche, esvazia tudo (e aquele quadro
/// vai para a CPU, porque o que já foi anotado aponta para o atlas velho).
#[derive(Default)]
pub(crate) struct Atlas {
    mapa: HashMap<CacheKey, Option<Glifo>>,
    /// Os pixels de cada glifo, como saem do swash (1 byte de cobertura, ou RGBA no emoji): o
    /// que sobe para a textura, e o que um repasse na CPU copia.
    pub(crate) bitmaps: Vec<Arc<[u8]>>,
    /// (y, altura, primeiro x livre).
    prateleiras: Vec<[u32; 3]>,
    topo: u32,
    /// Rasterizados e ainda não enviados à textura (índices em `bitmaps`).
    envios: Vec<Glifo>,
}

impl Atlas {
    /// `Err` = encheu (já esvaziado).
    fn glifo(&mut self, fonts: &mut FontSystem, swash: &mut SwashCache, k: CacheKey) -> Result<Option<Glifo>, ()> {
        if let Some(g) = self.mapa.get(&k) {
            return Ok(*g);
        }
        // Sem o cache do swash: a cópia fica só aqui (e na GPU).
        let img = swash.get_image_uncached(fonts, k).filter(|i| i.placement.width > 0 && i.placement.height > 0);
        let Some(img) = img.filter(|i| !matches!(i.content, SwashContent::SubpixelMask)) else {
            self.mapa.insert(k, None);
            return Ok(None);
        };
        let (w, h) = (img.placement.width, img.placement.height);
        // Maior que o atlas inteiro: nunca cabe, e esvaziar o atlas por ele só faria tudo de novo a cada quadro.
        if w >= ATLAS || h >= ATLAS {
            return Err(());
        }
        let cor = matches!(img.content, SwashContent::Color);
        let Some((x, y)) = self.lugar(w, h) else {
            self.mapa.clear();
            self.bitmaps.clear();
            self.prateleiras.clear();
            self.topo = 0;
            self.envios.clear();
            return Err(());
        };
        let g = Glifo { idx: self.bitmaps.len() as u32, x, y, w, h, left: img.placement.left, top: img.placement.top, cor };
        self.bitmaps.push(img.data.into());
        self.envios.push(g);
        self.mapa.insert(k, Some(g));
        Ok(Some(g))
    }

    fn lugar(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w >= ATLAS || h >= ATLAS {
            return None;
        }
        // Uma prateleira da altura quase igual (até 1/4 a mais); 1 px de folga entre glifos.
        if let Some(p) = self.prateleiras.iter_mut().find(|p| p[1] >= h && p[1] <= h + h / 4 + 2 && p[2] + w <= ATLAS) {
            p[2] += w + 1;
            return Some((p[2] - w - 1, p[0]));
        }
        if self.topo + h > ATLAS {
            return None;
        }
        self.prateleiras.push([self.topo, h, w + 1]);
        self.topo += h + 1;
        Some((0, self.topo - h - 1))
    }
}

/// Os glifos de `b` como retângulos do atlas: o mesmo laço (e os mesmos pixels) de `pintar_glifos`.
/// `false` se o atlas encheu.
#[allow(clippy::too_many_arguments)]
pub(crate) fn glifos(l: &mut Lista, b: &Buffer, fonts: &mut FontSystem, swash: &mut SwashCache, rec: &[f32; 12], s: f32, at: Point, color: Color, opac: f32) -> bool {
    // Só o y é arredondado (a linha de base fica nítida); o x vai com a fração para o cosmic-text
    // escolher a variação de subpixel do glifo. Arredondar a origem errava até meio pixel em cada
    // texto, e palavra desenhada em pedaços (a página do FeatherDocs) ganhava espaço no meio.
    let (fx, oy) = (at.x * s, (at.y * s).round() as i32);
    let cor = cosmic_text::Color::rgba((color.r * 255.0) as u8, (color.g * 255.0) as u8, (color.b * 255.0) as u8, (color.a * 255.0) as u8);
    for run in b.layout_runs() {
        let topo = oy + run.line_top as i32;
        if topo as f32 > rec[3] || ((topo + run.line_height.ceil() as i32 + 1) as f32) < rec[1] {
            continue;
        }
        for g in run.glyphs {
            let cor_g = g.color_opt.unwrap_or(cor);
            let fator = if g.color_opt.is_some() { (cor_g.a() as f32 * opac).round() } else { cor_g.a() as f32 };
            if fator == 0.0 {
                continue;
            }
            let fis = g.physical((fx, 0.), 1.0);
            let gl = match l.atlas.glifo(fonts, swash, fis.cache_key) {
                Ok(Some(gl)) => gl,
                Ok(None) => continue,
                Err(()) => return false,
            };
            let (x, y) = (fis.x + gl.left, oy + run.line_y as i32 + fis.y - gl.top);
            let c = [linear(cor_g.r() as f32 / 255.0), linear(cor_g.g() as f32 / 255.0), linear(cor_g.b() as f32 / 255.0), fator / 255.0];
            l.textura(DO_ATLAS, [x as f32, y as f32, gl.w as f32, gl.h as f32], [gl.x as f32, gl.y as f32, gl.idx as f32], c, [0.0; 4], if gl.cor { 1.0 } else { 0.0 }, rec);
        }
    }
    true
}

/// A cor como entra na lista: rgb linear (a mistura é em espaço linear, como no Iced), alfa igual.
fn rgba(c: Color) -> [f32; 4] {
    let [r, g, b, a] = [c.r, c.g, c.b, c.a].map(|v| v.clamp(0.0, 1.0));
    [linear(r), linear(g), linear(b), a]
}

/// sRGB (0..1) para linear.
pub(crate) fn linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// Linear (0..1) para sRGB.
pub(crate) fn srgb(v: f32) -> f32 {
    if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

impl Renderer {
    /// O recorte de agora, cortado no quadro: o que cai fora dele (um mapa com zoom maior que a janela) nem vira
    /// instância.
    fn recorte_gpu(&self) -> [f32; 12] {
        let mut r = self.clips.last().map_or(SEM_RECORTE, |c| c.gpu);
        let (w, h) = self.gpu.tamanho;
        (r[0], r[1], r[2], r[3]) = (r[0].max(0.0), r[1].max(0.0), r[2].min(w as f32), r[3].min(h as f32));
        r
    }

    /// O `fill_quad` na GPU: a sombra é uma instância à parte, embaixo.
    pub(super) fn quad_gpu(&mut self, r: Rect, bg: Option<&Background>, border: Border, shadow: Option<Shadow>) {
        let (s, op, rec) = (self.scale, self.opacidade, self.recorte_gpu());
        let caixa = [r.x * s, r.y * s, r.width * s, r.height * s];
        let raios = border.radius.map(|v| v * s);
        if let Some(sh) = shadow.filter(|sh| sh.color.a > 0.0 && caixa[2] >= 1.0 && caixa[3] >= 1.0) {
            // O desfoque do Iced: `blur` é até onde a sombra vai, em pixels da tela.
            let blur = sh.blur * s;
            let mut q = [0.0; QUAD];
            q[..4].copy_from_slice(&[(r.x + sh.offset.x) * s, (r.y + sh.offset.y) * s, caixa[2], caixa[3]]);
            q[4..8].copy_from_slice(&raios);
            q[8..12].copy_from_slice(&rgba(sh.color.alpha(sh.color.a * op)));
            q[32..36].copy_from_slice(&[0.0, 2.0, 0.0, blur]);
            q[36..40].copy_from_slice(&caixa);
            q[40..].copy_from_slice(&rec);
            let m = blur + 1.0;
            if cruza([q[0] - m, q[1] - m, q[2] + 2.0 * m, q[3] + 2.0 * m], &rec) {
                self.gpu.por(Tipo::Quad, 0, &q);
            }
        }
        let borda = if border.width > 0.0 && border.color.a > 0.0 { border.width * s } else { 0.0 };
        if caixa[2] <= 0.0 || caixa[3] <= 0.0 || (bg.is_none() && borda == 0.0) || !cruza(caixa, &rec) {
            return;
        }
        let mut q = [0.0; QUAD];
        q[..4].copy_from_slice(&caixa);
        q[4..8].copy_from_slice(&raios);
        q[28..32].copy_from_slice(&rgba(border.color.alpha(border.color.a * op)));
        let tipo = match bg {
            None => 0.0,
            Some(Background::Color(c)) => {
                q[8..12].copy_from_slice(&rgba(c.alpha(c.a * op)));
                0.0
            }
            Some(Background::Linear { angle, stops }) => {
                // A reta do degradê da CPU: do centro, meia projeção da caixa para cada lado.
                let (dx, dy) = (angle.cos(), angle.sin());
                let meio = (caixa[2] * dx.abs() + caixa[3] * dy.abs()) / 2.0;
                let (cx, cy) = (caixa[0] + caixa[2] / 2.0, caixa[1] + caixa[3] / 2.0);
                let g = if meio > 0.0 { 1.0 / (2.0 * meio) } else { 0.0 };
                q[36..40].copy_from_slice(&[cx - dx * meio, cy - dy * meio, dx * g, dy * g]);
                for (i, (t, c)) in stops.iter().take(4).enumerate() {
                    q[8 + 4 * i..12 + 4 * i].copy_from_slice(&rgba(c.alpha(c.a * op)));
                    q[24 + i] = *t;
                }
                q[34] = stops.len().min(4) as f32;
                1.0
            }
        };
        q[32] = borda;
        q[33] = tipo;
        q[40..].copy_from_slice(&rec);
        self.gpu.por(Tipo::Quad, 0, &q);
    }

    /// Imagem (pré-multiplicada) esticada em `dst` (pixels), com os cantos `raios` (pixels).
    pub(super) fn imagem_gpu(&mut self, id: u64, pm: &Arc<Pixmap>, dst: [f32; 4], raios: [f32; 4]) {
        let rec = self.recorte_gpu();
        if !cruza(dst, &rec) {
            return;
        }
        if self.gpu.vistas.insert(id) {
            self.gpu.usadas.push((id, pm.clone()));
        }
        let op = self.opacidade;
        self.gpu.textura(id, dst, [0.0; 3], [1.0, 1.0, 1.0, op], raios, 2.0, &rec);
    }

    /// Imagem sem suavizar esticada em `dst` (pixels): cada texel vira um bloco.
    pub(super) fn imagem_gpu_nitida(&mut self, id: u64, pm: &Arc<Pixmap>, dst: [f32; 4]) {
        let rec = self.recorte_gpu();
        if !cruza(dst, &rec) {
            return;
        }
        if self.gpu.vistas.insert(id) {
            self.gpu.usadas.push((id, pm.clone()));
        }
        let op = self.opacidade;
        self.gpu.textura(id, dst, [0.0; 3], [1.0, 1.0, 1.0, op], [0.0; 4], 4.0, &rec);
    }

    /// Imagem levada pela transformação `t` (de texels para pixels): um quadrilátero qualquer. A instância
    /// leva a caixa dele na tela e a volta (pixel para texel): `raios` = sx, ky, kx, sy; `uv` = tx, ty.
    pub(super) fn imagem_gpu_transformada(&mut self, id: u64, pm: &Arc<Pixmap>, t: tiny_skia::Transform, nitida: bool) {
        let Some(volta) = t.invert() else { return };
        let Some(b) = tiny_skia::Rect::from_xywh(0.0, 0.0, pm.width() as f32, pm.height() as f32).and_then(|c| c.transform(t)) else { return };
        let dst = [b.left().floor() - 1.0, b.top().floor() - 1.0, b.width().ceil() + 3.0, b.height().ceil() + 3.0];
        let rec = self.recorte_gpu();
        if !cruza(dst, &rec) {
            return;
        }
        if self.gpu.vistas.insert(id) {
            self.gpu.usadas.push((id, pm.clone()));
        }
        let op = self.opacidade;
        let inv = [volta.sx, volta.ky, volta.kx, volta.sy];
        self.gpu.textura(id, dst, [volta.tx, volta.ty, 0.0], [1.0, 1.0, 1.0, op], inv, if nitida { 5.0 } else { 6.0 }, &rec);
    }

    /// A imagem vai para a GPU com o quadro (uma vez, por mais que seja desenhada).
    pub(super) fn imagem_gpu_usada(&mut self, id: u64, pm: &Arc<Pixmap>) {
        if self.gpu.vistas.insert(id) {
            self.gpu.usadas.push((id, pm.clone()));
        }
    }

    /// Só o retângulo `sub` (x0, y0, x1, y1 em texels) da imagem, levado por `t` (texels da imagem
    /// inteira para pixels), nítido: o tipo 5 com o recorte em `uv.zw` e `params.yz`. Quem chama já pôs a
    /// imagem em `usadas` (uma vez para todas as peças).
    pub(super) fn imagem_gpu_peca(&mut self, id: u64, t: tiny_skia::Transform, sub: [f32; 4]) {
        let Some(volta) = t.invert() else { return };
        let Some(b) = tiny_skia::Rect::from_ltrb(sub[0], sub[1], sub[2], sub[3]).and_then(|c| c.transform(t)) else { return };
        let dst = [b.left().floor() - 1.0, b.top().floor() - 1.0, b.width().ceil() + 3.0, b.height().ceil() + 3.0];
        let rec = self.recorte_gpu();
        if !cruza(dst, &rec) {
            return;
        }
        let mut d = [0.0; GLIFO];
        d[..4].copy_from_slice(&dst);
        d[4..8].copy_from_slice(&[volta.tx, volta.ty, sub[0], sub[1]]);
        d[8..12].copy_from_slice(&[1.0, 1.0, 1.0, self.opacidade]);
        d[12..16].copy_from_slice(&[volta.sx, volta.ky, volta.kx, volta.sy]);
        d[16..19].copy_from_slice(&[5.0, sub[2], sub[3]]);
        d[20..].copy_from_slice(&rec);
        self.gpu.por(Tipo::Glifo, id, &d);
    }

    /// Cada reta uma cápsula (pontas redondas: as junções saem da sobreposição). Lógicos.
    pub(super) fn traco_gpu(&mut self, retas: impl Iterator<Item = (Point, Point)>, largura: f32, cor: Color) {
        let (s, rec) = (self.scale, self.recorte_gpu());
        let meia = largura * s / 2.0;
        let mut d = [0.0; TRACO];
        d[4] = meia;
        d[8..12].copy_from_slice(&rgba(cor));
        d[12..].copy_from_slice(&rec);
        for (a, b) in retas {
            let (ax, ay, bx, by) = (a.x * s, a.y * s, b.x * s, b.y * s);
            let caixa = [ax.min(bx) - meia - 1.0, ay.min(by) - meia - 1.0, (ax - bx).abs() + 2.0 * meia + 2.0, (ay - by).abs() + 2.0 * meia + 2.0];
            if !cruza(caixa, &rec) {
                continue;
            }
            d[..4].copy_from_slice(&[ax, ay, bx, by]);
            self.gpu.por(Tipo::Traco, 0, &d);
        }
    }
}

impl Renderer {
    /// A lista do quadro (gravada entre `begin_gpu` e `end`) pintada na CPU, em faixas de
    /// linhas, uma thread cada; só nas `regioes` lógicas (`None` = o quadro todo).
    pub(crate) fn pintar_lista_cpu(&mut self, regioes: Option<&[Rect]>) {
        let (w, h) = self.gpu.tamanho;
        let mesmo = self.pixmap.width() == w && self.pixmap.height() == h;
        let s = self.scale;
        let areas: Vec<[u32; 4]> = match regioes.filter(|_| mesmo) {
            Some(rs) => rs
                .iter()
                .map(|r| {
                    let px = |v: f32, max: u32| (v * s).max(0.0).min(max as f32);
                    [px(r.x, w).floor() as u32, px(r.y, h).floor() as u32, px(r.x + r.width, w).ceil() as u32, px(r.y + r.height, h).ceil() as u32]
                })
                .collect(),
            None => {
                if !mesmo {
                    self.pixmap = Pixmap::new(w, h).expect("tamanho válido");
                }
                vec![[0, 0, w, h]]
            }
        };
        for a in &areas {
            faixas::pintar(&self.gpu, &mut self.pixmap, *a);
        }
        self.mudou = if regioes.is_some() && mesmo { areas.iter().map(|a| (a[1], a[3])).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1))) } else { None };
    }

    /// O quadro de `desenhar` pela lista pintada em faixas na CPU (para comparar com o tiny-skia,
    /// `tests/gpu.rs`). `Err` com o motivo se a lista não representa o quadro.
    #[doc(hidden)]
    pub fn capture_bands(&mut self, size: crate::Size, scale: f32, fundo: Color, desenhar: impl FnOnce(&mut Renderer)) -> Result<Vec<u8>, &'static str> {
        self.falta_gpu = None;
        if self.gpu.max_textura == 0 {
            self.gpu.max_textura = SEM_LIMITE_TEXTURA;
        }
        self.begin_gpu(size, scale, fundo);
        desenhar(self);
        self.end();
        if let Some(m) = self.falta_gpu {
            return Err(m);
        }
        if !self.gpu.vidros.is_empty() {
            return Err("vidro (as faixas não desfocam)");
        }
        self.pintar_lista_cpu(None);
        Ok(self.pixmap.data().to_vec())
    }
}

/// O "lado máximo de textura" sem GPU: a lista pintada na CPU não tem limite de textura; o que
/// segura imagens e caminhos enormes são os limites de área de cada desenho.
pub(crate) const SEM_LIMITE_TEXTURA: u32 = 16384;

/// Quanto as texturas das imagens podem ocupar na GPU antes de soltar as que não estão na tela. Um jogo
/// com quadros de 17 MB, chão de 50 MB e uma centena de atlas de objetos empilhados passa de 300 MB.
const TEXTURAS_MAX: usize = 512 << 20;

/// Os pipelines e o que fica na GPU de um quadro para o outro (atlas, texturas das imagens).
/// Um só para todas as janelas, como o `Renderer`.
pub(crate) struct Pintor {
    quad: wgpu::RenderPipeline,
    glifo: wgpu::RenderPipeline,
    traco: wgpu::RenderPipeline,
    pilha: wgpu::RenderPipeline,
    pilha_funda: wgpu::RenderPipeline,
    linhas: wgpu::RenderPipeline,
    camada_fundo: wgpu::BindGroupLayout,
    /// O buffer de profundidade das passadas fundas, o grupo dele para as linhas e o tamanho (feito no primeiro quadro
    /// que tem uma).
    fundo: Option<(wgpu::TextureView, wgpu::BindGroup, (u32, u32))>,
    globais: wgpu::Buffer,
    grupo: wgpu::BindGroup,
    camada_textura: wgpu::BindGroupLayout,
    amostrador: wgpu::Sampler,
    atlas: wgpu::Texture,
    grupo_atlas: wgpu::BindGroup,
    buffers: [Option<wgpu::Buffer>; 6],
    /// Pelo número da imagem (ou da rasterização do SVG): a textura, o último quadro em que apareceu
    /// e os bytes dela.
    /// a textura de cada imagem: o grupo, o último quadro em que foi usada, os bytes e a imagem (que,
    /// liberada, solta a textura)
    texturas: HashMap<u64, (wgpu::BindGroup, u64, usize, std::sync::Weak<Pixmap>, wgpu::Texture)>,
    quadro: u64,
    borrar: wgpu::RenderPipeline,
    formato: wgpu::TextureFormat,
    /// Instâncias dos passes do vidro: os parâmetros de cada passe e a volta (um glifo do tipo 3).
    buffers_vidro: [Option<wgpu::Buffer>; 2],
    /// De onde os pedaços das imagens em versões ([`Image::changed_from`]) sobem: um buffer só por quadro, em vez de
    /// uma cópia (e um buffer de passagem) por retângulo.
    pedacos: Option<wgpu::Buffer>,
    /// Cópia da área (tamanho cheio) e as duas reduzidas que os passes alternam, com o tamanho de cada.
    copias: Option<Copias>,
}

/// As três texturas do vidro (a cópia cheia e as duas reduzidas) e os tamanhos alocados.
type Copias = ([(wgpu::Texture, wgpu::BindGroup); 3], (u32, u32), (u32, u32));

const BORRAR: usize = 8;

impl Pintor {
    /// `format`: o do alvo, um "Srgb" (a mistura é em espaço linear).
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let modulo = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("pelicano"), source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{}{}", pilha::SHADER, pilha::funda::SHADER).into()) });
        let camada_globais = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
        });
        let camada_textura = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let globais = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let grupo = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &camada_globais, entries: &[wgpu::BindGroupEntry { binding: 0, resource: globais.as_entire_binding() }] });
        let so_globais = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&camada_globais], push_constant_ranges: &[] });
        let com_textura = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&camada_globais, &camada_textura], push_constant_ranges: &[] });
        // Tudo sai pré-multiplicado, como o pixmap da CPU.
        let mistura = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
        let pipeline_em = |format: wgpu::TextureFormat, layout: &wgpu::PipelineLayout, vs: &str, fs: &str, vec4s: u32| {
            let attrs: Vec<wgpu::VertexAttribute> = (0..vec4s).map(|i| wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16 * i as u64, shader_location: i }).collect();
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(layout),
                vertex: wgpu::VertexState { module: &modulo, entry_point: vs, buffers: &[wgpu::VertexBufferLayout { array_stride: 16 * vec4s as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs }] },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState { module: &modulo, entry_point: fs, targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState { color: mistura, alpha: mistura }), write_mask: wgpu::ColorWrites::ALL })] }),
                multiview: None,
            })
        };
        let pipeline = |layout: &wgpu::PipelineLayout, vs: &str, fs: &str, vec4s: u32| pipeline_em(format, layout, vs, fs, vec4s);
        let quad = pipeline(&so_globais, "vs_quad", "fs_quad", (QUAD / 4) as u32);
        let glifo = pipeline(&com_textura, "vs_glifo", "fs_glifo", (GLIFO / 4) as u32);
        let traco = pipeline(&so_globais, "vs_traco", "fs_traco", (TRACO / 4) as u32);
        let pilha = pilha::pipeline(device, &modulo, &com_textura, format);
        let pilha_funda = pilha::funda::pipeline_funda(device, &modulo, &com_textura, format);
        let (linhas, camada_fundo) = pilha::funda::pipeline_linhas(device, &camada_globais, format);
        let borrar = pipeline_em(format.remove_srgb_suffix(), &com_textura, "vs_borrar", "fs_borrar", (BORRAR / 4) as u32);
        let amostrador = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let atlas = textura(device, ATLAS, ATLAS);
        let grupo_atlas = grupo_textura(device, &camada_textura, &atlas, &amostrador);
        Pintor { quad, glifo, traco, pilha, pilha_funda, linhas, camada_fundo, fundo: None, globais, grupo, camada_textura, amostrador, atlas, grupo_atlas, buffers: [None, None, None, None, None, None], texturas: HashMap::new(), quadro: 0, borrar, formato: format, buffers_vidro: [None, None], copias: None, pedacos: None }
    }

    /// Pinta a lista em `alvo` (limpo com o fundo) e envia. `alvo` tem o tamanho da lista; o vidro
    /// precisa que ele aceite `COPY_SRC` (sem isso o vidro sai só com a cor, sem desfoque).
    pub(crate) fn desenhar(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, alvo_tex: &wgpu::Texture, l: &mut Lista) {
        let alvo = &alvo_tex.create_view(&Default::default());
        self.quadro += 1;
        for g in l.atlas.envios.drain(..) {
            // A cobertura vira branco pré-multiplicado; o emoji, pré-multiplicado.
            let px = &l.atlas.bitmaps[g.idx as usize];
            let dados: Vec<u8> = if g.cor {
                px.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]].map(|c| ((c as u32 * p[3] as u32 + 127) / 255) as u8).into_iter().chain([p[3]])).collect()
            } else {
                px.iter().flat_map(|&a| [a; 4]).collect()
            };
            queue.write_texture(
                wgpu::ImageCopyTexture { texture: &self.atlas, mip_level: 0, origin: wgpu::Origin3d { x: g.x, y: g.y, z: 0 }, aspect: wgpu::TextureAspect::All },
                &dados,
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * g.w), rows_per_image: Some(g.h) },
                wgpu::Extent3d { width: g.w, height: g.h, depth_or_array_layers: 1 },
            );
        }
        let mut pedacos: Vec<(u64, Arc<Pixmap>, [u32; 4])> = vec![];
        for (id, pm) in l.usadas.drain(..) {
            let quadro = self.quadro;
            if !self.texturas.contains_key(&id) {
                let (w, h) = (pm.width(), pm.height());
                // Uma versão de outra: a textura da mais nova que ainda está aqui (e não é desenhada neste quadro)
                // passa a ser desta, e só o que mudou desde ela sobe.
                let versoes = l.versoes.iter().find(|v| v.0 == id).map(|v| &*v.1).unwrap_or(&[]);
                let achou = versoes.iter().position(|v| self.texturas.get(&v.0).is_some_and(|e| e.1 != quadro && e.4.width() == w && e.4.height() == h));
                let e = match achou {
                    Some(k) => {
                        let e = self.texturas.remove(&versoes[k].0).expect("achada acima");
                        for r in versoes[..=k].iter().flat_map(|v| v.1.iter()) {
                            let [x, y] = [r[0].min(w), r[1].min(h)];
                            let [rw, rh] = [r[2].min(w - x), r[3].min(h - y)];
                            if rw > 0 && rh > 0 {
                                pedacos.push((id, pm.clone(), [x, y, rw, rh]));
                            }
                        }
                        e
                    }
                    None => {
                        let t = textura(device, w, h);
                        subir(queue, &t, &pm);
                        (grupo_textura(device, &self.camada_textura, &t, &self.amostrador), quadro, pm.data().len(), Arc::downgrade(&pm), t)
                    }
                };
                self.texturas.insert(id, (e.0, quadro, e.2, Arc::downgrade(&pm), e.4));
            }
            if let Some(e) = self.texturas.get_mut(&id) {
                e.1 = quadro;
            }
        }
        // Imagem que sumiu da tela por um segundo ou foi liberada (o quadro de um jogo, novo a cada
        // vez) solta a textura. Passando de
        // TEXTURAS_MAX (uma imagem nova por quadro: zoom de PDF, resize), solta as usadas há mais tempo
        // até caber: soltar todas as que não são deste quadro fazia subir de novo, a cada quadro, as
        // que se revezam na tela (as poses de um bicho).
        let q = self.quadro;
        self.texturas.retain(|_, e| e.1 == q || (e.1 + 60 >= q && e.3.strong_count() > 0));
        let mut total: usize = self.texturas.values().map(|e| e.2).sum();
        if total > TEXTURAS_MAX {
            let mut velhas: Vec<(u64, u64, usize)> = self.texturas.iter().filter(|e| e.1 .1 < q).map(|(id, e)| (e.1, *id, e.2)).collect();
            velhas.sort_unstable();
            for (_, id, n) in velhas {
                if total <= TEXTURAS_MAX {
                    break;
                }
                self.texturas.remove(&id);
                total -= n;
            }
        }
        let (w, h) = l.tamanho;
        queue.write_buffer(&self.globais, 0, bytes(&[w as f32, h as f32, 0.0, 0.0]));
        for (i, v) in [&l.quads, &l.glifos, &l.tracos, &l.pilhas, &l.fundas, &l.linhas].into_iter().enumerate() {
            let b = bytes(v);
            if b.is_empty() {
                continue;
            }
            if self.buffers[i].as_ref().map_or(true, |x| x.size() < b.len() as u64) {
                let size = (b.len() as u64).next_power_of_two().max(4096);
                self.buffers[i] = Some(device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
            }
            queue.write_buffer(self.buffers[i].as_ref().expect("criado acima"), 0, b);
        }
        // Linear e pré-multiplicado: o alvo "Srgb" codifica na escrita.
        let [r, g, b, a] = rgba(l.fundo).map(|v| v as f64);
        let limpo = wgpu::Color { r: r * a, g: g * a, b: b * a, a };
        let vidro = !l.vidros.is_empty() && alvo_tex.usage().contains(wgpu::TextureUsages::COPY_SRC);
        if vidro {
            self.preparar_vidros(device, queue, l);
        }
        let mut enc = device.create_command_encoder(&Default::default());
        self.subir_pedacos(device, queue, &mut enc, &pedacos);
        let mut inicio = [0u32; 6];
        // As fundas seguidas vão numa passada só delas, com o buffer de profundidade (as outras passadas não o veem).
        if l.lotes.iter().any(|x| x.0 == Tipo::PilhaFunda) && self.fundo.as_ref().is_none_or(|f| f.2 != (w, h)) {
            let t = device.create_texture(&wgpu::TextureDescriptor { label: Some("fundo"), size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: pilha::funda::FORMATO_FUNDO, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[] });
            let vista = t.create_view(&Default::default());
            let grupo = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fundo"), layout: &self.camada_fundo, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&vista) }] });
            self.fundo = Some((vista, grupo, (w, h)));
        }
        let mut de = 0;
        loop {
            let funda = l.lotes.get(de).is_some_and(|x| x.0 == Tipo::PilhaFunda);
            let ate = l.lotes[de..].iter().position(|x| matches!(x.0, Tipo::Vidro | Tipo::LinhasFundas) || (x.0 == Tipo::PilhaFunda) != funda).map_or(l.lotes.len(), |p| de + p);
            let load = if de == 0 { wgpu::LoadOp::Clear(limpo) } else { wgpu::LoadOp::Load };
            let fundo = self.fundo.as_ref().filter(|_| funda).map(|f| wgpu::RenderPassDepthStencilAttachment { view: &f.0, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: alvo, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: fundo,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.grupo, &[]);
            let mut atual: Option<(Tipo, u64)> = None;
            for &(tipo, tex, n) in &l.lotes[de..ate] {
                let i = tipo as usize;
                if atual.map(|a| a.0) != Some(tipo) {
                    pass.set_pipeline(match tipo {
                        Tipo::Quad => &self.quad,
                        Tipo::Glifo => &self.glifo,
                        Tipo::Traco => &self.traco,
                        Tipo::Pilha => &self.pilha,
                        Tipo::PilhaFunda => &self.pilha_funda,
                        Tipo::Vidro | Tipo::LinhasFundas => unreachable!("o vidro e as linhas separam os passes"),
                    });
                    pass.set_vertex_buffer(0, self.buffers[i].as_ref().expect("tem instâncias").slice(..));
                }
                if matches!(tipo, Tipo::Glifo | Tipo::Pilha | Tipo::PilhaFunda) && atual != Some((tipo, tex)) {
                    let g = if tex == DO_ATLAS { &self.grupo_atlas } else { &self.texturas[&tex].0 };
                    pass.set_bind_group(1, g, &[]);
                }
                atual = Some((tipo, tex));
                let vertices = match tipo {
                    Tipo::Pilha => pilha::vertices(&l.pilhas[inicio[i] as usize * pilha::PILHA..(inicio[i] + n) as usize * pilha::PILHA]),
                    Tipo::PilhaFunda => pilha::funda::vertices_fundas(&l.fundas[inicio[i] as usize * pilha::funda::FUNDA..(inicio[i] + n) as usize * pilha::funda::FUNDA]),
                    _ => 4,
                };
                pass.draw(0..vertices, inicio[i]..inicio[i] + n);
                inicio[i] += n;
            }
            drop(pass);
            if ate == l.lotes.len() {
                break;
            }
            if l.lotes[ate].0 == Tipo::Vidro {
                if vidro {
                    self.desfocar(&mut enc, alvo, alvo_tex, l, l.lotes[ate].1 as usize);
                }
                de = ate + 1;
            } else if l.lotes[ate].0 == Tipo::LinhasFundas {
                // uma passada só para as linhas: lê a profundidade que a cena funda deixou
                let i = Tipo::LinhasFundas as usize;
                if let (Some(f), Some(b)) = (&self.fundo, &self.buffers[i]) {
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("linhas fundas"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: alvo, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.linhas);
                    pass.set_bind_group(0, &self.grupo, &[]);
                    pass.set_bind_group(1, &f.1, &[]);
                    pass.set_vertex_buffer(0, b.slice(..));
                    pass.draw(0..3, inicio[i]..inicio[i] + 1);
                }
                inicio[i] += 1;
                de = ate + 1;
            } else {
                de = ate;
            }
            if de == l.lotes.len() {
                break;
            }
        }
        queue.submit([enc.finish()]);
    }

    /// Os `pedacos` (o número da imagem, ela e o retângulo dela) todos num buffer só, linha a linha no alinhamento que a
    /// cópia pede, e de lá cada um para a sua textura.
    fn subir_pedacos(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, pedacos: &[(u64, Arc<Pixmap>, [u32; 4])]) {
        let linha = |w: u32| (4 * w).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) as u64;
        let total: u64 = pedacos.iter().map(|p| linha(p.2[2]) * p.2[3] as u64).sum();
        let Some(tamanho) = std::num::NonZeroU64::new(total) else { return };
        if self.pedacos.as_ref().is_none_or(|b| b.size() < total) {
            let size = total.next_power_of_two().max(1 << 20);
            self.pedacos = Some(device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        }
        let buf = self.pedacos.as_ref().expect("criado acima");
        {
            let Some(mut destino) = queue.write_buffer_with(buf, 0, tamanho) else { return };
            let mut de = 0usize;
            for (_, pm, [x, y, w, h]) in pedacos {
                let (pw, passo, n) = (pm.width() as usize, linha(*w) as usize, 4 * *w as usize);
                for k in 0..*h as usize {
                    let i = ((*y as usize + k) * pw + *x as usize) * 4;
                    destino[de + k * passo..][..n].copy_from_slice(&pm.data()[i..i + n]);
                }
                de += passo * *h as usize;
            }
        }
        let mut de = 0u64;
        for (id, _, [x, y, w, h]) in pedacos {
            let offset = de;
            de += linha(*w) * *h as u64;
            let Some(t) = self.texturas.get(id).map(|e| &e.4) else { continue };
            enc.copy_buffer_to_texture(
                wgpu::ImageCopyBuffer { buffer: buf, layout: wgpu::ImageDataLayout { offset, bytes_per_row: Some(linha(*w) as u32), rows_per_image: Some(*h) } },
                wgpu::ImageCopyTexture { texture: t, mip_level: 0, origin: wgpu::Origin3d { x: *x, y: *y, z: 0 }, aspect: wgpu::TextureAspect::All },
                wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
            );
        }
    }

    /// As texturas do tamanho do maior vidro e as instâncias de todos os passes do quadro.
    fn preparar_vidros(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, l: &Lista) {
        let degrau = |v: u32| v.div_ceil(256) * 256;
        let cheia = l.vidros.iter().fold((1, 1), |m, v| (m.0.max(v.area[2]), m.1.max(v.area[3])));
        let reduzida = l.vidros.iter().fold((1, 1), |m, v| {
            let r = v.reduzida();
            (m.0.max(r.0), m.1.max(r.1))
        });
        let cabe = |c: (u32, u32), q: (u32, u32)| c.0 >= q.0 && c.1 >= q.1;
        if !self.copias.as_ref().is_some_and(|c| cabe(c.1, cheia) && cabe(c.2, reduzida)) {
            let (cw, ch) = (degrau(cheia.0), degrau(cheia.1));
            let (rw, rh) = (degrau(reduzida.0), degrau(reduzida.1));
            let nova = |w: u32, h: u32| {
                let t = device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    // Sem o "Srgb": o desfoque soma os valores como estão, em sRGB, como a CPU faz.
                    format: self.formato.remove_srgb_suffix(),
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let g = grupo_textura(device, &self.camada_textura, &t, &self.amostrador);
                (t, g)
            };
            self.copias = Some(([nova(cw, ch), nova(rw, rh), nova(rw, rh)], (cw, ch), (rw, rh)));
        }
        let (rw, rh) = self.copias.as_ref().expect("criadas acima").2;
        let mut passes = Vec::with_capacity(l.vidros.len() * 7 * BORRAR);
        let mut voltas = Vec::with_capacity(l.vidros.len() * GLIFO);
        for v in &l.vidros {
            let (d, k) = v.reducao();
            let (sw, sh) = v.reduzida();
            passes.extend_from_slice(&[0.0, k as f32, d as f32, 0.0, v.area[2] as f32, v.area[3] as f32, 0.0, 0.0]);
            for _ in 0..3 {
                passes.extend_from_slice(&[1.0, k as f32, d as f32, 0.0, sw as f32, sh as f32, 0.0, 0.0]);
                passes.extend_from_slice(&[2.0, k as f32, d as f32, 0.0, sw as f32, sh as f32, 0.0, 0.0]);
            }
            let mut g = [0.0; GLIFO];
            g[..4].copy_from_slice(&v.forma);
            g[4..8].copy_from_slice(&[v.area[0] as f32, v.area[1] as f32, sw as f32, sh as f32]);
            g[8..12].copy_from_slice(&[1.0, 1.0, 1.0, v.alfa]);
            g[12..16].copy_from_slice(&v.raios);
            g[16..20].copy_from_slice(&[3.0, d as f32, 1.0 / rw as f32, 1.0 / rh as f32]);
            g[20..].copy_from_slice(&v.rec);
            voltas.extend_from_slice(&g);
        }
        for (i, dados) in [passes, voltas].iter().enumerate() {
            let b = bytes(dados);
            if self.buffers_vidro[i].as_ref().map_or(true, |x| x.size() < b.len() as u64) {
                let size = (b.len() as u64).next_power_of_two().max(1024);
                self.buffers_vidro[i] = Some(device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
            }
            queue.write_buffer(self.buffers_vidro[i].as_ref().expect("criado acima"), 0, b);
        }
    }

    /// O vidro `n`: copia a área, reduz, três caixas em cada direção e devolve com os cantos.
    fn desfocar(&self, enc: &mut wgpu::CommandEncoder, alvo: &wgpu::TextureView, alvo_tex: &wgpu::Texture, l: &Lista, n: usize) {
        let v = &l.vidros[n];
        let Some(([a, b, c], _, _)) = &self.copias else { return };
        let [x, y, w, h] = v.area;
        enc.copy_texture_to_texture(
            wgpu::ImageCopyTexture { texture: alvo_tex, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            wgpu::ImageCopyTexture { texture: &a.0, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let (sw, sh) = v.reduzida();
        let passes = self.buffers_vidro[0].as_ref().expect("preparado");
        // Cópia -> b (reduz); depois b -> c (horizontal) e c -> b (vertical), três vezes.
        let mut de_para = vec![(&a.1, &b.0)];
        for _ in 0..3 {
            de_para.push((&b.1, &c.0));
            de_para.push((&c.1, &b.0));
        }
        for (i, (de, para)) in de_para.into_iter().enumerate() {
            let vista = para.create_view(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &vista, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.borrar);
            pass.set_bind_group(0, &self.grupo, &[]);
            pass.set_bind_group(1, de, &[]);
            pass.set_viewport(0.0, 0.0, sw as f32, sh as f32, 0.0, 1.0);
            pass.set_vertex_buffer(0, passes.slice(..));
            let k = (n * 7 + i) as u32;
            pass.draw(0..4, k..k + 1);
        }
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: alvo, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.glifo);
        pass.set_bind_group(0, &self.grupo, &[]);
        pass.set_bind_group(1, &b.1, &[]);
        pass.set_vertex_buffer(0, self.buffers_vidro[1].as_ref().expect("preparado").slice(..));
        pass.draw(0..4, n as u32..n as u32 + 1);
    }
}

/// Os limites que o aparelho precisa dar: os do WebGL2, com as texturas do tamanho que ele aguenta
/// e as variáveis entre os estágios que o quad usa (13 vec4). `None` se não tem.
pub(crate) fn limites(a: &wgpu::Adapter) -> Option<wgpu::Limits> {
    let tem = a.limits().max_inter_stage_shader_components;
    (tem >= 4 * (QUAD as u32 / 4 + 1)).then(|| wgpu::Limits { max_inter_stage_shader_components: tem, ..wgpu::Limits::downlevel_webgl2_defaults().using_resolution(a.limits()) })
}

fn textura(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// A imagem `pm` inteira para a textura `t`, do mesmo tamanho.
fn subir(queue: &wgpu::Queue, t: &wgpu::Texture, pm: &Pixmap) {
    queue.write_texture(
        wgpu::ImageCopyTexture { texture: t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        pm.data(),
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(4 * pm.width()), rows_per_image: Some(pm.height()) },
        wgpu::Extent3d { width: pm.width(), height: pm.height(), depth_or_array_layers: 1 },
    );
}

fn grupo_textura(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, t: &wgpu::Texture, a: &wgpu::Sampler) -> wgpu::BindGroup {
    let view = t.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(a) }],
    })
}

fn bytes(v: &[f32]) -> &[u8] {
    // SAFETY: f32 não tem enchimento nem valor inválido como bytes, e u8 alinha em 1.
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
}

/// Desenho fora da tela, para comparar com a CPU (`tests/gpu.rs`). `None` sem adaptador gráfico.
#[doc(hidden)]
pub struct GpuOffscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pintor: Pintor,
}

impl GpuOffscreen {
    pub fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let adapter = futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() }))?;
        let (device, queue) = futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: limites(&adapter)? }, None)).ok()?;
        let pintor = Pintor::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb);
        Some(GpuOffscreen { device, queue, pintor })
    }

    /// O quadro de `desenhar` pela GPU, em RGBA pré-multiplicado como `Renderer::pixmap`.
    /// `Err` com o motivo se o quadro cairia na CPU.
    pub fn capture(&mut self, r: &mut Renderer, size: crate::Size, scale: f32, fundo: Color, desenhar: impl FnOnce(&mut Renderer)) -> Result<Vec<u8>, &'static str> {
        r.falta_gpu = None;
        r.gpu.max_textura = self.device.limits().max_texture_dimension_2d;
        r.begin_gpu(size, scale, fundo);
        desenhar(r);
        r.end();
        if let Some(m) = r.falta_gpu {
            return Err(m);
        }
        let (w, h) = r.gpu.tamanho;
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.pintor.desenhar(&self.device, &self.queue, &t, &mut r.gpu);
        let linha = (4 * w).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (linha * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::ImageCopyTexture { texture: &t, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::ImageCopyBuffer { buffer: &buf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(linha), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let dados = buf.slice(..).get_mapped_range();
        Ok(dados.chunks(linha as usize).flat_map(|l| &l[..4 * w as usize]).copied().collect())
    }
}

const SHADER: &str = r#"
struct Globais { tela: vec4<f32> }
@group(0) @binding(0) var<uniform> g: Globais;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var amostra: sampler;

fn ndc(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / g.tela.x * 2.0 - 1.0, 1.0 - p.y / g.tela.y * 2.0, 0.0, 1.0);
}

// Os 4 cantos da faixa de triângulos.
fn canto(i: u32) -> vec2<f32> {
    return vec2<f32>(f32(i & 1u), f32(i >> 1u));
}

// Quanto do pixel de centro `c` cai no retângulo r (x, y, w, h) com os cantos k: a `cobertura` da CPU.
fn cobertura(r: vec4<f32>, k0: vec4<f32>, c: vec2<f32>) -> f32 {
    let k = clamp(k0, vec4<f32>(0.0), vec4<f32>(min(r.z, r.w) / 2.0));
    let l = r.x;
    let t = r.y;
    let d = r.x + r.z;
    let b = r.y + r.w;
    let fx = clamp(min(c.x + 0.5 - l, d - c.x + 0.5), 0.0, 1.0);
    let fy = clamp(min(c.y + 0.5 - t, b - c.y + 0.5), 0.0, 1.0);
    var cob = fx * fy;
    var raio = 0.0;
    var o = vec2<f32>(0.0);
    if c.x < l + k.x && c.y < t + k.x {
        raio = k.x; o = vec2<f32>(l + k.x, t + k.x);
    } else if c.x > d - k.y && c.y < t + k.y {
        raio = k.y; o = vec2<f32>(d - k.y, t + k.y);
    } else if c.x > d - k.z && c.y > b - k.z {
        raio = k.z; o = vec2<f32>(d - k.z, b - k.z);
    } else if c.x < l + k.w && c.y > b - k.w {
        raio = k.w; o = vec2<f32>(l + k.w, b - k.w);
    }
    if raio > 0.0 {
        cob *= clamp(raio - distance(c, o) + 0.5, 0.0, 1.0);
    }
    return cob;
}

// O recorte: o retângulo em pixels inteiros e, se houver, o arredondado.
fn recorte(c: vec2<f32>, ret: vec4<f32>, rr: vec4<f32>, rk: vec4<f32>) -> f32 {
    if c.x < ret.x || c.y < ret.y || c.x >= ret.z || c.y >= ret.w {
        return 0.0;
    }
    if all(rk == vec4<f32>(0.0)) {
        return 1.0;
    }
    return cobertura(rr, rk, c);
}

fn pre(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

// Um texel pré-multiplicado em sRGB (imagens, emoji) para pré-multiplicado linear.
fn linear_pre(t: vec4<f32>) -> vec4<f32> {
    if t.a <= 0.0 { return vec4<f32>(0.0); }
    let c = clamp(t.rgb / t.a, vec3<f32>(0.0), vec3<f32>(1.0));
    let l = select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
    return vec4<f32>(l * t.a, t.a);
}

// A caixa (x, y, w, h) crescida m e cortada pelo recorte: só ali o fragmento roda.
fn vertice(i: u32, r: vec4<f32>, m: f32, ret: vec4<f32>) -> vec2<f32> {
    let lo = max(r.xy - m, ret.xy);
    let hi = max(min(r.xy + r.zw + m, ret.zw), lo);
    return mix(lo, hi, canto(i));
}

// A sombra do Iced (quad/solid.wgsl): some do meio (0,5 na borda) até `b` além dela.
fn sombra(r: vec4<f32>, k: f32, c: vec2<f32>, b: f32) -> f32 {
    let meia = r.zw * 0.5;
    let p = abs(c - (r.xy + meia)) - meia + vec2<f32>(k);
    let d = max(length(max(p, vec2<f32>(0.0))) - k, 0.0);
    return 1.0 - smoothstep(-b, b, d);
}

struct QuadIn {
    @location(0) rect: vec4<f32>,
    @location(1) raios: vec4<f32>,
    @location(2) c0: vec4<f32>,
    @location(3) c1: vec4<f32>,
    @location(4) c2: vec4<f32>,
    @location(5) c3: vec4<f32>,
    @location(6) paradas: vec4<f32>,
    @location(7) borda: vec4<f32>,
    // largura da borda, tipo (0 cor, 1 degradê, 2 sombra), paradas, desfoque da sombra
    @location(8) params: vec4<f32>,
    // degradê: início e direção/comprimento; sombra: a caixa que ela não cobre
    @location(9) extra: vec4<f32>,
    @location(10) ret: vec4<f32>,
    @location(11) rr: vec4<f32>,
    @location(12) rk: vec4<f32>,
}
struct QuadOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) rect: vec4<f32>,
    @location(1) @interpolate(flat) raios: vec4<f32>,
    @location(2) @interpolate(flat) c0: vec4<f32>,
    @location(3) @interpolate(flat) c1: vec4<f32>,
    @location(4) @interpolate(flat) c2: vec4<f32>,
    @location(5) @interpolate(flat) c3: vec4<f32>,
    @location(6) @interpolate(flat) paradas: vec4<f32>,
    @location(7) @interpolate(flat) borda: vec4<f32>,
    @location(8) @interpolate(flat) params: vec4<f32>,
    @location(9) @interpolate(flat) extra: vec4<f32>,
    @location(10) @interpolate(flat) ret: vec4<f32>,
    @location(11) @interpolate(flat) rr: vec4<f32>,
    @location(12) @interpolate(flat) rk: vec4<f32>,
}

@vertex fn vs_quad(@builtin(vertex_index) i: u32, q: QuadIn) -> QuadOut {
    var o: QuadOut;
    let m = select(1.0, q.params.w + 1.0, q.params.y == 2.0);
    o.pos = ndc(vertice(i, q.rect, m, q.ret));
    o.rect = q.rect; o.raios = q.raios; o.c0 = q.c0; o.c1 = q.c1; o.c2 = q.c2; o.c3 = q.c3;
    o.paradas = q.paradas; o.borda = q.borda; o.params = q.params; o.extra = q.extra;
    o.ret = q.ret; o.rr = q.rr; o.rk = q.rk;
    return o;
}

fn degrade(c0: vec4<f32>, c1: vec4<f32>, c2: vec4<f32>, c3: vec4<f32>, paradas: vec4<f32>, n: i32, t: f32) -> vec4<f32> {
    var cs = array<vec4<f32>, 4>(c0, c1, c2, c3);
    var i = 0;
    while i < n && paradas[i] < t {
        i++;
    }
    if i == 0 { return cs[0]; }
    if i == n { return cs[n - 1]; }
    let p0 = paradas[i - 1];
    let p1 = paradas[i];
    let k = select(1.0, (t - p0) / (p1 - p0), p1 > p0);
    return mix(cs[i - 1], cs[i], k);
}

@fragment fn fs_quad(q: QuadOut) -> @location(0) vec4<f32> {
    let c = q.pos.xy;
    let corte = recorte(c, q.ret, q.rr, q.rk);
    if corte == 0.0 { discard; }
    var saida: vec4<f32>;
    if q.params.y == 2.0 {
        let blur = q.params.w;
        var a: f32;
        if blur < 0.5 {
            a = cobertura(q.rect, q.raios, c);
        } else {
            let meio = q.rect.xy + q.rect.zw * 0.5;
            var k = select(q.raios.x, q.raios.y, c.x > meio.x);
            if c.y > meio.y { k = select(q.raios.w, q.raios.z, c.x > meio.x); }
            a = sombra(q.rect, min(k, min(q.rect.z, q.rect.w) / 2.0), c, blur);
        }
        saida = pre(q.c0) * a * (1.0 - cobertura(q.extra, q.raios, c));
    } else {
        var cor = q.c0;
        if q.params.y == 1.0 {
            cor = degrade(q.c0, q.c1, q.c2, q.c3, q.paradas, i32(q.params.z), clamp(dot(c - q.extra.xy, q.extra.zw), 0.0, 1.0));
        }
        let fora = cobertura(q.rect, q.raios, c);
        saida = pre(cor) * fora;
        let w = q.params.x;
        if w > 0.0 {
            let dentro = vec4<f32>(q.rect.xy + w, q.rect.zw - 2.0 * w);
            var cd = 0.0;
            if dentro.z > 0.0 && dentro.w > 0.0 {
                cd = cobertura(dentro, max(q.raios - w, vec4<f32>(0.0)), c);
            }
            let b = pre(q.borda) * clamp(fora - cd, 0.0, 1.0);
            saida = b + saida * (1.0 - b.a);
        }
    }
    return saida * corte;
}

struct GlifoIn {
    @location(0) dst: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) cor: vec4<f32>,
    @location(3) raios: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) ret: vec4<f32>,
    @location(6) rr: vec4<f32>,
    @location(7) rk: vec4<f32>,
}
struct GlifoOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) dst: vec4<f32>,
    @location(1) @interpolate(flat) uv: vec4<f32>,
    @location(2) @interpolate(flat) cor: vec4<f32>,
    @location(3) @interpolate(flat) raios: vec4<f32>,
    @location(4) @interpolate(flat) params: vec4<f32>,
    @location(5) @interpolate(flat) ret: vec4<f32>,
    @location(6) @interpolate(flat) rr: vec4<f32>,
    @location(7) @interpolate(flat) rk: vec4<f32>,
}

@vertex fn vs_glifo(@builtin(vertex_index) i: u32, q: GlifoIn) -> GlifoOut {
    var o: GlifoOut;
    o.pos = ndc(vertice(i, q.dst, 0.0, q.ret));
    o.dst = q.dst; o.uv = q.uv; o.cor = q.cor; o.raios = q.raios; o.params = q.params;
    o.ret = q.ret; o.rr = q.rr; o.rk = q.rk;
    return o;
}

@fragment fn fs_glifo(q: GlifoOut) -> @location(0) vec4<f32> {
    let c = q.pos.xy;
    let corte = recorte(c, q.ret, q.rr, q.rk);
    if corte == 0.0 { discard; }
    var saida: vec4<f32>;
    if q.params.x == 3.0 {
        // Vidro: o desfoque reduzido, esticado de volta (bilinear) e com os cantos. Ele foi feito em
        // sRGB (como a CPU): volta ao linear como uma imagem.
        // uv = (origem da área, tamanho reduzido); params = (3, redução, 1 / tamanho da textura).
        // A amostra fica dentro do reduzido: a borda se repete, como no `escalar` da CPU.
        let tc = clamp((c - q.uv.xy) / q.params.y, vec2<f32>(0.5), q.uv.zw - vec2<f32>(0.5));
        let t = textureSampleLevel(tex, amostra, tc * q.params.zw, 0.0);
        saida = linear_pre(t) * q.cor.a * cobertura(q.dst, q.raios, c);
    } else if q.params.x == 2.0 {
        // Imagem: suavizada (bilinear, como o `escalar` da CPU) e com os cantos.
        let t = textureSampleLevel(tex, amostra, (c - q.dst.xy) / q.dst.zw, 0.0);
        saida = linear_pre(t) * q.cor.a * cobertura(q.dst, q.raios, c);
    } else if q.params.x == 4.0 {
        // Imagem nítida: o texel em que o meio do pixel cai, inteiro (pixel art esticada em blocos).
        let tam = vec2<f32>(textureDimensions(tex));
        let p = clamp(floor((c - q.dst.xy) / q.dst.zw * tam), vec2<f32>(0.0), tam - vec2<f32>(1.0));
        let t = textureLoad(tex, vec2<i32>(p), 0);
        saida = linear_pre(t) * q.cor.a * cobertura(q.dst, q.raios, c);
    } else if q.params.x >= 5.0 {
        // Imagem transformada: o meio do pixel volta para a imagem pela inversa; a borda suavizada pela
        // distância (em pixels) até cada lado, como a cobertura da CPU. Um pedaço (`draw_image_pieces_nearest`)
        // traz o retângulo dele, de `uv.zw` a `params.yz`; sem pedaço `params.y` é zero e vale a imagem toda.
        let tam = vec2<f32>(textureDimensions(tex));
        let lo = q.uv.zw;
        let hi = select(tam, q.params.yz, q.params.y > 0.0);
        let p = vec2<f32>(q.raios.x * c.x + q.raios.z * c.y + q.uv.x, q.raios.y * c.x + q.raios.w * c.y + q.uv.y);
        let k = vec2<f32>(length(q.raios.xz), length(q.raios.yw));
        let d = min(p - lo, hi - p) / max(k, vec2<f32>(1e-6));
        let cob = clamp(d.x + 0.5, 0.0, 1.0) * clamp(d.y + 0.5, 0.0, 1.0);
        if cob <= 0.0 { discard; }
        var t: vec4<f32>;
        if q.params.x == 5.0 {
            t = textureLoad(tex, vec2<i32>(clamp(floor(p), lo, hi - vec2<f32>(1.0))), 0);
        } else {
            t = textureSampleLevel(tex, amostra, p / tam, 0.0);
        }
        saida = linear_pre(t) * q.cor.a * cob;
    } else {
        // Glifo: pixel a pixel do atlas.
        let t = textureLoad(tex, vec2<i32>(q.uv.xy + floor(c - q.dst.xy)), 0);
        if q.params.x == 1.0 {
            saida = linear_pre(t) * q.cor.a;
        } else {
            saida = vec4<f32>(q.cor.rgb, 1.0) * t.a * q.cor.a;
        }
    }
    return saida * corte;
}

struct BorrarIn {
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
}
struct BorrarOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) a: vec4<f32>,
    @location(1) @interpolate(flat) b: vec4<f32>,
}

// O vidro, passe a passo: a cobre o alvo inteiro (o viewport limita à área reduzida).
@vertex fn vs_borrar(@builtin(vertex_index) i: u32, q: BorrarIn) -> BorrarOut {
    var o: BorrarOut;
    let p = canto(i);
    o.pos = vec4<f32>(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0, 0.0, 1.0);
    o.a = q.a; o.b = q.b;
    return o;
}

// a = (passe, raio da caixa, redução, _); b = (largura, altura da origem, _, _).
// Passe 0 reduz (média de d × d); 1 e 2 são a caixa horizontal e a vertical (a `box_blur` da CPU,
// com a borda repetida).
@fragment fn fs_borrar(q: BorrarOut) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(q.pos.xy));
    let k = i32(q.a.y);
    let d = i32(q.a.z);
    let lim = vec2<i32>(q.b.xy) - vec2<i32>(1);
    var soma = vec4<f32>(0.0);
    if q.a.x == 0.0 {
        var n = 0.0;
        for (var y = 0; y < d; y++) {
            for (var x = 0; x < d; x++) {
                let e = p * d + vec2<i32>(x, y);
                if e.x <= lim.x && e.y <= lim.y {
                    soma += textureLoad(tex, e, 0);
                    n += 1.0;
                }
            }
        }
        return soma / max(n, 1.0);
    }
    let dir = select(vec2<i32>(0, 1), vec2<i32>(1, 0), q.a.x == 1.0);
    for (var j = -k; j <= k; j++) {
        soma += textureLoad(tex, clamp(p + dir * j, vec2<i32>(0), lim), 0);
    }
    return soma / f32(2 * k + 1);
}

struct TracoIn {
    @location(0) ab: vec4<f32>,
    @location(1) params: vec4<f32>,
    @location(2) cor: vec4<f32>,
    @location(3) ret: vec4<f32>,
    @location(4) rr: vec4<f32>,
    @location(5) rk: vec4<f32>,
}
struct TracoOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) ab: vec4<f32>,
    @location(1) @interpolate(flat) params: vec4<f32>,
    @location(2) @interpolate(flat) cor: vec4<f32>,
    @location(3) @interpolate(flat) ret: vec4<f32>,
    @location(4) @interpolate(flat) rr: vec4<f32>,
    @location(5) @interpolate(flat) rk: vec4<f32>,
}

@vertex fn vs_traco(@builtin(vertex_index) i: u32, q: TracoIn) -> TracoOut {
    var o: TracoOut;
    let a = q.ab.xy;
    let v = q.ab.zw - a;
    let n = length(v);
    let d = select(vec2<f32>(1.0, 0.0), v / n, n > 1e-6);
    let e = q.params.x + 1.0;
    let k = canto(i);
    let p = a + d * mix(-e, n + e, k.x) + vec2<f32>(-d.y, d.x) * mix(-e, e, k.y);
    o.pos = ndc(p);
    o.ab = q.ab; o.params = q.params; o.cor = q.cor; o.ret = q.ret; o.rr = q.rr; o.rk = q.rk;
    return o;
}

@fragment fn fs_traco(q: TracoOut) -> @location(0) vec4<f32> {
    let c = q.pos.xy;
    let corte = recorte(c, q.ret, q.rr, q.rk);
    if corte == 0.0 { discard; }
    let pa = c - q.ab.xy;
    let ba = q.ab.zw - q.ab.xy;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    let cob = clamp(q.params.x - length(pa - ba * h) + 0.5, 0.0, 1.0);
    return pre(q.cor) * cob * corte;
}
"#;
