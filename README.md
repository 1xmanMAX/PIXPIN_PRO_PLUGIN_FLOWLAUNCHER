# PinDesk para Flow Launcher

Plugin de [Flow Launcher](https://www.flowlauncher.com/) para **[PinDesk](https://github.com/1xmanMAX/PINDESK-)**:
busca y abre tus chats, tareas, lecciones, notas y archivos de PinDesk, apunta tareas y calcula, sin salir de Flow.

> **Necesita PinDesk instalado** en el mismo PC: el plugin lee lo que PinDesk guarda en el disco y, para cambiar
> algo, se lo pide a la app. Descarga PinDesk en <https://github.com/1xmanMAX/PINDESK->.

## Qué hace

Todo empieza con la palabra clave **`p`**:

| Escribes | Hace |
|---|---|
| `p geometría` | Busca en chats, tareas, lecciones, notas y archivos de PinDesk |
| `p t comprar planos mañana #obra` | Apunta una tarea (entiende fecha, urgencia y #etiquetas) |
| `p = 2x+3=7` | Calcula con el motor de Calculator PROMAX: `x = 2` (Intro copia el resultado) |
| `p calc 15% de 230` | Lo mismo con la palabra `calc` |
| `p geom` | Una asignatura: su próxima clase y sus pendientes |

## Instalar

- **Desde Flow**: escribe `pm install PinDesk` (cuando esté en la tienda de plugins).
- **A mano**: descarga `PinDesk-<versión>.zip` de
  [Releases](https://github.com/1xmanMAX/PIXPIN_PRO_PLUGIN_FLOWLAUNCHER/releases) y descomprímelo en
  `%APPDATA%\FlowLauncher\Plugins\PinDesk`; reinicia Flow.

## Compilar

```powershell
cargo build --release -p pindesk-lanzador
```

Las versiones se publican solas al subir una etiqueta `vX.Y.Z` igual a la `Version` de
`pindesk-lanzador/plugin/plugin.json` (`.github/workflows/release.yml`).

## Licencia

MIT (`LICENSE`). `crates/promax-core` es el motor de Calculator PROMAX, también MIT (su `LICENSE`).

---

## English

Flow Launcher plugin for **PinDesk**: search and open your PinDesk chats, tasks, lessons, notes and files, add
tasks and calculate (`p = 2x+3=7`) without leaving Flow. **Requires PinDesk installed** on the same PC
(<https://github.com/1xmanMAX/PINDESK->). Install with `pm install PinDesk` once it is in the plugin store, or
unzip a release into `%APPDATA%\FlowLauncher\Plugins\PinDesk`. MIT licensed.
