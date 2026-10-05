//! Lo que se teclea detras de `pp`, entendido.
//!
//! - nada: las funciones y los ultimos proyectos;
//! - `<verbo> [texto] [@proyecto]`: una funcion con lo que haga falta
//!   (`chat hola`, `nota idea`, `lienzo plano`, `grabar clase 3`, `tareas`);
//! - `tareas <lista> > [texto]`: las tareas de esa lista;
//! - una letra sola, o una letra y un blanco: el atajo de una funcion
//!   (`t <texto>` apunta una tarea en el Inbox, `n` nota, `l` lienzo,
//!   `g` galeria, `c` capturar, `u` ultima captura, `a` «aprendi»: una
//!   leccion nueva). Una letra pegada a
//!   otras («tx», «nota») es lo de siempre;
//! - `<proyecto> > [texto]`: el chat de ese proyecto, de lo ultimo a lo
//!   primero (con un verbo delante tambien, si lo de antes de `>` es el
//!   nombre entero de un proyecto: eso lo mira `resultados`, que conoce los
//!   nombres);
//! - cualquier otra cosa: buscar en todo (con `@proyecto` para quedarse en uno).

use crate::normalizar::normalizar;

/// Las funciones de PixPin que se llaman por su nombre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Funcion {
    Chat,
    Tareas,
    Lienzo,
    Nota,
    Grabar,
    /// El recuadro flotante donde se sueltan archivos (pedido `soltar`).
    Soltar,
    /// Apuntar una leccion aprendida (pedido `leccion_nueva`).
    Leccion,
    /// Buscar las lecciones aprendidas, leidas del disco.
    Lecciones,
    /// Las lecciones que tocan repasar hoy (el repaso espaciado de la app).
    Repasar,
    /// Recortar una zona de la pantalla (pedido `capturar`).
    Capturar,
    /// Las ultimas capturas de `<raiz>/capturas`, para buscar y pinear.
    Capturas,
    /// La galeria de capturas (pedido `ventana {cual:"galeria"}`).
    Galeria,
    /// Sacar la ultima captura como pin (pedido `pinear_ultima`).
    Ultima,
    Abrir,
}

impl Funcion {
    pub const TODAS: [Funcion; 14] = [
        Funcion::Chat,
        Funcion::Tareas,
        Funcion::Lienzo,
        Funcion::Nota,
        Funcion::Grabar,
        Funcion::Soltar,
        Funcion::Leccion,
        Funcion::Lecciones,
        Funcion::Repasar,
        Funcion::Capturar,
        Funcion::Capturas,
        Funcion::Galeria,
        Funcion::Ultima,
        Funcion::Abrir,
    ];

    /// Las que se ofrecen en la lista (con `p` a secas y al buscar en todo):
    /// todas menos el chat. El usuario, 4-oct: escribir algo que no se
    /// encontraba dejaba «Chat» elegido, e Intro lo mandaba a Mensajes
    /// guardados. Escribir en un chat es entrar en el (`<proyecto> > texto`)
    /// o, a proposito, `chat <texto>`.
    pub const OFRECIDAS: [Funcion; 13] = [
        Funcion::Tareas,
        Funcion::Lienzo,
        Funcion::Nota,
        Funcion::Grabar,
        Funcion::Soltar,
        Funcion::Leccion,
        Funcion::Lecciones,
        Funcion::Repasar,
        Funcion::Capturar,
        Funcion::Capturas,
        Funcion::Galeria,
        Funcion::Ultima,
        Funcion::Abrir,
    ];

    /// Las palabras que la llaman, ya normalizadas; la primera es la que se
    /// escribe al completar.
    pub fn alias(self) -> &'static [&'static str] {
        match self {
            // Solo «chat»: «mensaje …» o «escribir …» son busquedas normales
            // (como verbo mandaban lo escrito a Mensajes guardados).
            Funcion::Chat => &["chat"],
            Funcion::Tareas => &[
                "tareas",
                "tarea",
                "todo",
                "todos",
                "tasks",
                "task",
                "pendientes",
            ],
            Funcion::Lienzo => &["lienzo", "canvas", "dibujo", "dibujar", "pizarra"],
            Funcion::Nota => &["nota", "note", "notas"],
            Funcion::Grabar => &["grabar", "audio", "voz", "record", "grabacion"],
            // «añadir» normalizado es «anadir».
            Funcion::Soltar => &["anadir", "soltar", "agregar", "subir", "adjuntar", "drop"],
            Funcion::Leccion => &["leccion", "aprendi", "aprendido", "lesson"],
            Funcion::Lecciones => &["lecciones", "lessons"],
            Funcion::Repasar => &["repasar", "repaso", "review"],
            Funcion::Capturar => &["captura", "capturar", "screenshot", "recortar"],
            Funcion::Capturas => &["capturas", "screenshots", "recortes"],
            Funcion::Galeria => &["galeria", "gallery"],
            Funcion::Ultima => &["ultima", "last"],
            Funcion::Abrir => &["pixpin", "abrir", "open"],
        }
    }

    /// La palabra que se escribe al completar (`pp tareas `).
    pub fn verbo(self) -> &'static str {
        match self {
            Funcion::Soltar => "añadir",
            Funcion::Leccion => "lección",
            Funcion::Lecciones => "lecciones",
            Funcion::Galeria => "galería",
            Funcion::Ultima => "última",
            _ => self.alias()[0],
        }
    }

    /// La funcion de una letra sola (`t`, `n`, `l`, `g`, `c`, `u`, `a`). La
    /// `a` es «aprendi»: apuntar una leccion (la `l` ya era del lienzo).
    pub fn atajo(letra: &str) -> Option<Funcion> {
        Some(match normalizar(letra).as_str() {
            "t" => Funcion::Tareas,
            "n" => Funcion::Nota,
            "l" => Funcion::Lienzo,
            "g" => Funcion::Galeria,
            "c" => Funcion::Capturar,
            "u" => Funcion::Ultima,
            "a" => Funcion::Leccion,
            _ => return None,
        })
    }

    /// La letra de su atajo, si tiene (para decirlo en el subtitulo).
    pub fn letra(self) -> Option<char> {
        Some(match self {
            Funcion::Tareas => 't',
            Funcion::Nota => 'n',
            Funcion::Lienzo => 'l',
            Funcion::Galeria => 'g',
            Funcion::Capturar => 'c',
            Funcion::Ultima => 'u',
            Funcion::Leccion => 'a',
            _ => return None,
        })
    }

    pub fn de_palabra(palabra: &str) -> Option<Funcion> {
        let p = normalizar(palabra);
        Funcion::TODAS
            .into_iter()
            .find(|f| f.alias().contains(&p.as_str()))
    }
}

/// Lo tecleado, entendido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modo {
    Vacio,
    /// Buscar en todo. `proyecto` es lo escrito detras de `@`.
    Buscar {
        texto: String,
        proyecto: Option<String>,
    },
    /// Una funcion y lo que va detras (sin el `@proyecto`).
    Verbo {
        funcion: Funcion,
        resto: String,
        proyecto: Option<String>,
    },
    /// Dentro de una lista de tareas: `tareas <lista> > <filtro>`.
    Lista {
        lista: String,
        filtro: String,
    },
    /// Dentro del chat de un proyecto: `<proyecto> > <filtro>`.
    Proyecto {
        proyecto: String,
        filtro: String,
    },
    /// `t <texto>`: apuntar una tarea en el Inbox.
    Apuntar {
        texto: String,
    },
}

/// El separador entre la lista y la tarea, y entre el proyecto y lo que se
/// busca en su chat.
pub const SEPARADOR: char = '>';

pub fn analizar(busqueda: &str) -> Modo {
    let t = busqueda.trim_start();
    if t.trim().is_empty() {
        return Modo::Vacio;
    }
    let (primera, resto) = match t.find(char::is_whitespace) {
        Some(i) => (&t[..i], &t[i..]),
        None => (t, ""),
    };
    // Una letra sola es un atajo (y solo si va sola o seguida de un blanco:
    // «tx» o «nota» siguen su camino).
    if primera.chars().count() == 1 {
        if let Some(funcion) = Funcion::atajo(primera) {
            let texto = resto.trim();
            if funcion == Funcion::Tareas {
                if let Some((lista, filtro)) = resto.split_once(SEPARADOR) {
                    return Modo::Lista {
                        lista: lista.trim().to_string(),
                        filtro: filtro.trim().to_string(),
                    };
                }
                if !texto.is_empty() {
                    return Modo::Apuntar {
                        texto: texto.to_string(),
                    };
                }
            }
            let (resto, proyecto) = separar_proyecto(resto);
            return Modo::Verbo {
                funcion,
                resto,
                proyecto,
            };
        }
    }
    if let Some(funcion) = Funcion::de_palabra(primera) {
        if funcion == Funcion::Tareas {
            if let Some((lista, filtro)) = resto.split_once(SEPARADOR) {
                return Modo::Lista {
                    lista: lista.trim().to_string(),
                    filtro: filtro.trim().to_string(),
                };
            }
        }
        let (resto, proyecto) = separar_proyecto(resto);
        return Modo::Verbo {
            funcion,
            resto,
            proyecto,
        };
    }
    if let Some((proyecto, filtro)) = t.split_once(SEPARADOR) {
        return Modo::Proyecto {
            proyecto: proyecto.trim().to_string(),
            filtro: filtro.trim().to_string(),
        };
    }
    let (texto, proyecto) = separar_proyecto(t);
    Modo::Buscar { texto, proyecto }
}

/// `texto @proyecto` → (`texto`, `Some("proyecto")`). Vale la ultima `@` que
/// este al principio o detras de un blanco (un correo no cuenta).
pub fn separar_proyecto(texto: &str) -> (String, Option<String>) {
    let mut corte = None;
    let mut anterior: Option<char> = None;
    for (i, c) in texto.char_indices() {
        if c == '@' && anterior.is_none_or(char::is_whitespace) {
            corte = Some(i);
        }
        anterior = Some(c);
    }
    match corte {
        Some(i) => (
            texto[..i].trim().to_string(),
            Some(texto[i + 1..].trim().to_string()),
        ),
        None => (texto.trim().to_string(), None),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn buscar(t: &str, p: Option<&str>) -> Modo {
        Modo::Buscar {
            texto: t.into(),
            proyecto: p.map(Into::into),
        }
    }
    fn verbo(f: Funcion, r: &str, p: Option<&str>) -> Modo {
        Modo::Verbo {
            funcion: f,
            resto: r.into(),
            proyecto: p.map(Into::into),
        }
    }

    #[test]
    fn vacio_y_buscar() {
        assert_eq!(analizar(""), Modo::Vacio);
        assert_eq!(analizar("   "), Modo::Vacio);
        assert_eq!(analizar("objetivos  "), buscar("objetivos", None));
        assert_eq!(analizar("tare"), buscar("tare", None));
        assert_eq!(analizar("plano @Thesis"), buscar("plano", Some("Thesis")));
    }

    #[test]
    fn los_verbos_en_espanol_e_ingles_y_con_tildes() {
        assert_eq!(
            analizar("chat hola que tal"),
            verbo(Funcion::Chat, "hola que tal", None)
        );
        assert_eq!(analizar("todo"), verbo(Funcion::Tareas, "", None));
        assert_eq!(
            analizar("Canvas plano"),
            verbo(Funcion::Lienzo, "plano", None)
        );
        assert_eq!(
            analizar("grabación clase 3"),
            verbo(Funcion::Grabar, "clase 3", None)
        );
        assert_eq!(analizar("note idea"), verbo(Funcion::Nota, "idea", None));
        assert_eq!(analizar("grabar "), verbo(Funcion::Grabar, "", None));
        assert_eq!(
            analizar("Añadir thesis"),
            verbo(Funcion::Soltar, "thesis", None)
        );
        assert_eq!(analizar("drop"), verbo(Funcion::Soltar, "", None));
        assert_eq!(
            analizar("adjuntar ges"),
            verbo(Funcion::Soltar, "ges", None)
        );
    }

    #[test]
    fn caso_negativo_mensaje_y_escribir_ya_no_son_el_chat() {
        assert_eq!(analizar("mensaje hola"), buscar("mensaje hola", None));
        assert_eq!(
            analizar("escribir informe"),
            buscar("escribir informe", None)
        );
        assert_eq!(analizar("message"), buscar("message", None));
        assert_eq!(analizar("chat hola"), verbo(Funcion::Chat, "hola", None));
    }

    #[test]
    fn la_arroba_elige_proyecto_pero_no_en_un_correo() {
        assert_eq!(
            analizar("chat hola @Gestión de pro"),
            verbo(Funcion::Chat, "hola", Some("Gestión de pro"))
        );
        assert_eq!(
            analizar("chat escribe a max@x.com"),
            verbo(Funcion::Chat, "escribe a max@x.com", None)
        );
        assert_eq!(analizar("lienzo @"), verbo(Funcion::Lienzo, "", Some("")));
        assert_eq!(
            analizar("nota a @uno @dos"),
            verbo(Funcion::Nota, "a @uno", Some("dos"))
        );
    }

    #[test]
    fn las_letras_solas_son_atajos() {
        assert_eq!(
            analizar("t comprar pan"),
            Modo::Apuntar {
                texto: "comprar pan".into()
            }
        );
        assert_eq!(
            analizar("T  llamar a Ana @casa "),
            Modo::Apuntar {
                texto: "llamar a Ana @casa".into()
            }
        );
        assert_eq!(analizar("t"), verbo(Funcion::Tareas, "", None));
        assert_eq!(analizar("t "), verbo(Funcion::Tareas, "", None));
        assert_eq!(analizar("n idea"), verbo(Funcion::Nota, "idea", None));
        assert_eq!(
            analizar("l plano @thesis"),
            verbo(Funcion::Lienzo, "plano", Some("thesis"))
        );
        assert_eq!(analizar("g"), verbo(Funcion::Galeria, "", None));
        assert_eq!(analizar("c"), verbo(Funcion::Capturar, "", None));
        assert_eq!(analizar("u"), verbo(Funcion::Ultima, "", None));
        assert_eq!(
            analizar("capturas capt"),
            verbo(Funcion::Capturas, "capt", None)
        );
        assert_eq!(analizar("galería"), verbo(Funcion::Galeria, "", None));
        assert_eq!(analizar("última"), verbo(Funcion::Ultima, "", None));
        // La `a` es «aprendi»: una leccion nueva.
        assert_eq!(
            analizar("a revisar la escala @thesis"),
            verbo(Funcion::Leccion, "revisar la escala", Some("thesis"))
        );
        assert_eq!(analizar("a"), verbo(Funcion::Leccion, "", None));
    }

    #[test]
    fn las_lecciones_una_nueva_buscarlas_y_repasar() {
        assert_eq!(
            analizar("lección no cargar de noche"),
            verbo(Funcion::Leccion, "no cargar de noche", None)
        );
        assert_eq!(
            analizar("aprendí algo"),
            verbo(Funcion::Leccion, "algo", None)
        );
        assert_eq!(
            analizar("lecciones encofrado"),
            verbo(Funcion::Lecciones, "encofrado", None)
        );
        assert_eq!(analizar("lessons"), verbo(Funcion::Lecciones, "", None));
        assert_eq!(analizar("repasar"), verbo(Funcion::Repasar, "", None));
        assert_eq!(analizar("repaso "), verbo(Funcion::Repasar, "", None));
        // Caso negativo: «a» pegada a otras letras es buscar («ana», «acta»).
        assert_eq!(analizar("acta"), buscar("acta", None));
        assert_eq!(analizar("ab c"), buscar("ab c", None));
    }

    #[test]
    fn caso_negativo_una_letra_pegada_a_otras_no_es_atajo() {
        // Una letra seguida de mas letras es una busqueda normal.
        assert_eq!(analizar("tx"), buscar("tx", None));
        assert_eq!(analizar("ui"), buscar("ui", None));
        assert_eq!(analizar("gest"), buscar("gest", None));
        // Una letra sin atajo, tampoco.
        assert_eq!(analizar("x algo"), buscar("x algo", None));
        // «t lista > filtro» sigue siendo entrar en una lista, no apuntar.
        assert_eq!(
            analizar("t Compra > pan"),
            Modo::Lista {
                lista: "Compra".into(),
                filtro: "pan".into()
            }
        );
    }

    #[test]
    fn dentro_de_una_lista_con_el_separador() {
        assert_eq!(
            analizar("tareas Compra > "),
            Modo::Lista {
                lista: "Compra".into(),
                filtro: "".into()
            }
        );
        assert_eq!(
            analizar("tasks Compra · Casa > leche @casa"),
            Modo::Lista {
                lista: "Compra · Casa".into(),
                filtro: "leche @casa".into()
            }
        );
        // El separador solo vale detras de «tareas».
        assert_eq!(analizar("chat a > b"), verbo(Funcion::Chat, "a > b", None));
        // Sin verbo delante es el chat de un proyecto.
        assert_eq!(
            analizar("Mensajes guardados > foto"),
            Modo::Proyecto {
                proyecto: "Mensajes guardados".into(),
                filtro: "foto".into()
            }
        );
        assert_eq!(
            analizar("Thesis >"),
            Modo::Proyecto {
                proyecto: "Thesis".into(),
                filtro: "".into()
            }
        );
    }
}
