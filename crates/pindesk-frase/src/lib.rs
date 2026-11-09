//! **Lo que se entiende de una frase escrita**, sin ventanas, disco ni reloj.
//!
//! - [`fechas`]: las fechas dichas en español («mañana a las 10», «18/10
//!   17:00»), las de las tareas del asistente y las de la app.
//! - [`tarea`]: lo que se reconoce al apuntar una tarea (#etiquetas en
//!   cualquier sitio, urgencia normal / urgente / muy urgente y la hora de
//!   recordar).
//!
//! Un solo parser para la ventana de Tareas, el pin de una tarea, el pedido
//! `anadir_tarea` y el plugin de Flow Launcher (18-oct-2026): por eso vive
//! aqui, sin dependencias, y no dentro de la app.

#![forbid(unsafe_code)]

pub mod fechas;
pub mod tarea;
