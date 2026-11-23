//! O [`Volume`]: material e normal de cada voxel, fatia por fatia, a partir de uma função de forma.
//! A normal que a forma não dá sai da ocupação em volta. Não sabe de cor nem de desenho.
use std::thread::JoinHandle;

/// Material (0 = vazio) e normal de cada voxel; índice `(z * lado + y) * lado + x`, z = 0 embaixo.
pub struct Volume {
    pub lado: u32,
    pub fatias: u32,
    /// Quanto cada fatia sobe, em texels (2 fatias por texel = 0,5).
    pub espessura: f32,
    pub material: Vec<u8>,
    pub normal: Vec<[f32; 3]>,
}

impl Volume {
    /// `forma(x, y, z)` com x e y em texels a partir do meio da base (+y é a frente) e z em texels de
    /// altura: o material ali e a normal (no espaço do objeto), ou `None`. Normal zero = tirar da
    /// ocupação (raio de 2 texels).
    pub fn de_forma(lado: u32, fatias: u32, espessura: f32, forma: impl Fn(f32, f32, f32) -> Option<(u8, [f32; 3])>) -> Volume {
        let (l, n) = (lado as usize, fatias as usize);
        let meio = lado as f32 / 2.0;
        let cheio: Vec<Option<(u8, [f32; 3])>> = (0..n * l * l)
            .map(|i| {
                let (z, y, x) = (i / (l * l), i / l % l, i % l);
                forma(x as f32 + 0.5 - meio, y as f32 + 0.5 - meio, (z as f32 + 0.5) * espessura).filter(|v| v.0 != 0)
            })
            .collect();
        let ocupado = |x: i32, y: i32, z: i32| -> f32 {
            let dentro = x >= 0 && y >= 0 && z >= 0 && (x as usize) < l && (y as usize) < l && (z as usize) < n;
            if dentro && cheio[(z as usize * l + y as usize) * l + x as usize].is_some() { 1.0 } else { 0.0 }
        };
        // em z o raio é em fatias, por isso divide pela espessura
        let rz = (2.0 / espessura).round().max(1.0) as i32;
        let mut material = vec![0; n * l * l];
        let mut normal = vec![[0.0; 3]; n * l * l];
        for (i, v) in cheio.iter().enumerate() {
            let Some((m, nrm)) = *v else { continue };
            material[i] = m;
            normal[i] = if nrm != [0.0; 3] {
                nrm
            } else {
                let (z, y, x) = ((i / (l * l)) as i32, (i / l % l) as i32, (i % l) as i32);
                let g = [ocupado(x - 2, y, z) - ocupado(x + 2, y, z), ocupado(x, y - 2, z) - ocupado(x, y + 2, z), ocupado(x, y, z - rz) - ocupado(x, y, z + rz)];
                if g == [0.0; 3] { [0.0, 0.0, 1.0] } else { g }
            };
        }
        Volume { lado, fatias, espessura, material, normal }
    }

    /// O mesmo que [`de_forma`](Self::de_forma), numa thread à parte.
    pub fn em_fundo(lado: u32, fatias: u32, espessura: f32, forma: impl Fn(f32, f32, f32) -> Option<(u8, [f32; 3])> + Send + 'static) -> JoinHandle<Volume> {
        std::thread::spawn(move || Volume::de_forma(lado, fatias, espessura, forma))
    }

    /// O voxel `(x, y, z)`: material e normal.
    pub fn voxel(&self, x: u32, y: u32, z: u32) -> (u8, [f32; 3]) {
        let i = ((z * self.lado + y) * self.lado + x) as usize;
        (self.material[i], self.normal[i])
    }
}

#[test]
fn normal_da_ocupacao_aponta_para_fora() {
    let v = Volume::de_forma(16, 16, 1.0, |x, y, z| (x.abs() < 5.0 && y.abs() < 5.0 && (2.0..12.0).contains(&z)).then_some((1, [0.0; 3])));
    // o voxel no meio da face direita da caixa: normal para +x
    let (m, n) = v.voxel(12, 8, 8);
    assert_eq!(m, 1);
    assert!(n[0] > 0.0 && n[1].abs() < 1e-6 && n[2].abs() < 1e-6, "{n:?}");
    assert_eq!(v.voxel(0, 0, 0).0, 0);
}
