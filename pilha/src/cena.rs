//! A cena: o [`Modelo`] (o volume subido como atlas, com paleta e contorno), as [`Instancia`]s no
//! mapa, a [`Camera`] e o [`desenhar`] (descarta, ordena e manda as pilhas ao pelicano). Não monta
//! volume nem sabe de chão ou interface.
use crate::Volume;
use pelicano::{Point, Rect, Renderer, Stack, StackAtlas};

/// De onde vem a luz na tela (x para a direita, y para quem olha, z para cima): a do gato 2.5D do
/// jogo, de cima à esquerda, com pelo menos metade dela vindo da frente.
pub const LUZ: [f32; 3] = [-0.512, 0.5, 0.698];

/// O volume pronto para desenhar. O pelicano sobe o atlas para a GPU no primeiro uso.
#[derive(Clone)]
pub struct Modelo {
    pub atlas: StackAtlas,
    /// De onde vem a luz na tela e os limiares dos 3 tons (`Stack::tones`): [`LUZ`] e
    /// `Stack::TONES` se não mudar.
    pub luz: [f32; 3],
    pub tons: [f32; 2],
}

impl Modelo {
    /// `paleta[m]` = luz, base e sombra do material `m`; `paleta[0][0]` é a cor do contorno.
    /// `borda`: texels de contorno de cada lado (2 pontos na tela ÷ pontos por texel).
    pub fn novo(v: &Volume, borda: u32, paleta: &[[[u8; 3]; 3]]) -> Modelo {
        Modelo { atlas: StackAtlas::new(v.lado, v.fatias, v.espessura, borda, paleta, |x, y, z| v.voxel(x, y, z)), luz: LUZ, tons: Stack::TONES }
    }
}

/// Um objeto no mapa.
#[derive(Clone, Copy, Debug)]
pub struct Instancia {
    pub modelo: usize,
    /// No mapa (unidades do mapa).
    pub x: f32,
    pub y: f32,
    /// Giro próprio, em radianos (somado ao da câmera).
    pub giro: f32,
    /// Unidades do mapa por texel.
    pub escala: f32,
    /// Pose: deslocamento no topo (texels, no espaço do objeto), crescendo linear da base até lá: a
    /// árvore balança, o gato se inclina. E quanto ele sobe inteiro (pulo), em texels.
    pub balanco: (f32, f32),
    pub pulo: f32,
    /// A tinta da cor (`Stack::tint`): `Stack::NO_TINT` se não mudar.
    pub tinta: [[f32; 3]; 2],
    /// Onde começa a paleta dela no atlas (`Stack::palette`): 0 se só tem uma.
    pub paleta: u32,
}

/// A câmera: `centro` do mapa no meio da tela, `giro` em volta dele, `inclinacao` (0 = olhando reto
/// de cima) e `zoom` (pontos da tela por unidade do mapa).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub centro: (f32, f32),
    pub giro: f32,
    pub inclinacao: f32,
    pub zoom: f32,
    /// A área da tela (pontos lógicos) onde o mapa aparece.
    pub tela: Rect,
}

impl Camera {
    /// O ponto do chão do mapa na tela.
    pub fn projetar(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, c) = self.giro.sin_cos();
        let (dx, dy) = (x - self.centro.0, y - self.centro.1);
        let (rx, ry) = (c * dx - s * dy, s * dx + c * dy);
        (self.tela.x + self.tela.width / 2.0 + rx * self.zoom, self.tela.y + self.tela.height / 2.0 + ry * self.inclinacao.cos() * self.zoom)
    }
}

/// Desenha as instâncias vistas pela câmera: descarta as que caem fora da tela, ordena de trás para
/// a frente (pela base na tela) e manda as pilhas, as seguidas do mesmo modelo juntas. `contorno`:
/// a silhueta de uma fatia a cada tantas vai por baixo (0 = sem). Devolve quantas foram desenhadas.
pub fn desenhar(r: &mut Renderer, cam: &Camera, modelos: &[Modelo], inst: &[Instancia], contorno: u32) -> usize {
    let sobe = cam.inclinacao.sin();
    let t = cam.tela;
    let mut vistas: Vec<(usize, Stack)> = Vec::with_capacity(inst.len());
    for i in inst {
        let m = &modelos[i.modelo];
        let a = &m.atlas;
        let (px, py) = cam.projetar(i.x, i.y);
        let u = i.escala * cam.zoom;
        let raio = a.cell() as f32 * 0.75 * u;
        let alto = (a.slices() as f32 * a.thickness() + i.pulo) * u * sobe + raio;
        if px + raio < t.x || px - raio > t.x + t.width || py + raio < t.y || py - alto > t.y + t.height {
            continue;
        }
        let st = Stack { at: Point::new(px, py), yaw: cam.giro + i.giro, pitch: cam.inclinacao, scale: u, sway: i.balanco, lift: i.pulo, outline_step: contorno, light: m.luz, tones: m.tons, tint: i.tinta, palette: i.paleta };
        vistas.push((i.modelo, st));
    }
    vistas.sort_by(|a, b| a.1.at.y.total_cmp(&b.1.at.y));
    let mut lote: Vec<Stack> = Vec::new();
    for (k, &(m, st)) in vistas.iter().enumerate() {
        lote.push(st);
        if vistas.get(k + 1).is_none_or(|v| v.0 != m) {
            r.draw_stacks(&modelos[m].atlas, &lote);
            lote.clear();
        }
    }
    vistas.len()
}
