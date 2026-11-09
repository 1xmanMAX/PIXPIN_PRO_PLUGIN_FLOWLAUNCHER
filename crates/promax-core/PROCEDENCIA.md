# De donde sale este crate

Es una **copia** de `promax-core`, el motor de algebra (CAS) de Calculator PROMAX, la
calculadora cientifica y algebraica del propio autor de PinDesk (estilo Casio ClassPad).

| | |
|---|---|
| Repositorio | `github.com/1xmanMAX/calculator-PROMAX` (privado) |
| Rama | `claude/advanced-math-calculator-app-m95pft` (la de por defecto) |
| Commit | `c85eb11c9826d9987f0bb08140c57668bb10e5fb` |
| Carpeta de origen | `crates/promax-core` (paquete `promax-core` 0.1.0) |
| Licencia | MIT, ver `LICENSE` (copiado de la raiz de calculator-PROMAX) |
| Copiado | 2026-11-08 |

## Por que copiado y no como dependencia git

Lo mismo que con `pdfsqueeze-core`: el repositorio es privado y una dependencia
`git = "..."` obliga a tener credenciales de GitHub para compilar. La CI y cualquier equipo
sin ellas no compilarian PinDesk, y `deny.toml` solo deja las dos fuentes git de los lectores
de planos. Copiado, compila en cualquier sitio. El motor no tiene ninguna dependencia, asi que
la copia no trae nada nuevo de crates.io (`cargo deny` solo ve un crate MIT mas).

Tampoco se copia la app entera (`promax-app`, egui): PinDesk tiene su propia interfaz y solo
usa el motor. Para la app completa esta «Abrir en Calculator PROMAX»
(`pindesk_calculadora::app`).

## Que se cambio respecto al original

**Nada del codigo.** `src/` es byte a byte el de calculator-PROMAX (sus pruebas, en
`src/tests.rs` y en los `mod tests` de cada modulo, incluidas). Lo propio es:

- **`Cargo.toml`**: alli la version, la edicion, la licencia y el `rust-version` venian del
  `[workspace.package]` de calculator-PROMAX; aqui estan escritos a mano con los mismos valores.
  La edicion se queda en **2021** (la suya), no en la 2024 del workspace de PinDesk.
- **Clippy apagado en este paquete** (`[lints.clippy] all = "allow"` en el `Cargo.toml`). En
  calculator-PROMAX Clippy es informativo (no bloquea su CI) y con el `-D warnings` de PinDesk
  saltan unos 90 avisos de estilo (`map_or` simplificable, floats con demasiados decimales,
  bucles por indice, metodos `add`/`mul` que se parecen a los de los operadores). Ninguno es un
  fallo; reescribirlos haria la copia distinta del original y cada actualizacion un lio. Los
  avisos de `rustc` siguen encendidos (no hay ninguno).
- **`rustfmt.toml`** con `disable_all_formatting = true`: su codigo no esta pasado por
  `cargo fmt` (lineas de hasta ~200 letras) y el `cargo fmt --all --check` de la CI de PinDesk
  lo reformatearia entero.

## Donde se usa

Solo a traves de `crates/pindesk-calculadora`, que pone alrededor lo de PinDesk (el `= ...` del
chat, la herramienta `calcular` del asistente, el texto de Flow Launcher, resolver sola una
ecuacion, el tiempo maximo y la pila del hilo que calcula). Lo enlazan `pindesk-ia`, la app
(`pixpinmax.exe`) y el plugin de Flow (`pindesk-lanzador.exe`). Ver `docs/integraciones.md`,
«Calculator PROMAX».

**Un panico mataria PinDesk**: el perfil release de PinDesk es `panic = "abort"` (el de
calculator-PROMAX es `unwind` justo para envolver cada celda en `catch_unwind`). El motor esta
escrito con cuidado (profundidad del parser limitada a 256, factoriales hasta 170!, Taylor hasta
orden 40, aritmetica `checked_`), y `pindesk-calculadora` ademas limita lo que entra (2 000
letras, 24 lineas), calcula en un hilo con 64 MB de pila y deja de esperar a los 3 s. Su prueba
`lo_raro_no_rompe_el_motor` pasa un monton de entradas raras sin que el motor entre en panico.
Si algun dia se ve uno, la salida es la de pdfsqueeze: calcular en otro proceso.

## Como actualizarlo

```sh
gh repo clone 1xmanMAX/calculator-PROMAX promax
cd promax && git log -1 --format=%H          # el commit nuevo
rm -rf "<pindesk>/crates/promax-core/src"
cp -r crates/promax-core/src "<pindesk>/crates/promax-core/"
cp LICENSE "<pindesk>/crates/promax-core/LICENSE"
```

Despues:

1. Comparar `crates/promax-core/Cargo.toml` y el `[workspace.package]` de calculator-PROMAX con
   el `Cargo.toml` de aqui. Si gano alguna dependencia, la prueba
   `promax_core_sigue_sin_dependencias` de `pindesk-calculadora` falla a proposito: decidir si
   vale la pena y pasarla por `cargo deny check`.
2. Apuntar el commit nuevo en la tabla de arriba.
3. `cargo test -p promax-core -p pindesk-calculadora -p pindesk-lanzador` y el Clippy de siempre.
