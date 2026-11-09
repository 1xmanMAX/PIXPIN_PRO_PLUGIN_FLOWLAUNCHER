//! **Una leccion aprendida** (`Leccion.kt`) y su repaso espaciado
//! (`Repaso`).
//!
//! Los campos salen del estudio que hizo el movil antes de escribirla
//! (`docs/lecciones.md` de Android): las preguntas de la revision despues de
//! la accion (que paso, por que, que hare distinto), la «prueba de que no se
//! repite» de la NASA —aqui, [`Leccion::repeticiones`]— y que estas bases
//! fracasan porque **cuesta rellenarlas y no salen cuando hacen falta**. Por
//! eso solo [`Leccion::titulo`] es obligatorio.
//!
//! **El JSON es el del movil, campo a campo** (camelCase, los mismos
//! nombres): el archivo `guardados/lecciones/<id>.leccion` viaja con la
//! sincronizacion y lo leen los dos. Lo que no se conoce se guarda en
//! [`Leccion::resto`] y se vuelve a escribir tal cual: una version nueva del
//! movil puede anadir campos, y reescribir aqui no puede perderlos.
//!
//! Los textos que eligen entre opciones (`tipo`, `area`) son cadenas y no
//! enums, como en Kotlin: un valor que no conoce una version vieja haria
//! fallar la lectura entera.

use serde_json::{Map, Value};

pub const TIPO_LECCION: &str = "leccion";
pub const TIPO_ERROR: &str = "error";
pub const TIPO_ACIERTO: &str = "acierto";

/// Un dia en milisegundos.
pub const DIA: i64 = 24 * 60 * 60 * 1000;

/// Dias hasta el siguiente repaso, por caja: lo que dicen los estudios del
/// repaso espaciado.
pub const INTERVALOS: [i64; 7] = [1, 3, 7, 14, 30, 90, 180];

/// Las areas con las que se empieza (decision del usuario, 2-oct-2026).
pub const AREAS: [&str; 4] = ["Trabajo", "Construcción", "Estudio", "Vida diaria"];

/// Las causas de un toque: las de siempre en los cuadernos de errores y en
/// los informes de incidentes.
pub const CAUSAS: [&str; 9] = [
    "Prisa",
    "No revisé",
    "Comunicación",
    "No sabía",
    "Supuse algo",
    "Herramienta",
    "Planificación",
    "Cansancio",
    "Distracción",
];

/// La extension del archivo de una leccion.
pub const EXTENSION: &str = ".leccion";
/// El prefijo del id del mensaje que la lleva (`lec-<id>`).
pub const PREFIJO: &str = "lec-";

#[derive(Debug, Clone, PartialEq)]
pub struct Leccion {
    pub id: String,
    pub creada: i64,
    pub tocada: i64,
    /// **Lo que aprendi**, en una frase. Lo unico obligatorio.
    pub titulo: String,
    /// Que paso: el suceso que la origino.
    pub que_paso: String,
    /// Por que paso: la causa, como pista y no como verdad unica.
    pub por_que: String,
    /// Que hare la proxima vez («si…, entonces…»). Es lo que va a la lista
    /// de comprobacion.
    pub proxima: String,
    /// [`TIPO_LECCION`], [`TIPO_ERROR`] o [`TIPO_ACIERTO`].
    pub tipo: String,
    /// El area grande. Vacia = sin area.
    pub area: String,
    /// 1 leve, 2 importante, 3 grave.
    pub gravedad: i64,
    /// Las que puso uno, o que acepto de las propuestas.
    pub etiquetas: Vec<String>,
    /// Las que puso el etiquetador y siguen ahi. Se ensenan con ✨.
    pub etiquetas_auto: Vec<String>,
    /// Las automaticas que uno quito: no vuelven a salir.
    pub quitadas: Vec<String>,
    /// Palabras de referencia ocultas: la encuentran aunque no salgan en su
    /// texto.
    pub referencias: Vec<String>,
    pub causas: Vec<String>,
    /// **Cada vez que volvio a pasar.** La medida de si se aprendio.
    pub repeticiones: Vec<i64>,
    /// El mensaje del chat del que salio, si salio de uno.
    pub de_mensaje: Option<String>,
    /// El repaso: en que caja va (0..=6) y cuando toca.
    pub caja: i64,
    pub repasar: i64,
    /// Si su `proxima` sale en la lista de comprobacion.
    pub en_lista: bool,
    /// Fotos y audios (4-oct en el movil): ids de los mensajes que los
    /// llevan. El PC aun no los anade, pero los conserva.
    pub adjuntos: Vec<String>,
    /// Lo que escribio una version que este PC no conoce.
    pub resto: Map<String, Value>,
}

/// Los nombres del JSON, en el orden de `Leccion.kt`.
const CAMPOS: [&str; 21] = [
    "id",
    "creada",
    "tocada",
    "titulo",
    "quePaso",
    "porQue",
    "proxima",
    "tipo",
    "area",
    "gravedad",
    "etiquetas",
    "etiquetasAuto",
    "quitadas",
    "referencias",
    "causas",
    "repeticiones",
    "deMensaje",
    "caja",
    "repasar",
    "enLista",
    "adjuntos",
];

impl Leccion {
    /// Una leccion nueva con lo minimo, como el constructor de Kotlin con
    /// sus valores por defecto.
    pub fn nueva(id: &str, creada: i64, titulo: &str) -> Leccion {
        Leccion {
            id: id.to_string(),
            creada,
            tocada: creada,
            titulo: titulo.to_string(),
            que_paso: String::new(),
            por_que: String::new(),
            proxima: String::new(),
            tipo: TIPO_LECCION.to_string(),
            area: String::new(),
            gravedad: 1,
            etiquetas: Vec::new(),
            etiquetas_auto: Vec::new(),
            quitadas: Vec::new(),
            referencias: Vec::new(),
            causas: Vec::new(),
            repeticiones: Vec::new(),
            de_mensaje: None,
            caja: 0,
            repasar: creada + DIA,
            en_lista: true,
            adjuntos: Vec::new(),
            resto: Map::new(),
        }
    }

    /// `todasLasEtiquetas`: las puestas y las automaticas, sin repetir.
    pub fn todas_las_etiquetas(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for e in self.etiquetas.iter().chain(&self.etiquetas_auto) {
            if !v.contains(e) {
                v.push(e.clone());
            }
        }
        v
    }

    pub fn veces_que_paso(&self) -> usize {
        1 + self.repeticiones.len()
    }

    pub fn es_error(&self) -> bool {
        self.tipo == TIPO_ERROR
    }

    /// `Leccion.leer`: `None` si no es una leccion (le falta el id, la fecha
    /// o el titulo, o un campo trae un tipo que no es el suyo). Lo que no se
    /// conoce se guarda aparte.
    pub fn leer(texto: &str) -> Option<Leccion> {
        Leccion::de_valor(&serde_json::from_str(texto).ok()?)
    }

    pub fn de_valor(v: &Value) -> Option<Leccion> {
        let o = v.as_object()?;
        let cadena = |k: &str| -> Option<String> {
            match o.get(k) {
                None => Some(String::new()),
                Some(Value::String(s)) => Some(s.clone()),
                Some(_) => None,
            }
        };
        let entero = |k: &str| -> Option<Option<i64>> {
            match o.get(k) {
                None => Some(None),
                Some(Value::Number(n)) => n.as_i64().map(Some),
                Some(_) => None,
            }
        };
        let lista = |k: &str| -> Option<Vec<String>> {
            match o.get(k) {
                None => Some(Vec::new()),
                Some(Value::Array(a)) => a.iter().map(|x| x.as_str().map(str::to_string)).collect(),
                Some(_) => None,
            }
        };
        let id = o.get("id")?.as_str()?.to_string();
        let creada = o.get("creada")?.as_i64()?;
        let titulo = o.get("titulo")?.as_str()?.to_string();
        let repeticiones = match o.get("repeticiones") {
            None => Vec::new(),
            Some(Value::Array(a)) => a.iter().map(Value::as_i64).collect::<Option<Vec<_>>>()?,
            Some(_) => return None,
        };
        let de_mensaje = match o.get("deMensaje") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return None,
        };
        let en_lista = match o.get("enLista") {
            None => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => return None,
        };
        let tipo = cadena("tipo")?;
        let mut resto = Map::new();
        for (k, v) in o {
            if !CAMPOS.contains(&k.as_str()) {
                resto.insert(k.clone(), v.clone());
            }
        }
        Some(Leccion {
            tocada: entero("tocada")?.unwrap_or(creada),
            que_paso: cadena("quePaso")?,
            por_que: cadena("porQue")?,
            proxima: cadena("proxima")?,
            tipo: if o.contains_key("tipo") {
                tipo
            } else {
                TIPO_LECCION.to_string()
            },
            area: cadena("area")?,
            gravedad: entero("gravedad")?.unwrap_or(1),
            etiquetas: lista("etiquetas")?,
            etiquetas_auto: lista("etiquetasAuto")?,
            quitadas: lista("quitadas")?,
            referencias: lista("referencias")?,
            causas: lista("causas")?,
            repeticiones,
            de_mensaje,
            caja: entero("caja")?.unwrap_or(0),
            repasar: entero("repasar")?.unwrap_or(creada + DIA),
            en_lista,
            adjuntos: lista("adjuntos")?,
            id,
            creada,
            titulo,
            resto,
        })
    }

    /// El JSON, con todos los campos (como `encodeDefaults = true`) en el
    /// orden de Kotlin y lo desconocido al final.
    pub fn a_valor(&self) -> Value {
        let mut o = Map::new();
        let l = |v: &[String]| Value::Array(v.iter().cloned().map(Value::String).collect());
        o.insert("id".into(), self.id.clone().into());
        o.insert("creada".into(), self.creada.into());
        o.insert("tocada".into(), self.tocada.into());
        o.insert("titulo".into(), self.titulo.clone().into());
        o.insert("quePaso".into(), self.que_paso.clone().into());
        o.insert("porQue".into(), self.por_que.clone().into());
        o.insert("proxima".into(), self.proxima.clone().into());
        o.insert("tipo".into(), self.tipo.clone().into());
        o.insert("area".into(), self.area.clone().into());
        o.insert("gravedad".into(), self.gravedad.into());
        o.insert("etiquetas".into(), l(&self.etiquetas));
        o.insert("etiquetasAuto".into(), l(&self.etiquetas_auto));
        o.insert("quitadas".into(), l(&self.quitadas));
        o.insert("referencias".into(), l(&self.referencias));
        o.insert("causas".into(), l(&self.causas));
        o.insert(
            "repeticiones".into(),
            Value::Array(self.repeticiones.iter().map(|r| (*r).into()).collect()),
        );
        o.insert(
            "deMensaje".into(),
            self.de_mensaje
                .clone()
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        o.insert("caja".into(), self.caja.into());
        o.insert("repasar".into(), self.repasar.into());
        o.insert("enLista".into(), self.en_lista.into());
        o.insert("adjuntos".into(), l(&self.adjuntos));
        for (k, v) in &self.resto {
            o.insert(k.clone(), v.clone());
        }
        Value::Object(o)
    }

    /// `Leccion.escribir`: JSON con sangria, como `prettyPrint` de Kotlin.
    pub fn escribir(&self) -> String {
        serde_json::to_string_pretty(&self.a_valor()).unwrap_or_default()
    }

    /// `nombreDe`: lo que lleva el mensaje como nombre del adjunto.
    pub fn nombre(&self) -> String {
        format!("💡 {}", self.titulo.chars().take(80).collect::<String>())
    }

    /// `LeccionesStore.resumen`: lo que se lee en el chat, en Windows y en
    /// las versiones que no conocen las lecciones. **Identico al de Kotlin**:
    /// el mensaje viaja y los dos lo reescriben.
    pub fn resumen(&self) -> String {
        let mut s = String::new();
        s.push_str(match self.tipo.as_str() {
            TIPO_ERROR => "⚠️ Error que no repetir",
            TIPO_ACIERTO => "✅ Lo que funcionó",
            _ => "💡 Lección",
        });
        s.push_str(": ");
        s.push_str(&self.titulo);
        if !self.que_paso.trim().is_empty() {
            s.push_str("\nQué pasó: ");
            s.push_str(&self.que_paso);
        }
        if !self.por_que.trim().is_empty() {
            s.push_str("\nPor qué: ");
            s.push_str(&self.por_que);
        }
        if !self.proxima.trim().is_empty() {
            s.push_str("\nLa próxima vez: ");
            s.push_str(&self.proxima);
        }
        if !self.repeticiones.is_empty() {
            s.push_str(&format!("\n🔁 Pasó {} veces", self.veces_que_paso()));
        }
        if !self.adjuntos.is_empty() {
            let n = self.adjuntos.len();
            s.push_str(&format!(
                "\n📎 {n}{}",
                if n == 1 { " adjunto" } else { " adjuntos" }
            ));
        }
        let etiquetas = self.todas_las_etiquetas();
        if !etiquetas.is_empty() {
            s.push('\n');
            s.push_str(
                &etiquetas
                    .iter()
                    .map(|e| format!("#{e}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        s
    }

    /// El icono de su tipo, como en las tarjetas del movil.
    pub fn icono(&self) -> &'static str {
        match self.tipo.as_str() {
            TIPO_ERROR => "⚠️",
            TIPO_ACIERTO => "✅",
            _ => "💡",
        }
    }
}

/// Si el `ruta` de un mensaje es el de una leccion (`esLeccion`).
pub fn es_ruta_de_leccion(ruta: &str) -> bool {
    ruta.ends_with(EXTENSION)
}

/// `nuevoId`: la hora en base 36 y tres signos al azar, como el movil.
pub fn nuevo_id(ahora: i64) -> String {
    const SIGNOS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";
    use std::sync::atomic::{AtomicU64, Ordering};
    static CONTADOR: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let mut x =
        nanos ^ (ahora as u64).rotate_left(17) ^ CONTADOR.fetch_add(0x9e37_79b9, Ordering::Relaxed);
    let mut s = base36(ahora);
    for _ in 0..3 {
        // xorshift: basta para no chocar con otra leccion del mismo milisegundo.
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.push(SIGNOS[(x % SIGNOS.len() as u64) as usize] as char);
    }
    s
}

/// `Long.toString(36)` de Kotlin.
pub fn base36(n: i64) -> String {
    if n == 0 {
        return "0".into();
    }
    let negativo = n < 0;
    let mut u = n.unsigned_abs();
    let mut v = Vec::new();
    while u > 0 {
        let d = (u % 36) as u8;
        v.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
        u /= 36;
    }
    if negativo {
        v.push(b'-');
    }
    v.reverse();
    String::from_utf8(v).unwrap_or_default()
}

/// Lo que se contesta en el repaso: «¿lo recordabas?».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nota {
    Recordaba,
    AMedias,
    Olvide,
}

impl Nota {
    /// Las tres, en el orden de sus teclas 1, 2 y 3.
    pub const TODAS: [Nota; 3] = [Nota::Recordaba, Nota::AMedias, Nota::Olvide];
}

/// **El repaso espaciado**: acordarse sube de caja y aleja el siguiente;
/// olvidarse vuelve a empezar.
pub struct Repaso;

impl Repaso {
    pub fn toca(l: &Leccion, ahora: i64) -> bool {
        l.repasar <= ahora
    }

    pub fn recordada(l: &Leccion, ahora: i64) -> Leccion {
        let caja = (l.caja + 1).min(INTERVALOS.len() as i64 - 1).max(0);
        Leccion {
            caja,
            repasar: ahora + INTERVALOS[caja as usize] * DIA,
            ..l.clone()
        }
    }

    pub fn olvidada(l: &Leccion, ahora: i64) -> Leccion {
        Leccion {
            caja: 0,
            repasar: ahora + DIA,
            ..l.clone()
        }
    }

    /// **Me volvio a pasar**: se apunta la fecha, sube la gravedad si se
    /// repite mucho y el repaso vuelve a empezar —si paso otra vez, no
    /// estaba aprendida—.
    pub fn repetida(l: &Leccion, ahora: i64) -> Leccion {
        let mut veces = l.repeticiones.clone();
        veces.push(ahora);
        let gravedad = if veces.len() >= 2 {
            l.gravedad.max(3)
        } else {
            l.gravedad.max(2)
        };
        Leccion {
            repeticiones: veces,
            gravedad,
            caja: 0,
            repasar: ahora + DIA,
            tocada: ahora,
            ..l.clone()
        }
    }

    /// **A medias** (lo anade el PC, v2): ni sube ni vuelve a empezar; baja
    /// una caja, para verla antes que si se hubiera recordado. Solo cambia
    /// `caja` y `repasar`, los mismos campos que el movil: el formato no se
    /// toca y el telefono la sigue repasando igual.
    pub fn a_medias(l: &Leccion, ahora: i64) -> Leccion {
        let caja = (l.caja - 1).clamp(0, INTERVALOS.len() as i64 - 1);
        Leccion {
            caja,
            repasar: ahora + INTERVALOS[caja as usize] * DIA,
            ..l.clone()
        }
    }

    /// La leccion tras contestar `nota`.
    pub fn calificar(l: &Leccion, nota: Nota, ahora: i64) -> Leccion {
        match nota {
            Nota::Recordaba => Repaso::recordada(l, ahora),
            Nota::AMedias => Repaso::a_medias(l, ahora),
            Nota::Olvide => Repaso::olvidada(l, ahora),
        }
    }

    /// **Cuantos dias tarda en volver** si se contesta `nota`: lo que dice
    /// cada boton («vuelve en 30 dias», «manana»).
    pub fn dias_hasta(l: &Leccion, nota: Nota) -> i64 {
        Repaso::calificar(l, nota, 0).repasar / DIA
    }

    /// Las que tocan hoy, primero las graves y las que mas se repiten.
    pub fn de_hoy(todas: &[Leccion], ahora: i64, cuantas: usize) -> Vec<Leccion> {
        let mut v: Vec<Leccion> = todas
            .iter()
            .filter(|l| Repaso::toca(l, ahora))
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            b.gravedad
                .cmp(&a.gravedad)
                .then(b.repeticiones.len().cmp(&a.repeticiones.len()))
                .then(a.repasar.cmp(&b.repasar))
        });
        v.truncate(cuantas);
        v
    }
}
