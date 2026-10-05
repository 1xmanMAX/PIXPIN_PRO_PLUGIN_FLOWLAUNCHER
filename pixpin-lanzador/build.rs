//! El exe arranca una vez por tecla (Flow 2.1.4, modo `Executable`): cada
//! milisegundo de arranque se nota. `user32.dll` (ventanas, portapapeles,
//! `WM_COPYDATA`) solo hace falta al mandar un pedido o mirar el
//! portapapeles, no al buscar; cargarla prepara la parte grafica del proceso
//! y cuesta unos milisegundos. Con `/DELAYLOAD` se carga la primera vez que
//! se llama a una de sus funciones (medido: ver `pruebas::medir_por_partes`
//! y el informe del 4-oct).

fn main() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|e| e == "msvc");
    if msvc {
        println!("cargo:rustc-link-arg-bin=pixpin-lanzador=/DELAYLOAD:user32.dll");
        println!("cargo:rustc-link-arg-bin=pixpin-lanzador=delayimp.lib");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
