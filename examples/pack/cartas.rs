//! O baralho pixelado do pack (as SVGs de `assets/baralho`), embutido no executável.
#![allow(dead_code)] // cada jogo usa uma parte
use pelicano::Svg;

macro_rules! naipe {
    ($n:literal) => {
        [
            include_str!(concat!("../assets/baralho/", $n, "/01_A_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/02_2_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/03_3_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/04_4_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/05_5_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/06_6_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/07_7_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/08_8_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/09_9_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/10_10_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/11_J_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/12_Q_", $n, ".svg")),
            include_str!(concat!("../assets/baralho/", $n, "/13_K_", $n, ".svg")),
        ]
    };
}

/// Na ordem dos naipes: copas e ouros (vermelhos) alternam com espadas e paus.
pub const FACES: [[&str; 13]; 4] = [naipe!("copas"), naipe!("espadas"), naipe!("ouros"), naipe!("paus")];
pub const VERSO: &str = include_str!("../assets/baralho/verso.svg");

thread_local! {
    // Lidos uma vez: o mesmo `Svg` reaproveita a rasterização em cache.
    static SVGS: Vec<Svg> = FACES.iter().flatten().chain([&VERSO]).map(|f| Svg::from_bytes(f.as_bytes()).expect("carta embutida válida")).collect();
}

/// A face de uma carta: `naipe` na ordem de [`FACES`] (copas, espadas, ouros, paus), `valor` 1..=13.
pub fn svg_carta(naipe: u8, valor: u8) -> Svg {
    SVGS.with(|s| s[naipe as usize * 13 + valor as usize - 1].clone())
}

pub fn svg_verso() -> Svg {
    SVGS.with(|s| s[52].clone())
}
