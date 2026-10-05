//! `pixpin-lanzador.exe`: ver la cabecera de `lib.rs`.
//!
//! Subsistema de consola a proposito: Flow lo lanza sin ventana y con las
//! tuberias redirigidas, y a mano (`buscar`, `pedido`) escribe en la consola.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(pixpin_lanzador::ejecutar(&args));
}
