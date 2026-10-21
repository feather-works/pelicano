//! Buraco do pack de jogos, sozinho numa janela; o jogo está em `pack/buraco.rs`.
//! `cargo run --release --example buraco`; `-- --capturar x.png` só gera o PNG.
#[path = "pack/estilo.rs"]
mod estilo;
#[path = "pack/cartas.rs"]
mod cartas;
#[path = "pack/buraco.rs"]
mod buraco;

fn main() -> Result<(), pelicano::Error> {
    buraco::principal()
}
