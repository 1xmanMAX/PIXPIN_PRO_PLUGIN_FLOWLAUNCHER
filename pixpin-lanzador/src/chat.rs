//! **El chat de un proyecto, dentro de Flow**: `p <proyecto> > [filtro]`.
//!
//! El usuario, 2-oct: «si entro a un proyecto a traves del Flow Launcher, que
//! como primera opcion me salga "abrir proyecto en app" y las demas sean el
//! contenido del chat ordenado de ultimo a primero — todo: audios, archivos,
//! fotos y aun mensajes, como un pequeno chat».
//!
//! - Intro en un proyecto (en la busqueda) ya no abre la app: entra aqui.
//! - Arriba, «Abrir proyecto en la app» y «Añadir arrastrando»; debajo, los mensajes del mas nuevo
//!   al mas viejo (sin los del buzon), como [`Vista::Chat`].
//! - Lo escrito tras `>` filtra y, a la vez, es un mensaje: «Escribir en
//!   «P»: …» lo manda a ese chat (con las imagenes y ficheros pegados como
//!   `[img NN]` y `[archivo NN]`). Las dos filas fijas bajan al final,
//!   para que Intro abra lo encontrado (y el recuadro, con ella).
//! - El nombre se escribe con `>` cambiado por `›` y, si dos proyectos se
//!   llaman igual, el segundo con ` · 2` (como las listas de tareas).

use crate::consulta::SEPARADOR;
use crate::datos::Proyecto;
use crate::normalizar::{SALTEADO, normalizar, puntuar};
use crate::resultados::{
    Accion, Contexto, Resultado, Vista, abrir_proyecto, de_mensaje, glifo, listas, pedido,
    resultado_arrastrar,
};
use serde_json::json;

/// Cuantos resultados, como mucho, en el chat de un proyecto.
pub const MAXIMO_CHAT: usize = 50;

/// Lo que se escribe para entrar en cada proyecto (en el orden de
/// `proyectos`): su nombre, sin `>`, y con un numero si se repite.
pub fn etiquetas(proyectos: &[Proyecto]) -> Vec<String> {
    let mut v: Vec<String> = Vec::with_capacity(proyectos.len());
    let mut vistas: Vec<String> = Vec::with_capacity(proyectos.len());
    for p in proyectos {
        let base = p.nombre.replace(SEPARADOR, "›");
        let base = if base.trim().is_empty() {
            "Proyecto".to_string()
        } else {
            base.trim().to_string()
        };
        let n = normalizar(&base);
        let antes = vistas.iter().filter(|x| **x == n).count();
        vistas.push(n);
        v.push(if antes == 0 {
            base
        } else {
            format!("{base} · {}", antes + 1)
        });
    }
    v
}

/// La consulta que entra en el chat de un proyecto: `p <etiqueta> > `.
pub fn consulta_de(etiqueta: &str, ctx: &Contexto) -> String {
    ctx.consulta(&format!("{etiqueta} {SEPARADOR} "))
}

/// El proyecto cuya etiqueta es exactamente lo escrito (sin tildes ni
/// mayusculas).
pub fn exacto(proyectos: &[Proyecto], texto: &str) -> Option<usize> {
    let q = normalizar(texto.trim());
    if q.is_empty() {
        return None;
    }
    etiquetas(proyectos).iter().position(|e| normalizar(e) == q)
}

/// `<algo> > ...` sin nombre exacto: el proyecto que mejor encaje.
pub fn buscar_y_entrar(
    proyectos: &[Proyecto],
    texto: &str,
    filtro: &str,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let q = normalizar(texto.trim());
    let mejor = (!q.is_empty())
        .then(|| {
            proyectos
                .iter()
                .enumerate()
                .map(|(i, p)| (i, puntuar(&q, &normalizar(&p.nombre), "")))
                .filter(|(_, s)| *s > 0)
                .max_by(|a, b| {
                    a.1.cmp(&b.1)
                        .then(proyectos[a.0].tocado.cmp(&proyectos[b.0].tocado))
                })
                .map(|(i, _)| i)
        })
        .flatten();
    match mejor {
        Some(i) => en_proyecto(proyectos, i, filtro, ctx),
        None => vec![Resultado::nuevo(
            format!("No encuentro el proyecto «{}»", texto.trim()),
            "Intro: buscarlo en todo",
            glifo::AVISO,
            Accion::Consulta(ctx.consulta(texto.trim())),
        )],
    }
}

/// **«Escribir en «P»: texto»**: el pedido `chat` a ese proyecto, con las
/// imagenes y los ficheros pegados (`[img NN]`, `[archivo NN]`, ver
/// `imagenes::con_imagenes`).
fn escribir_en(p: &Proyecto, texto: &str, aqui: &str, ctx: &Contexto) -> Resultado {
    let mut r = Resultado::nuevo(
        format!("Escribir en «{}»: {texto}", p.nombre),
        "Intro: mandarlo a su chat",
        glifo::CHAT,
        Accion::Pedido(pedido(
            "chat",
            json!({ "texto": texto, "proyecto": p.id_para_pedido() }),
        )),
    );
    r.autocompletar = Some(format!("{aqui}{texto}"));
    crate::imagenes::con_imagenes(&mut r, texto, ctx);
    r
}

/// El chat del proyecto `i`.
pub fn en_proyecto(
    proyectos: &[Proyecto],
    i: usize,
    filtro: &str,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let p = &proyectos[i];
    let etiqueta = &etiquetas(proyectos)[i];
    let aqui = consulta_de(etiqueta, ctx);
    let filtro = filtro.trim();
    let q = normalizar(filtro);

    let mut abrir = Resultado::nuevo(
        "Abrir proyecto en la app",
        format!("{} · Intro: su chat en PixPin", p.nombre),
        glifo::PIXPIN,
        abrir_proyecto(p),
    );
    abrir.autocompletar = Some(aqui.clone());
    abrir.con_menu(json!({
        "tipo": "proyecto",
        "proyecto": p.id_para_pedido(),
        "carpeta": p.carpeta.to_string_lossy(),
        "consulta": aqui,
    }));

    // Del mas nuevo al mas viejo; a igual hora, el que va despues en el
    // fichero (el ultimo escrito).
    let mut orden: Vec<(usize, &crate::datos::Mensaje)> = p
        .mensajes
        .iter()
        .enumerate()
        .filter(|(_, m)| !m.en_buzon)
        .collect();
    orden.sort_by(|a, b| b.1.cuando.cmp(&a.1.cuando).then(b.0.cmp(&a.0)));

    let todas = listas(proyectos);
    let mut arrastrar = resultado_arrastrar(Some(p));
    arrastrar.autocompletar = Some(aqui.clone());
    arrastrar.contexto = abrir.contexto.clone();
    // Las dos filas fijas: abrir en la app y el recuadro para soltar.
    let fijas = 2;
    let mut v = Vec::new();
    if q.is_empty() {
        v.push(abrir.clone());
        v.push(arrastrar.clone());
    }
    for (_, m) in orden {
        if v.len() >= MAXIMO_CHAT - if q.is_empty() { 0 } else { fijas + 1 } {
            break;
        }
        let Some(pz) = de_mensaje(proyectos, i, m, Vista::Chat, &todas, ctx) else {
            continue;
        };
        if !q.is_empty()
            && puntuar(&q, &normalizar(&pz.nombre), &normalizar(&pz.dentro)) <= SALTEADO
        {
            continue;
        }
        let mut r = pz.resultado;
        if r.autocompletar.is_none() {
            r.autocompletar = Some(format!("{aqui}{}", r.titulo));
        }
        v.push(r);
    }
    if !q.is_empty() {
        // Lo escrito, como mensaje para este chat. Con fichas pegadas
        // (`[img NN]`, `[archivo NN]`) es un mensaje seguro: arriba. Si no,
        // debajo de lo encontrado (o arriba, si no se encontro nada).
        let escribir = escribir_en(p, filtro, &aqui, ctx);
        let con_fichas = !crate::imagenes::fichas(filtro).is_empty()
            || !crate::imagenes::fichas_de_archivo(filtro).is_empty();
        if con_fichas {
            v.insert(0, escribir);
        } else {
            v.push(escribir);
        }
        v.push(abrir);
        v.push(arrastrar);
    } else if v.len() == fijas {
        v.push(Resultado::nuevo(
            "El chat está vacío",
            "Nada que enseñar todavía",
            glifo::CHAT,
            abrir_proyecto(p),
        ));
    }
    v
}
