//! Poker do pack de jogos, sozinho numa janela; o jogo está em `pack/poker.rs`.
//! `cargo run --release --example poker`; `-- --capturar x.png` só gera o PNG.
#[path = "pack/estilo.rs"]
mod estilo;
#[path = "pack/cartas.rs"]
mod cartas;
#[path = "pack/poker.rs"]
mod poker;

fn main() -> Result<(), pelicano::Error> {
    poker::principal()
}
