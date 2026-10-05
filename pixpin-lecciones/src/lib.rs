//! **Las lecciones aprendidas** (puerto de `lecciones/` de PixPin Android,
//! 3-oct-2026).
//!
//! Lo que no depende de ventanas ni de disco, copiado del Kotlin actual para
//! que el telefono y el PC hagan lo mismo con la misma leccion:
//!
//! - [`leccion`]: la ficha (`Leccion.kt`), su JSON y el repaso espaciado
//!   (`Repaso`), mas el resumen que lleva el mensaje del chat
//!   (`LeccionesStore.resumen`).
//! - [`texto`]: las palabras como las compara el buscador (`Texto.kt`).
//! - [`etiquetador`]: las etiquetas, el area, el tipo y las causas que se
//!   proponen solas (`Etiquetador.kt`).
//! - [`buscador`]: buscar, «se parece a una que ya tienes», relacionadas y el
//!   aviso por contexto (`Buscador.kt`).
//! - [`dictado`]: una frase dicha de corrido, repartida en sus campos
//!   (`Dictado.kt`).
//! - [`rapida`]: **lo que el PC anade (v2)**: la barra «¿Que aprendiste?»
//!   que reparte una frase y propone area, gravedad y proyecto.
//! - [`fusion`]: **lo que el PC anade**: cuando la sincronizacion trae otra
//!   version de una leccion que tambien cambio aqui, se juntan en vez de
//!   pisarse (ver el modulo).
//!
//! Donde vive cada leccion (un archivo `guardados/lecciones/<id>.leccion` y un
//! mensaje del chat que lo senala) lo explica `docs/lecciones.md` del movil;
//! aqui solo se lee y se escribe el JSON.

#![forbid(unsafe_code)]

pub mod buscador;
pub mod dictado;
pub mod etiquetador;
pub mod fusion;
pub mod leccion;
pub mod rapida;
pub mod texto;

pub use buscador::{Indice, Resultado};
pub use leccion::{Leccion, Nota, Repaso};

#[cfg(test)]
mod pruebas;
