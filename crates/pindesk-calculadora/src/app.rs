//! **La app Calculator PROMAX, si esta instalada.**
//!
//! Calculator PROMAX para Windows se publica como un ZIP con
//! `calculator-promax.exe` (su `release.yml`), sin instalador, sin registrar
//! un enlace propio (`promax://`) y sin leer argumentos (su `main.rs` solo
//! llama a `run_native`). Asi que:
//!
//! - se busca el exe donde lo dejaria una persona: en
//!   `%LOCALAPPDATA%\Programs\Calculator PROMAX` (donde PinDesk se instala a
//!   si misma), en `Archivos de programa`, o en el `PATH`;
//! - abrirla es solo arrancarla; quien la abre copia antes la expresion al
//!   portapapeles para pegarla alli con Ctrl+V.
//!
//! Puro: quien llama da las variables de entorno y dice si existe un
//! fichero, asi se prueba sin Windows.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// El ejecutable de Calculator PROMAX (el `[[bin]]` de `promax-app`).
pub const EXE: &str = "calculator-promax.exe";

/// Las carpetas de instalacion que se miran, por su variable de entorno.
const CARPETAS: &[(&str, &[&str])] = &[
    ("LOCALAPPDATA", &["Programs", "Calculator PROMAX"]),
    ("LOCALAPPDATA", &["Programs", "calculator-promax"]),
    ("LOCALAPPDATA", &["Calculator PROMAX"]),
    ("ProgramFiles", &["Calculator PROMAX"]),
];

/// **Donde esta `calculator-promax.exe`**, o `None` si no esta instalada.
pub fn buscar(
    variable: &dyn Fn(&str) -> Option<OsString>,
    existe: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let mut candidatos: Vec<PathBuf> = Vec::new();
    for (var, partes) in CARPETAS {
        if let Some(base) = variable(var).filter(|v| !v.is_empty()) {
            let mut p = PathBuf::from(base);
            p.extend(partes.iter());
            candidatos.push(p.join(EXE));
        }
    }
    if let Some(path) = variable("PATH") {
        candidatos.extend(std::env::split_paths(&path).map(|d| d.join(EXE)));
    }
    candidatos.into_iter().find(|p| existe(p))
}

/// [`buscar`] con el entorno y el disco de verdad.
pub fn instalada() -> Option<PathBuf> {
    buscar(&|v| std::env::var_os(v), &|p| p.is_file())
}

/// **Abre Calculator PROMAX** (en su carpeta, sin esperar a que se cierre).
pub fn abrir(exe: &Path) -> std::io::Result<()> {
    let mut c = std::process::Command::new(exe);
    if let Some(carpeta) = exe.parent() {
        c.current_dir(carpeta);
    }
    c.spawn().map(drop)
}
