//! Damas do pack de jogos, sozinha numa janela; o jogo está em `pack/damas.rs`.
//! `cargo run --release --example damas`; `-- --capturar x.png` só gera o PNG.
#[path = "pack/estilo.rs"]
mod estilo;
#[allow(dead_code)] // o menu e o exemplo sozinho usam partes diferentes
#[path = "pack/damas.rs"]
mod damas;

fn main() -> Result<(), pelicano::Error> {
    damas::principal()
}
