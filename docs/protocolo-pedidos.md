# Pedidos a PixPin Max (protocolo v1)

Cualquier programa del mismo equipo puede pedirle cosas a la PixPin Max que esta abierta:
abrir un proyecto, escribir en el chat, crear un lienzo, grabar, añadir una tarea... Lo usa
el plugin de Flow Launcher (`apps/pixpin-lanzador`), y sirve igual para un script propio
(AutoHotkey, PowerShell, otro lanzador).

## Transporte

- `WM_COPYDATA` a la ventana de clase **`PixPinMaxVentanaMensajes`** (`FindWindowW`), con
  `dwData = 0x5049_5851` y en `lpData` el **JSON en UTF-8, sin nulo final**, de 64 KB como
  mucho. Se manda con `SendMessageW` (sincrono) y despues se le postea `WM_NULL` para
  despertar su bucle.
- Respuesta inmediata (`LRESULT`): `1` aceptado (en cola), `2` version no soportada, `3` JSON
  roto o accion desconocida, `0` no era para nosotros o no hay nadie escuchando.
- El trabajo de verdad lo hace el bucle principal de PixPin despues: lo que salga mal lo
  dice PixPin con su aviso, no la respuesta.
- Si no hay ventana, PixPin no esta abierta: se arranca
  `%LOCALAPPDATA%\Programs\PixPinMax\pixpinmax.exe` y se espera a la ventana (hasta 8 s).

## El pedido

```json
{ "pixpin": 1, "accion": "<accion>", ... }
```

- `pixpin` es la version. Un campo nuevo opcional no la sube; quitar o cambiar uno si.
- Lo que no se conoce se ignora.
- `proyecto` es el `id` de la carpeta del proyecto (`proyectos/<id>`, el de
  `proyectos/indice.json`). `null` o ausente = **Mensajes guardados**.
- `codigo` es el `id` del mensaje dentro de `guardados.jsonl`.

| accion | campos | que hace |
|---|---|---|
| `abrir` | `que: {tipo:"proyecto", proyecto}` | El chat en ese proyecto |
| `abrir` | `que: {tipo:"mensaje", proyecto, codigo}` | El chat en ese mensaje, resaltado |
| `abrir` | `que: {tipo:"hoja", proyecto, referencia}` | Ese lienzo u hoja, en el lienzo |
| `abrir` | `que: {tipo:"nota", proyecto, codigo}` | Esa nota en el editor de notas |
| `abrir` | `que: {tipo:"fichero", ruta}` | Como «Abrir con PixPin» (lector, editor, pin). Un `.leccion` abre la ficha de esa lección |
| `chat` | `texto`, `proyecto?`, `imagenes?`, `archivos?` | Un mensaje de texto nuevo en ese chat. Con `imagenes` (rutas; `[img 01]` en `texto` = `imagenes[0]`), el texto sin sus fichas va primero y cada imagen detras como una foto del chat (un BMP se guarda como PNG). Con `archivos` (ver abajo), cada fichero va detras como un adjunto
| `nota_nueva` | `texto?`, `proyecto?`, `imagenes?` | Una nota nueva, abierta en el editor. Con `imagenes`, cada `[img NN]` del texto queda como esa imagen dentro de la nota
| `lienzo_nuevo` | `nombre?`, `proyecto?` | Un lienzo nuevo, abierto |
| `grabar` | `nombre`, `proyecto?` | Saca el micrófono flotante y graba una nota de voz; un clic la guarda con ese nombre (`<nombre>.m4a`) en ese chat |
| `lista_nueva` | `titulo`, `proyecto?` | Una lista de tareas nueva (mini-app `tareas`) |
| `anadir_tarea` | `texto`, `proyecto?`, `codigo?`, `imagenes?` | Añade `- [ ] texto` (con la fecha de hoy). **Sin `proyecto` ni `codigo`**: a la lista **«Inbox»** de «Mensajes guardados» (se crea si no esta), como la caja de la ventana de tareas. Con `codigo`: a esa lista. Con `proyecto` y sin `codigo`: a la ultima lista de tareas de ese chat, o a una nueva «Tareas». Con `imagenes` (ver abajo), la tarea lleva esas imagenes |
| `marcar_tarea` | `proyecto?`, `codigo`, `indice`, `hecha` | Marca (o desmarca) la tarea numero `indice` (desde 0, en el orden del documento) de la lista `codigo` |
| `mover_tarea` | `proyecto?`, `codigo`, `indice`, `a_proyecto?`, `a_codigo` | Pasa la tarea numero `indice` (desde 0, en el orden del documento) de la lista `codigo` (del chat `proyecto`) al final de la lista `a_codigo` (del chat `a_proyecto`; ausente = «Mensajes guardados»), con su fecha y su estado, como «Mover a…» de la ventana de tareas. Si la tarea cambio entre medias no se toca nada y se avisa |
| `quitar_tarea` | `proyecto?`, `codigo`, `indice` | Quita la tarea numero `indice` (desde 0, en el orden del documento) de la lista `codigo`, como su aspa en la ventana de tareas (`Tareas.borrar` del móvil). La lista se reescribe en su mensaje y asi viaja al móvil. Si no hay esa tarea, o `codigo` no es una lista, no se toca nada y se avisa |
| `borrar_lista` | `proyecto?`, `codigo` | Borra la lista `codigo` **entera**, con todas sus tareas: su mensaje sale del chat por el mismo camino que «Borrar» en el chat (sale de `guardados.jsonl` y deja su marca en `sincro\borrados.jsonl`, para que el móvil tambien la quite en la siguiente vuelta). No se pregunta ni se puede deshacer: quien lo manda ya pregunto. Las imagenes de sus tareas se quedan en `archivos/`. El Inbox tambien se puede borrar: se vuelve a crear con la siguiente tarea apuntada. Si `codigo` no es una lista, no se toca nada y se avisa |
| `ventana` | `cual: "tareas" \| "galeria" \| "lecciones" \| "sincronizar"` | Abre (o trae delante) esa ventana, como su entrada de la bandeja. Otro `cual` es un pedido roto |
| `capturar` | `modo?: "zona"` | Empieza la captura de una zona, la misma del atajo general («Capturar» de la bandeja), unos 300 ms despues para que el lanzador ya se haya escondido. Sin `modo`, `"zona"`; otro modo es un pedido roto |
| `pinear_ultima` | — | Saca a la pantalla como pin la captura mas reciente de la carpeta de capturas (la primera de la galeria), como `pinear` con su `ruta`. Sin capturas, se avisa |
| `conservar_captura` | `ruta` | Como «Conservar» de la galeria: la captura entra en «Mensajes guardados» y deja de caducar. Solo vale una `ruta` que este en la carpeta de capturas (`<raiz>\capturas\`); otra se rechaza con aviso |
| `borrar_captura` | `ruta` | Como «Borrar» de la galeria: la captura va a la papelera de PixPin (`<raiz>\papelera\capturas\`) y, si estaba conservada, se olvida. Solo una `ruta` de la carpeta de capturas |
| `copiar_imagen` | `ruta` | Pone esa imagen (png, jpg, bmp, webp) en el portapapeles **como imagen**, no como fichero. Un GIF o algo que no es imagen se rechaza con aviso |
| `ventana_principal` | — | Saca el chat |
| `pinear` | `ruta` o (`proyecto?`, `codigo`) | Saca ese fichero (o el del mensaje) a la pantalla como pin, como «Sacar a la pantalla» del chat |
| `iconos` | `extensiones: ["pdf","docx",...]` | Pinta el icono de archivo de cada extension (el del chat: hoja de su color con la extension escrita) en `<raiz>/cache/iconos-de-extension/<ext>.png` (64 px, fondo transparente) si aun no esta. No abre nada ni avisa |
| `soltar` | `proyecto?` | Saca una caja flotante (siempre encima) del proyecto: los ficheros que se le suelten entran en su chat como si se soltaran en la ventana del chat. Una por proyecto; pedirla otra vez la trae delante. Se cierra con su aspa o Esc |
| `reproducir` | `ruta`, `titulo?` | Hace sonar un audio (m4a, mp3, wav…) en un reproductor flotante siempre encima, sin abrir el chat: atras 10 s, play/pausa, adelante 10 s, velocidad, pista para saltar y aspa. Espacio pausa, flechas saltan, Esc cierra. Se cierra sola al acabar. Si ya hay uno abierto, cambia de pista. `titulo` es lo que se lee; sin el, el nombre del fichero |
| `leccion_nueva` | `texto?`, `proyecto?`, `imagenes?` | Abre la ficha de una lección aprendida nueva, como «Nueva lección» del móvil. Con `texto`, se reparte solo («pasó que…, porque…, la próxima vez…») y se proponen etiquetas, área, tipo y causas. Nada se guarda hasta pulsar Guardar o Esc (que guarda, como el «atrás» del móvil); la «×» descarta. Irá al chat de `proyecto` (sin él, «Mensajes guardados»). `imagenes` (rutas absolutas, como en `anadir_tarea`) son **las fotos de la lección**: se leen al recibir el pedido, la ficha dice «📎 N fotos» y, al guardar, cada una va a `archivos/` del chat (`leccion-<ms>-NN.<ext>`, un `.bmp` como `.png`) con su mensaje `IMAGEN` «Foto de la lección» que **responde** al de la lección (`respondeA: "lec-<id>"`), y su id entra en `adjuntos` del `.leccion`, como hace el móvil. Las fichas `[img 01]`… salen del texto. Si falta una imagen o no lo es, no se abre nada y se avisa |
| `lecciones` | `proyecto?`, `consulta?` | Abre la lista de lecciones (buscador, repaso de hoy, lista de comprobación). Con `proyecto`, primero las suyas y las que hablan de lo mismo; con `consulta`, ya buscando. Una sola ventana: pedirla otra vez la trae delante |
| `enviar_al_movil` | `aparato`, `imagenes?`, `archivos?` | Manda esas imagenes (rutas absolutas) **al lienzo que el móvil `aparato` tiene abierto** (`aparato` es su `id` en el grupo, `sincro\identidad.json`), sin abrir Sincronizar en ninguno de los dos. Ver abajo. Sin nada que mandar, o con un fichero que no esta, no sale nada y se avisa |

### Una imagen al lienzo del móvil (`enviar_al_movil`, 6-oct-2026)

```json
{ "pixpin": 1, "accion": "enviar_al_movil", "aparato": "6f1c…-…",
  "imagenes": ["C:\\Users\\yo\\AppData\\Roaming\\PixPinMax\\cache\\lanzador-imagenes\\img-1.png"] }
```

- La app contesta «aceptado» al momento y lo hace **en un hilo** (`sincronizar::al_movil`):
  busca al móvil en su ultima direccion (`sincro\direcciones.txt`, un `PING` de un segundo) y,
  si no contesta, por mDNS (`_pixpin._tcp`, por el `id` y la etiqueta del grupo de su TXT);
  conecta por el canal cifrado del grupo (puerto 47474), saluda y manda cada fichero con la
  peticion `suelto` (`crates/pixpin-sincro/src/al_lienzo.rs`).
- El móvil la pone **en el centro del lienzo que tenga delante** y la guarda; si no tiene un
  lienzo abierto, va a su Conversación general. Lo que no es una imagen (`archivos`) va siempre
  a su chat.
- Lo que pasó se dice en el globo al acabar: «Enviada al lienzo de Pixel», «Pixel la guardó en
  el chat (no tenía un lienzo abierto)», «Pixel no está abierto: abre PixPin en el móvil» (solo
  escucha con una pantalla de PixPin a la vista). Un PixPin del móvil anterior a esto contesta
  «No sé qué es «suelto»»: se dice «Actualiza PixPin en Pixel…» y se abre «Enviar por Wi-Fi» con
  los mismos ficheros, el envío de siempre.
- En el lanzador: `p móvil` (o `enviar`, `celular`, `m`) + Ctrl+V de la imagen (`[img 01]`):
  sale una fila por aparato del grupo; Intro manda. Lo escrito detras de las fichas elige el
  móvil por su nombre (`p m [img 01] pixel`). Sin grupo, la fila abre Sincronizar
  (`ventana {cual:"sincronizar"}`).
- Lo que tiene que hacer el móvil: `docs/investigacion/2026-10-06-foto-al-lienzo-android.md`.

### Tareas con imagenes (`anadir_tarea.imagenes`)

`imagenes` es una lista de **rutas absolutas** de imagenes (png, jpg, jpeg, bmp, gif, webp) de
este equipo. En `texto`, las fichas `[img 01]`, `[img 02]`… (desde 1, en el orden de la lista:
`[img 01]` es `imagenes[0]`) marcan donde va cada una:

```json
{ "pixpin": 1, "accion": "anadir_tarea", "texto": "comprar yeso [img 01]",
  "imagenes": ["C:\\Users\\yo\\AppData\\Roaming\\PixPinMax\\cache\\lanzador-imagenes\\pegada-1.bmp"] }
```

- PixPin **copia** cada imagen a `archivos/` del chat de la lista (con nombre propio,
  `tarea-<ms>-<n>.<ext>`; un `.bmp` se guarda como `.png`) y cambia su ficha por
  `![img 01](pixpin:files/guardados/pc/<chat>/archivos/tarea-…png)`. El original no se toca y
  la tarea no depende de el: se puede borrar despues.
- La imagen cuya ficha no esta en `texto` va al final del texto (antes de la fecha). Una
  ficha sin imagen (`[img 05]` con tres imagenes) se queda como texto.
- Un `texto` vacio vale si hay imagenes: la tarea es solo la imagen.
- Si falta alguna imagen o alguna no es una imagen, **no se apunta nada ni se copia nada** y se
  avisa («No se encuentra la imagen …»). Igual si `codigo` no es una lista.
- El formato de la tarea esta en `docs/investigacion/2026-10-03-tareas-con-imagenes-android.md`.

### Ficheros en un mensaje del chat (`chat.archivos`, 4-oct-2026)

`archivos` es una lista de **rutas absolutas** de ficheros cualesquiera (un PDF, un Excel, un
`.dwg`…). En `texto`, `[archivo 01]`, `[archivo 02]`… (desde 1, en el orden de la lista) marcan
cada uno y se quitan del texto:

```json
{ "pixpin": 1, "accion": "chat", "proyecto": "pr-1789412424738",
  "texto": "el plano [archivo 01] y la foto [img 01]",
  "imagenes": ["C:\\Users\\yo\\AppData\\Roaming\\PixPinMax\\cache\\lanzador-imagenes\\img-1.png"],
  "archivos": ["C:\\Users\\yo\\Desktop\\plano.pdf"] }
```

- Va primero el texto sin fichas (si queda algo), luego las imagenes como fotos y luego cada
  fichero como un adjunto con su nombre, **igual que al soltarlo en el chat**
  (`ventana_chat::meter_en_proyecto`): se copia a `archivos/` del proyecto.
- Es un campo aparte de `imagenes` a proposito: una imagen se guarda como foto (un BMP pasa a
  PNG) y un fichero tal cual, sin mirar su extension.
- Si falta un fichero o es una carpeta, **no se escribe nada** y se avisa («No existe …»).
- Solo `chat` lo entiende. En el lanzador, una ficha `[archivo NN]` en una tarea, una nota o una
  leccion se queda como texto (y lo dice el subtitulo).
- De donde salen: con Flow delante y ficheros copiados en el Explorador, el Ctrl+V lo atrapa la
  app (`pegar_en_flow`) y escribe ` [archivo 01] ` por cada fichero (una imagen copiada como
  fichero sigue siendo ` [img NN] `). Un fichero, un nombre: el mismo dos veces no se repite. El
  borrador (`<raiz>\cache\lanzador-imagenes\borrador.json`, campo `archivos`) guarda la ruta
  original; no se copia nada hasta mandar el mensaje. Se escribe en el chat de un proyecto con
  `p <proyecto> > texto` (la fila «Escribir en «P»: …») o, a proposito, con `p chat texto`.

`grabar` no abre el chat: saca un **microfono flotante** siempre encima, con la señal de que
graba y el tiempo; un clic lo para y lo guarda (con su `nombre`) en el chat pedido. Al
guardarse ofrece **«Convertir en llamada»** con un selector de hora (la llamada secreta de
PixPin) y desaparece al acabar (o sola a los 10 s). Esc mientras graba la descarta sin preguntar.

## Donde leer (sin pedir nada)

Buscar se hace leyendo el disco, sin pasar por la app (funciona aunque este cerrada). La raiz
es `%APPDATA%\PixPinMax` (o la carpeta del exe si al lado hay un `pixpinmax.toml`):

- `proyectos/indice.json`: `{"proyectos":[{id, nombre, tocado, hojas, guardados?, ...}]}`.
  El de `guardados: true` es «Mensajes guardados».
- `proyectos/<id>/guardados.jsonl`: un mensaje por linea (`id, cuando, clase, texto, nombre,
  ruta, referencia, miniapp, transcripcion, enBuzon...`). `clase`: `NOTA, IMAGEN, ARCHIVO,
  VOZ, DIBUJO, PAGINA, PROYECTO, MINIAPP`. Las listas de tareas son `MINIAPP` con
  `miniapp:"tareas"` y el documento en `texto` (`# Titulo` y casillas `- [ ] x` / `- [x] x`).
  Una tarea puede acabar en `➕ AAAA-MM-DD` (su fecha de creacion) y llevar delante imagenes
  `![img 01](pixpin:files/…)` (`mini::partir` y `mini::imagenes_de` las separan del texto).
- `proyectos/<id>/proyecto.json`: las hojas del proyecto.
- Las **lecciones aprendidas**: cada una es un mensaje `ARCHIVO` con id `lec-<id>` cuya `ruta`
  acaba en `.leccion` (normalmente `pixpin:files/guardados/lecciones/<id>.leccion`, que vive en
  `proyectos/<id>/android/guardados/lecciones/`). Ese fichero es la verdad: JSON en camelCase
  (`titulo, quePaso, porQue, proxima, tipo, area, gravedad, etiquetas, etiquetasAuto,
  referencias, causas, repeticiones, caja, repasar, adjuntos…`, ver `crates/pixpin-lecciones`).
  `adjuntos` son ids de mensajes `IMAGEN`/`VOZ` del mismo chat con `respondeA: "lec-<id>"`.
  Toca repasarla cuando `repasar <= ahora` (ms).
- Nunca escribir en esos ficheros desde fuera: la app los protege con un cerrojo que solo
  existe dentro de ella. Para cambiar algo, un pedido.

## Desde la linea de mandatos

`pixpin-lanzador.exe pedido '<json>'` manda un pedido (arranca PixPin si hace falta) y
`pixpin-lanzador.exe buscar "<texto>"` escribe los resultados, una linea JSON por resultado.
Sin argumentos es el plugin de Flow Launcher (JSON-RPC por stdin/stdout).
