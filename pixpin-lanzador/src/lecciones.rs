//! **Las lecciones aprendidas en Flow**: buscarlas (`p lecciones <texto>`),
//! las que tocan repasar hoy (`p repasar`) y que salgan tambien al buscar en
//! todo, debajo de lo que coincide de verdad.
//!
//! Se leen del disco como lo demas ([`crate::datos`]: el `.leccion` de cada
//! mensaje `lec-<id>`), y se buscan y se repasan con el crate de la app
//! (`pixpin-lecciones`): el mismo buscador del movil (sin tildes, sin
//! plurales, con erratas y por significado cercano) y la misma regla del
//! repaso (`Repaso::de_hoy`: las que tocan, la mas grave primero, cinco al
//! dia). Asi Flow ensena lo mismo que la ventana de Lecciones.
//!
//! Intro abre la ficha de la leccion (pedido `abrir` del fichero `.leccion`,
//! que la app convierte en su ficha). El menu: verla en la lista de la app,
//! copiar su texto y su carpeta.

use crate::datos::{self, Proyecto};
use crate::normalizar::{normalizar, puntuar};
use crate::resultados::{
    Accion, Contexto, Resultado, glifo, hace, para_ayuda, pedido, resultado_ventana,
};
use pixpin_lecciones::leccion::{TIPO_ACIERTO, TIPO_ERROR};
use pixpin_lecciones::{Leccion, Repaso, buscador};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Cuantas tocan repasar cada dia: las de la ventana de Lecciones (y del
/// movil, `Repaso.deHoy`).
pub const DE_HOY: usize = 5;
/// Cuantas, como mucho, salen al buscar en todo.
pub const EN_TODO: usize = 5;

/// Una leccion con donde esta.
#[derive(Debug, Clone)]
pub struct LeccionEn {
    /// El indice de su proyecto (su chat).
    pub proyecto: usize,
    pub mensaje: String,
    /// Su fichero `.leccion`.
    pub ruta: PathBuf,
    pub leccion: Leccion,
    /// Sus fotos que estan en este equipo, en su orden.
    pub fotos: Vec<PathBuf>,
}

/// **Todas las lecciones**, de la mas tocada a la menos. Una leccion que
/// salga dos veces (dos guardados a la vez, o lo mismo llegado por dos
/// caminos) se ensena una sola vez, como en el movil.
pub fn todas(proyectos: &[Proyecto]) -> Vec<LeccionEn> {
    let mut v: Vec<LeccionEn> = Vec::new();
    for (i, p) in proyectos.iter().enumerate() {
        for l in &p.lecciones {
            let Some(leccion) = Leccion::de_valor(&l.json) else {
                continue;
            };
            let fotos = leccion
                .adjuntos
                .iter()
                .filter_map(|id| p.mensajes.iter().find(|m| m.id == *id))
                .filter(|m| m.clase == "IMAGEN")
                .filter_map(|m| m.ruta.as_deref().and_then(|r| p.ruta_real(r)))
                .filter(|r| datos::existe(r))
                .collect();
            v.push(LeccionEn {
                proyecto: i,
                mensaje: l.mensaje.clone(),
                ruta: l.ruta.clone(),
                leccion,
                fotos,
            });
        }
    }
    v.sort_by_key(|x| std::cmp::Reverse(x.leccion.tocada));
    let mut vistas = std::collections::HashSet::new();
    v.retain(|l| vistas.insert(l.leccion.id.clone()));
    v
}

/// Lo que se busca en ella por su nombre (el titulo) y por dentro (lo demas).
fn texto_entero(l: &Leccion) -> String {
    [
        l.que_paso.as_str(),
        l.por_que.as_str(),
        l.proxima.as_str(),
        l.area.as_str(),
        &l.todas_las_etiquetas().join(" "),
        &l.referencias.join(" "),
        &l.causas.join(" "),
    ]
    .join("\n")
}

/// **Buscar**: las que encuentra el buscador de las lecciones (el del movil)
/// en su orden y, detras, las que solo casan letra a letra (lo que el
/// buscador no mira: una o dos letras, una palabra vacia). Cada una con sus
/// puntos de Flow ([`puntuar`], de 0 a [`crate::normalizar::IGUAL`]).
pub fn buscar<'a>(todas: &'a [LeccionEn], texto: &str) -> Vec<(u32, &'a LeccionEn)> {
    let q = normalizar(texto.trim());
    if q.is_empty() {
        return Vec::new();
    }
    let indices: Vec<buscador::Indice> = todas
        .iter()
        .map(|l| buscador::Indice::nuevo(l.leccion.clone()))
        .collect();
    let mut v: Vec<(u32, &LeccionEn)> = Vec::new();
    for r in buscador::buscar(&indices, texto) {
        if let Some(l) = todas.iter().find(|l| l.leccion.id == r.leccion.id) {
            let letra = puntuar(
                &q,
                &normalizar(&l.leccion.titulo),
                &normalizar(&texto_entero(&l.leccion)),
            );
            // Lo que solo encuentra por significado o con erratas, por lo bajo.
            v.push((letra.max(crate::normalizar::SALTEADO + 50), l));
        }
    }
    for l in todas {
        if v.iter().any(|(_, x)| x.leccion.id == l.leccion.id) {
            continue;
        }
        let p = puntuar(
            &q,
            &normalizar(&l.leccion.titulo),
            &normalizar(&texto_entero(&l.leccion)),
        );
        if p > 0 {
            v.push((p, l));
        }
    }
    v
}

/// Lo que es, como en las tarjetas del movil.
fn tipo(l: &Leccion) -> &'static str {
    match l.tipo.as_str() {
        TIPO_ERROR => "Error",
        TIPO_ACIERTO => "Acierto",
        _ => "Lección",
    }
}

/// **Una leccion como resultado**: «Lección · área · #etiquetas · hace N
/// días»; el texto entero al pasar el raton; su primera foto en la vista
/// previa (F1); Intro abre su ficha.
pub fn resultado(l: &LeccionEn, proyectos: &[Proyecto], ctx: &Contexto) -> Resultado {
    let p = &proyectos[l.proyecto];
    let x = &l.leccion;
    let etiquetas = x
        .todas_las_etiquetas()
        .iter()
        .take(3)
        .map(|e| format!("#{e}"))
        .collect::<Vec<_>>()
        .join(" ");
    let veces = if x.repeticiones.is_empty() {
        String::new()
    } else {
        format!("🔁 {}", x.veces_que_paso())
    };
    let donde = if p.guardados {
        String::new()
    } else {
        p.nombre.clone()
    };
    let cuando = hace(x.creada, ctx.ahora);
    let mut sub: Vec<&str> = vec![tipo(x), x.area.trim(), &etiquetas, &veces, &donde, &cuando];
    sub.retain(|t| !t.is_empty());
    let mut subtitulo = sub.join(" · ");
    if !l.fotos.is_empty() {
        let n = l.fotos.len();
        subtitulo = format!(
            "📎 {n} {} · {subtitulo}",
            if n == 1 { "foto" } else { "fotos" }
        );
    }
    let ruta = l.ruta.to_string_lossy().to_string();
    let abrir = Accion::Pedido(pedido(
        "abrir",
        json!({ "que": { "tipo": "fichero", "ruta": ruta } }),
    ));
    let titulo = x.titulo.trim();
    let mut r = Resultado::nuevo(
        if titulo.is_empty() {
            "Lección"
        } else {
            titulo
        },
        subtitulo,
        glifo::LECCION,
        abrir,
    );
    let resumen = x.resumen();
    r.copiar = Some(resumen.clone());
    r.ayuda_titulo = Some(para_ayuda(titulo));
    r.ayuda_subtitulo = Some(para_ayuda(&resumen));
    r.clave = Some(format!("leccion/{}", x.id));
    r.autocompletar = Some(ctx.consulta(&format!("lecciones {titulo}")));
    if let Some(f) = l.fotos.first() {
        let f = f.to_string_lossy().to_string();
        r.vista_previa = Some(f.clone());
        r.fichero = Some(f);
    }
    let mut menu = json!({
        "tipo": "leccion",
        "ruta": ruta,
        "texto": resumen,
        "titulo": titulo,
        "proyecto": p.id_para_pedido(),
    });
    if let Some(f) = l.fotos.first() {
        menu["pin"] = pedido("pinear", json!({ "ruta": f.to_string_lossy() }));
    }
    r.con_menu(menu);
    r
}

/// «Abrir la ventana de Lecciones».
pub fn resultado_ventana_lecciones() -> Resultado {
    resultado_ventana(
        "lecciones",
        "Abrir la ventana de Lecciones",
        "Todas, con buscador, repaso y lista de comprobación",
        glifo::VENTANA,
    )
}

/// «Repasar hoy (N)»: entra en `p repasar`.
fn resultado_repasar(todas: &[LeccionEn], ctx: &Contexto) -> Resultado {
    let lecciones: Vec<Leccion> = todas.iter().map(|l| l.leccion.clone()).collect();
    let tocan = lecciones
        .iter()
        .filter(|l| Repaso::toca(l, ctx.ahora))
        .count();
    let (titulo, sub) = match tocan {
        0 => (
            "Repasar hoy".to_string(),
            "Hoy no toca repasar ninguna".to_string(),
        ),
        n => (
            format!("Repasar hoy ({})", n.min(DE_HOY)),
            "Las que tocan repasar hoy, la más grave primero".to_string(),
        ),
    };
    let entrar = ctx.consulta("repasar ");
    let mut r = Resultado::nuevo(
        titulo,
        sub,
        glifo::LECCION,
        Accion::Consulta(entrar.clone()),
    );
    r.autocompletar = Some(entrar);
    r.clave = Some("funcion/repasar".into());
    r
}

/// **`p lecciones [texto]`**: sin texto, repasar, la ventana y todas de la
/// mas tocada a la menos; con texto, las que encuentra el buscador y, al
/// final, buscarlo en la ventana o apuntarlo como leccion nueva.
pub fn lista(
    proyectos: &[Proyecto],
    texto: &str,
    en: Option<&Proyecto>,
    ctx: &Contexto,
) -> Vec<Resultado> {
    let mut todas = todas(proyectos);
    if let Some(p) = en {
        todas.retain(|l| proyectos[l.proyecto].id == p.id);
    }
    let texto = texto.trim();
    let mut v = Vec::new();
    if texto.is_empty() {
        v.push(resultado_repasar(&todas, ctx));
        v.extend(todas.iter().map(|l| resultado(l, proyectos, ctx)));
        if todas.is_empty() {
            let mut r = Resultado::nuevo(
                "Aún no hay lecciones",
                "Escribe «a <lo que aprendiste>» para apuntar la primera",
                glifo::LECCION,
                Accion::Consulta(ctx.consulta("a ")),
            );
            r.autocompletar = Some(ctx.consulta("a "));
            v.push(r);
        }
        v.push(resultado_ventana_lecciones());
        return v;
    }
    let mut halladas = buscar(&todas, texto);
    // Estable: lo que el buscador puso antes, a igualdad, sigue antes.
    halladas.sort_by_key(|x| std::cmp::Reverse(x.0));
    v.extend(halladas.iter().map(|(_, l)| resultado(l, proyectos, ctx)));
    if halladas.is_empty() {
        v.push(Resultado::nuevo(
            format!("Ninguna lección habla de «{texto}»"),
            "Prueba con otra palabra · abajo, apuntarla como nueva",
            glifo::AVISO,
            Accion::Consulta(ctx.consulta("lecciones ")),
        ));
    }
    let proyecto = en.map(Proyecto::id_para_pedido).unwrap_or(Value::Null);
    v.push(Resultado::nuevo(
        format!("Buscar «{texto}» en la ventana de Lecciones"),
        "Intro: la lista de la app con esta búsqueda",
        glifo::VENTANA,
        Accion::Pedido(pedido(
            "lecciones",
            json!({ "consulta": texto, "proyecto": proyecto }),
        )),
    ));
    let mut nueva = Resultado::nuevo(
        format!("Nueva lección: {texto}"),
        "Intro: abre la ficha con esto",
        glifo::ANADIR,
        Accion::Pedido(pedido(
            "leccion_nueva",
            json!({ "texto": texto, "proyecto": proyecto }),
        )),
    );
    crate::imagenes::con_imagenes(&mut nueva, texto, ctx);
    v.push(nueva);
    v
}

/// **`p repasar`**: las que tocan hoy con la regla de la app
/// (`Repaso::de_hoy`), Intro abre su ficha. Si tocan mas de las del dia, se
/// dice; si no toca ninguna, cuando toca la siguiente.
pub fn repasar(proyectos: &[Proyecto], ctx: &Contexto) -> Vec<Resultado> {
    let todas = todas(proyectos);
    let lecciones: Vec<Leccion> = todas.iter().map(|l| l.leccion.clone()).collect();
    let hoy = Repaso::de_hoy(&lecciones, ctx.ahora, DE_HOY);
    let tocan = lecciones
        .iter()
        .filter(|l| Repaso::toca(l, ctx.ahora))
        .count();
    let mut v: Vec<Resultado> = hoy
        .iter()
        .filter_map(|h| todas.iter().find(|l| l.leccion.id == h.id))
        .map(|l| {
            let mut r = resultado(l, proyectos, ctx);
            let pista = if l.leccion.proxima.trim().is_empty() {
                String::new()
            } else {
                format!("La próxima vez: {}", l.leccion.proxima.trim())
            };
            if !pista.is_empty() {
                r.subtitulo = format!("{pista} · {}", r.subtitulo);
            }
            r
        })
        .collect();
    if v.is_empty() {
        let siguiente = lecciones
            .iter()
            .map(|l| l.repasar)
            .filter(|r| *r > ctx.ahora)
            .min();
        let sub = match siguiente {
            Some(s) => format!("La siguiente toca {}", cuando_toca(s, ctx.ahora)),
            None if lecciones.is_empty() => {
                "Aún no hay lecciones · «a <lo que aprendiste>» apunta una".to_string()
            }
            None => String::new(),
        };
        v.push(Resultado::nuevo(
            "Hoy no toca repasar ninguna",
            sub,
            glifo::LECCION,
            Accion::Consulta(ctx.consulta("lecciones ")),
        ));
    } else if tocan > hoy.len() {
        v.push(Resultado::nuevo(
            format!("Y {} más esperan", tocan - hoy.len()),
            "Cinco al día, como en la app: las demás, mañana o en la ventana de Lecciones",
            glifo::LECCION,
            Accion::Pedido(crate::resultados::pedido_ventana("lecciones")),
        ));
    }
    v.push(resultado_ventana_lecciones());
    v
}

/// «mañana», «en 3 días».
fn cuando_toca(ms: i64, ahora: i64) -> String {
    let dias = ((ms - ahora) + pixpin_lecciones::leccion::DIA - 1) / pixpin_lecciones::leccion::DIA;
    match dias {
        i64::MIN..=1 => "mañana".into(),
        n => format!("en {n} días"),
    }
}

/// Las lecciones para la busqueda en todo: cada una con sus puntos, ya por
/// debajo de lo que coincide de verdad (un proyecto, una lista o una tarea
/// que se llama asi salen antes). Como mucho [`EN_TODO`].
pub fn para_buscar_en_todo(
    proyectos: &[Proyecto],
    texto: &str,
    en: Option<&str>,
    ctx: &Contexto,
) -> Vec<(u32, i64, Resultado)> {
    let mut todas = todas(proyectos);
    if let Some(id) = en {
        todas.retain(|l| proyectos[l.proyecto].id == id);
    }
    let mut v: Vec<(u32, i64, Resultado)> = buscar(&todas, texto)
        .into_iter()
        .map(|(p, l)| {
            (
                p.min(crate::normalizar::PALABRA),
                l.leccion.tocada,
                resultado(l, proyectos, ctx),
            )
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    v.truncate(EN_TODO);
    v
}
