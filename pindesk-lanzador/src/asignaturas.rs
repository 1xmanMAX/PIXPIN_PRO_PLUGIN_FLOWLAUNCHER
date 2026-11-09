//! **Las asignaturas del horario en Flow** (1-nov-2026).
//!
//! El usuario: «que esas asignaturas tambien se puedan buscar desde Flow
//! Launcher, facilmente». Escribir el nombre de una asignatura (`p geom`)
//! la ensena con su proxima clase y sus tareas pendientes; Intro abre su
//! lista en la ventana de Tareas (pedido `abrir_lista`). Y lo que se escriba
//! detras de su nombre es una tarea para ella: `p geometría resolver la hoja
//! 3` → «Añadir tarea a Geometría» (pedido `anadir_tarea` con `asignatura`).
//!
//! **Su chat** (6-nov-2026): detras de la mejor, «Geometría en el chat
//! «Asignaturas»» abre el chat de todas las asignaturas con ella elegida en
//! su fila (pedido `abrir_asignatura`): sus clases grabadas, su lista y lo
//! que se le mando, juntos.
//!
//! El plugin no lee `horario.md` (su formato es de la app): la app deja cada
//! asignatura con sus clases y su lista en `<raiz>/cache/asignaturas.json`
//! (`horario::asignaturas::para_flow`) cada vez que el horario cambia.

use std::path::Path;

use serde::Deserialize;
use serde_json::json;

use crate::datos::Proyecto;
use crate::normalizar::{normalizar, puntuar};
use crate::resultados::{
    Accion, Contexto, Lista, Resultado, cuenta_de, glifo, para_ayuda, pedido, tarea_entendida, unir,
};

/// Una clase de la semana: el dia (0 lunes … 6 domingo) y sus minutos.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Clase {
    pub dia: u8,
    pub inicio: u32,
    pub fin: u32,
    #[serde(default)]
    pub aula: Option<String>,
}

/// Una asignatura, como la deja la app.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asignatura {
    pub nombre: String,
    #[serde(default)]
    pub aula: Option<String>,
    /// El chat y el mensaje de su lista de tareas (`None` si aun no tiene).
    #[serde(default)]
    pub proyecto: Option<String>,
    #[serde(default)]
    pub codigo: Option<String>,
    #[serde(default)]
    pub clases: Vec<Clase>,
}

#[derive(Deserialize)]
struct Fichero {
    #[serde(default)]
    asignaturas: Vec<Asignatura>,
}

/// Las asignaturas de un `asignaturas.json` (vacio si no se entiende).
pub fn de_json(texto: &str) -> Vec<Asignatura> {
    serde_json::from_str::<Fichero>(texto)
        .map(|f| f.asignaturas)
        .unwrap_or_default()
}

/// Las de la app, si las dejo (`<raiz>/cache/asignaturas.json`).
pub fn leer(raiz: &Path) -> Vec<Asignatura> {
    std::fs::read_to_string(raiz.join("cache").join("asignaturas.json"))
        .map(|t| de_json(&t))
        .unwrap_or_default()
}

/// La proxima clase, vista desde ahora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Proxima {
    /// Esta en clase: hasta el minuto `fin` de hoy.
    Ahora { fin: u32, aula: Option<String> },
    /// Dentro de `dias` dias (0 hoy, 1 mañana…), el dia de la semana `dia`
    /// al minuto `inicio`.
    Luego {
        dias: u32,
        dia: u8,
        inicio: u32,
        aula: Option<String>,
    },
}

/// **La proxima clase** de `clases`: `hoy` es el dia de la semana (0 lunes)
/// y `minuto` el de ahora. Una clase en curso es «ahora».
pub fn proxima(clases: &[Clase], hoy: u8, minuto: u32) -> Option<Proxima> {
    if let Some(c) = clases
        .iter()
        .find(|c| c.dia == hoy && c.inicio <= minuto && minuto < c.fin)
    {
        return Some(Proxima::Ahora {
            fin: c.fin,
            aula: c.aula.clone(),
        });
    }
    clases
        .iter()
        .filter(|c| c.dia < 7)
        .map(|c| {
            let mut dias = (u32::from(c.dia) + 7 - u32::from(hoy)) % 7;
            if dias == 0 && c.inicio <= minuto {
                dias = 7;
            }
            (dias * 1440 + c.inicio, dias, c)
        })
        .min_by_key(|(clave, _, _)| *clave)
        .map(|(_, dias, c)| Proxima::Luego {
            dias,
            dia: c.dia,
            inicio: c.inicio,
            aula: c.aula.clone(),
        })
}

const DIAS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];

fn hora(m: u32) -> String {
    format!("{}:{:02}", m / 60, m % 60)
}

/// «En clase hasta 10:00 · A-12», «Próxima clase: hoy 10:15», «mañana
/// 8:00», «jue 8:00».
pub fn legible(p: &Proxima) -> String {
    let (t, aula) = match p {
        Proxima::Ahora { fin, aula } => (format!("En clase hasta {}", hora(*fin)), aula),
        Proxima::Luego {
            dias,
            dia,
            inicio,
            aula,
        } => {
            let cuando = match dias {
                0 => "hoy".to_string(),
                1 => "mañana".to_string(),
                _ => DIAS[usize::from(*dia).min(6)].to_string(),
            };
            (format!("Próxima clase: {cuando} {}", hora(*inicio)), aula)
        }
    };
    match aula.as_deref().filter(|a| !a.trim().is_empty()) {
        Some(a) => format!("{t} · {a}"),
        None => t,
    }
}

/// **Si lo escrito empieza por el nombre entero de una asignatura**: ella y
/// lo de detras (la tarea). Gana la de nombre mas largo («Física II» antes
/// que «Física»). Sin nada detras, `None`: eso es buscarla.
pub fn al_principio<'a>(
    todas: &'a [Asignatura],
    escrito: &str,
) -> Option<(&'a Asignatura, String)> {
    let palabras: Vec<&str> = escrito.split_whitespace().collect();
    todas
        .iter()
        .filter_map(|a| {
            let suyas: Vec<String> = a.nombre.split_whitespace().map(normalizar).collect();
            let n = suyas.len();
            (n > 0
                && palabras.len() > n
                && palabras[..n]
                    .iter()
                    .zip(&suyas)
                    .all(|(p, s)| normalizar(p) == *s))
            .then(|| (n, a, palabras[n..].join(" ")))
        })
        .max_by_key(|(n, _, _)| *n)
        .map(|(_, a, resto)| (a, resto))
}

/// La lista de la asignatura entre las de los chats.
fn su_lista<'a>(a: &Asignatura, listas: &'a [Lista], proyectos: &[Proyecto]) -> Option<&'a Lista> {
    let codigo = a.codigo.as_deref()?;
    listas.iter().find(|l| {
        l.codigo == codigo
            && a.proyecto
                .as_deref()
                .is_none_or(|p| proyectos.get(l.proyecto).is_some_and(|x| x.id == p))
    })
}

/// El dia de la semana (0 lunes) y el minuto de ahora.
fn ahora(ctx: &Contexto) -> (u8, u32) {
    let m = ctx.momento_local();
    // El 1-1-1970 fue jueves.
    ((m.dia + 3).rem_euclid(7) as u8, m.minuto)
}

/// El resultado de una asignatura: Intro abre su lista en Tareas; Tab deja
/// su nombre escrito para apuntarle una tarea.
pub fn resultado(
    a: &Asignatura,
    listas: &[Lista],
    proyectos: &[Proyecto],
    ctx: &Contexto,
) -> Resultado {
    let (hoy, minuto) = ahora(ctx);
    let cuando = proxima(&a.clases, hoy, minuto)
        .map(|p| legible(&p))
        .unwrap_or_default();
    let l = su_lista(a, listas, proyectos);
    let cuenta = l.map(cuenta_de).unwrap_or_else(|| "sin tareas".to_string());
    let mut r = Resultado::nuevo(
        &a.nombre,
        unir(&["Asignatura", &cuando, &cuenta, "Intro: abrir sus tareas"]),
        glifo::ASIGNATURA,
        Accion::Pedido(pedido("abrir_lista", json!({ "asignatura": a.nombre }))),
    );
    r.autocompletar = Some(ctx.consulta(&format!("{} ", a.nombre)));
    r.clave = Some(format!("asignatura/{}", normalizar(&a.nombre)));
    if let Some(l) = l {
        r.progreso = l.progreso();
        let pendientes: String = l
            .tareas
            .iter()
            .filter(|t| !t.hecha)
            .map(|t| format!("- [ ] {}\n", t.texto))
            .collect();
        if !pendientes.is_empty() {
            r.ayuda_subtitulo = Some(para_ayuda(&pendientes));
        }
    }
    r
}

/// «Geometría en el chat «Asignaturas»»: el chat de las asignaturas con ella
/// elegida arriba (pedido `abrir_asignatura`).
pub fn resultado_en_el_chat(a: &Asignatura, ctx: &Contexto) -> Resultado {
    let mut r = Resultado::nuevo(
        format!("{} en el chat «Asignaturas»", a.nombre),
        unir(&[
            "Sus clases grabadas, su lista y lo que le mandaste",
            "Intro: abrir el chat",
        ]),
        glifo::CHAT,
        Accion::Pedido(pedido(
            "abrir_asignatura",
            json!({ "asignatura": a.nombre }),
        )),
    );
    r.autocompletar = Some(ctx.consulta(&format!("{} ", a.nombre)));
    r.clave = Some(format!("asignatura-chat/{}", normalizar(&a.nombre)));
    r
}

/// «Añadir tarea a Geometría: …»: la tarea va a su lista (la app la busca
/// por el nombre, y la crea si aun no esta). Flow se queda en la asignatura
/// para apuntar otra.
pub fn resultado_anadir(
    a: &Asignatura,
    texto: &str,
    listas: &[Lista],
    proyectos: &[Proyecto],
    ctx: &Contexto,
) -> Resultado {
    let (titulo, entendido) = tarea_entendida(texto, ctx);
    let cuenta = su_lista(a, listas, proyectos)
        .map(cuenta_de)
        .unwrap_or_default();
    let mut r = Resultado::nuevo(
        format!("Añadir tarea a {}: {titulo}", a.nombre),
        unir(&[
            &entendido,
            &format!("Intro: añadirla a «{}»", a.nombre),
            &cuenta,
        ]),
        glifo::ANADIR,
        Accion::PedirYSeguir {
            pedido: pedido(
                "anadir_tarea",
                json!({ "texto": texto, "asignatura": a.nombre }),
            ),
            consulta: ctx.consulta(&format!("{} ", a.nombre)),
        },
    );
    r.ayuda_titulo = Some(texto.to_string());
    crate::imagenes::con_imagenes(&mut r, texto, ctx);
    r
}

/// **Lo de las asignaturas para lo tecleado** (lo que va delante de buscar
/// en todo): con el nombre entero y algo detras, añadir esa tarea (y la
/// asignatura); si no, las asignaturas cuyo nombre coincide, la mejor
/// primero.
pub fn para_lo_escrito(
    todas: &[Asignatura],
    escrito: &str,
    proyectos: &[Proyecto],
    ctx: &Contexto,
) -> Vec<Resultado> {
    if todas.is_empty() || escrito.trim().is_empty() {
        return Vec::new();
    }
    let listas = crate::resultados::listas(proyectos);
    if let Some((a, resto)) = al_principio(todas, escrito) {
        return vec![
            resultado_anadir(a, &resto, &listas, proyectos, ctx),
            resultado(a, &listas, proyectos, ctx),
        ];
    }
    let q = normalizar(escrito.trim());
    let mut v: Vec<(u32, &Asignatura)> = todas
        .iter()
        .map(|a| (puntuar(&q, &normalizar(&a.nombre), ""), a))
        .filter(|(p, _)| *p > 0)
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.0));
    let mut r: Vec<Resultado> = v
        .iter()
        .map(|(_, a)| resultado(a, &listas, proyectos, ctx))
        .collect();
    // Y la mejor, tambien en su chat: justo detras de ella.
    if let Some((_, a)) = v.first() {
        r.insert(1, resultado_en_el_chat(a, ctx));
    }
    r
}

/// Las claves (`lista/<chat>/<codigo>`) de las listas de las asignaturas
/// que salen en `ya` (la asignatura ya ensena su lista, y no sale dos
/// veces; la de una asignatura que no salio, si).
pub fn claves_de_sus_listas(todas: &[Asignatura], ya: &[Resultado]) -> Vec<String> {
    todas
        .iter()
        .filter(|a| {
            let k = format!("asignatura/{}", normalizar(&a.nombre));
            ya.iter().any(|r| r.clave.as_deref() == Some(k.as_str()))
        })
        .filter_map(|a| {
            Some(format!(
                "lista/{}/{}",
                a.proyecto.as_deref()?,
                a.codigo.as_deref()?
            ))
        })
        .collect()
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn clase(dia: u8, inicio: u32, fin: u32) -> Clase {
        Clase {
            dia,
            inicio,
            fin,
            aula: None,
        }
    }

    fn asig(nombre: &str) -> Asignatura {
        Asignatura {
            nombre: nombre.into(),
            aula: None,
            proyecto: Some("asig".into()),
            codigo: Some(format!("m-{}", normalizar(nombre))),
            clases: Vec::new(),
        }
    }

    #[test]
    fn la_proxima_clase_mira_la_semana_entera() {
        // Lunes y miercoles de 8:00 a 10:00.
        let c = vec![clase(0, 480, 600), clase(2, 480, 600)];
        // Lunes 9:00: en clase.
        assert_eq!(
            proxima(&c, 0, 540),
            Some(Proxima::Ahora {
                fin: 600,
                aula: None
            })
        );
        // Lunes 10:00 (acabo): la del miercoles.
        assert_eq!(
            legible(&proxima(&c, 0, 600).unwrap()),
            "Próxima clase: mié 8:00"
        );
        // Martes 23:00: mañana.
        assert_eq!(
            legible(&proxima(&c, 1, 23 * 60).unwrap()),
            "Próxima clase: mañana 8:00"
        );
        // Lunes 7:00: hoy.
        assert_eq!(
            legible(&proxima(&c, 0, 420).unwrap()),
            "Próxima clase: hoy 8:00"
        );
        // Miercoles 11:00: el lunes que viene.
        assert_eq!(
            proxima(&c, 2, 660),
            Some(Proxima::Luego {
                dias: 5,
                dia: 0,
                inicio: 480,
                aula: None
            })
        );
        // Una sola clase, ya pasada hoy: dentro de una semana.
        let una = vec![Clase {
            aula: Some("A-12".into()),
            ..clase(4, 600, 660)
        }];
        assert_eq!(
            legible(&proxima(&una, 4, 700).unwrap()),
            "Próxima clase: vie 10:00 · A-12"
        );
        // Caso negativo: sin clases no hay proxima.
        assert_eq!(proxima(&[], 0, 0), None);
    }

    #[test]
    fn el_nombre_entero_delante_es_una_tarea_para_ella() {
        let v = vec![asig("Física"), asig("Física II"), asig("Geometría")];
        let (a, resto) = al_principio(&v, "fisica ii repasar ondas").unwrap();
        assert_eq!(a.nombre, "Física II", "la de nombre mas largo");
        assert_eq!(resto, "repasar ondas");
        let (a, resto) = al_principio(&v, "Geometria  hoja 3 mañana").unwrap();
        assert_eq!(
            (a.nombre.as_str(), resto.as_str()),
            ("Geometría", "hoja 3 mañana")
        );
        // Caso negativo: solo el nombre es buscarla, y medio nombre no vale.
        assert!(al_principio(&v, "geometría").is_none());
        assert!(al_principio(&v, "geo hoja 3").is_none());
        assert!(al_principio(&v, "historia hoja").is_none());
    }

    #[test]
    fn se_lee_lo_que_deja_la_app() {
        let v = de_json(
            r##"{"version":1,"asignaturas":[{"nombre":"Geometría","aula":"A-12","color":"#0A84FF",
            "proyecto":"p1","codigo":"m1","clases":[{"dia":0,"inicio":480,"fin":540,"aula":"A-12"}]},
            {"nombre":"Arte","proyecto":null,"codigo":null,"clases":[]}]}"##,
        );
        assert_eq!(v.len(), 2);
        assert_eq!(
            v[0].clases,
            vec![Clase {
                aula: Some("A-12".into()),
                ..clase(0, 480, 540)
            }]
        );
        assert_eq!(v[1].codigo, None);
        let mut ya = Resultado::nuevo(
            "Geometría",
            "",
            glifo::ASIGNATURA,
            Accion::Copiar(String::new()),
        );
        ya.clave = Some("asignatura/geometria".into());
        assert_eq!(
            claves_de_sus_listas(&v, std::slice::from_ref(&ya)),
            vec!["lista/p1/m1".to_string()]
        );
        // Si la asignatura no salio, su lista si.
        assert!(claves_de_sus_listas(&v, &[]).is_empty());
        // Caso negativo: un fichero roto no rompe nada.
        assert!(de_json("{no es json").is_empty());
    }
}
