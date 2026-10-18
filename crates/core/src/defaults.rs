//! Valores por defecto: catálogo estándar de escritorios y reglas con sus sub-reglas.
//! Las medidas son estándar de mercado y se pueden editar en la app.

use crate::model::*;

pub fn default_catalog() -> Vec<DeskType> {
    let st = |id: &str, name: &str, color: &str, w: f64, d: f64, sides: u8, per: u32| DeskType {
        id: id.into(),
        name: name.into(),
        color: color.into(),
        shape: DeskShape::Straight,
        seat_width: w,
        depth: d,
        sides,
        per_side: per,
        flexible: false,
        min_per_side: per,
        max_per_side: per,
        return_length: 0.0,
        return_depth: 0.0,
        clear_chair: None,
        clear_back: 0.0,
        clear_left: 0.0,
        clear_right: 0.0,
    };
    let mut isla = st("isla_flex", "Isla cara a cara (flexible)", "#5e9c7f", 1200.0, 600.0, 2, 4);
    isla.flexible = true;
    isla.min_per_side = 2;
    isla.max_per_side = 4;
    let mut lineal = st("lineal_flex", "Bench lineal un lado (flexible)", "#c49a4e", 1200.0, 600.0, 1, 4);
    lineal.flexible = true;
    lineal.min_per_side = 2;
    lineal.max_per_side = 6;
    vec![
        st("recto_120", "Recto individual 120×60", "#5b80a8", 1200.0, 600.0, 1, 1),
        st("recto_140", "Recto individual 140×70", "#8a74ad", 1400.0, 700.0, 1, 1),
        DeskType {
            id: "l_150".into(),
            name: "Escritorio en L 150×150".into(),
            color: "#c4704f".into(),
            shape: DeskShape::L,
            seat_width: 1500.0,
            depth: 600.0,
            sides: 1,
            per_side: 1,
            flexible: false,
            min_per_side: 1,
            max_per_side: 1,
            return_length: 1500.0,
            return_depth: 600.0,
            clear_chair: None,
            clear_back: 0.0,
            clear_left: 0.0,
            clear_right: 0.0,
        },
        st("pareja", "Pareja cara a cara 2×(120×60)", "#4b9ea5", 1200.0, 600.0, 2, 1),
        st("isla_8", "Isla 2×4 (8 puestos)", "#3f6788", 1200.0, 600.0, 2, 4),
        isla,
        lineal,
    ]
}

fn item(id: &str, label: &str, value: Option<f64>, unit: &str, min: f64, max: f64, step: f64, relax_to: Option<f64>, help: &str) -> RuleItem {
    RuleItem { id: id.into(), label: label.into(), enabled: true, value, unit: unit.into(), min, max, step, relax_to, help: help.into() }
}
fn toggle(id: &str, label: &str, help: &str) -> RuleItem {
    item(id, label, None, "", 0.0, 0.0, 0.0, None, help)
}
fn mm(id: &str, label: &str, v: f64, min: f64, max: f64, relax: Option<f64>, help: &str) -> RuleItem {
    item(id, label, Some(v), "mm", min, max, 50.0, relax, help)
}

#[allow(clippy::too_many_arguments)]
fn rule(id: &str, group: &str, name: &str, desc: &str, hard: bool, locked: bool, weight: f64, items: Vec<RuleItem>) -> Rule {
    Rule { id: id.into(), group: group.into(), name: name.into(), description: desc.into(), enabled: true, locked, hard, weight, items }
}

pub const G_CIRC: &str = "Circulación y evacuación";
pub const G_DESK: &str = "Escritorios y sillas";
pub const G_OBS: &str = "Obstáculos";
pub const G_AMB: &str = "Confort y ambiente";
pub const G_ORG: &str = "Organización (roles y clientes)";

pub fn default_rules() -> Vec<Rule> {
    vec![
        // ---------------- Circulación ----------------
        rule("C1", G_CIRC, "Pasillo principal",
            "Columna vertebral que une la puerta principal con las salidas y las puertas de oficinas. Se traza antes que los escritorios.",
            true, false, 1.0, vec![
                mm("ancho", "Ancho mínimo", 1500.0, 900.0, 3000.0, Some(1200.0), "Referencia típica 1,2–1,5 m. Verifica NSR-10 Título K."),
                toggle("salidas", "Conectar con salidas de emergencia", "El pasillo principal llega a cada salida."),
                {
                    let mut t = toggle("oficinas", "Llevar el pasillo principal hasta cada puerta de oficina", "Si está apagado, basta con que cada puerta quede accesible por un pasillo secundario (regla C4/C7).");
                    t.enabled = false;
                    t
                },
                toggle("recto", "Preferir tramos rectos", "Penaliza giros para que el pasillo sea ordenado."),
            ]),
        rule("C2", G_CIRC, "Pasillos secundarios",
            "Pasillos que llegan a las bancadas desde el pasillo principal.",
            true, false, 1.0, vec![
                mm("ancho", "Ancho mínimo", 900.0, 600.0, 2000.0, Some(800.0), "Paso cómodo de una persona: 0,9–1,0 m."),
                item("paso_sillas", "Paso angosto detrás de sillas (último tramo)", Some(600.0), "mm", 400.0, 1200.0, 50.0, Some(500.0),
                    "Ancho mínimo para llegar a la silla desde el pasillo en los últimos metros."),
            ]),
        rule("C3", G_CIRC, "Vestíbulo libre en la puerta principal",
            "Zona libre frente a la puerta principal para que no quede bloqueada.",
            true, false, 1.0, vec![
                mm("profundidad", "Profundidad hacia adentro", 2000.0, 0.0, 6000.0, Some(1500.0), ""),
                mm("ancho", "Ancho", 2400.0, 0.0, 8000.0, Some(1800.0), "Medido a lo largo del muro, centrado en la puerta."),
            ]),
        rule("C4", G_CIRC, "Despeje en puertas",
            "Ninguna puerta queda bloqueada: se respeta el barrido de la hoja y un espacio libre al frente.",
            true, false, 1.0, vec![
                toggle("barrido", "Respetar barrido de la hoja", "Área del arco de apertura (radio = ancho de la puerta)."),
                mm("extra", "Espacio libre adicional al frente", 1200.0, 0.0, 3000.0, Some(900.0), ""),
                toggle("salidas", "Aplicar a salidas de emergencia", ""),
                toggle("oficinas", "Aplicar a puertas de oficinas", ""),
            ]),
        rule("C5", G_CIRC, "Recorrido máximo hasta una salida",
            "Distancia caminando desde cualquier puesto hasta la salida más cercana (puerta principal o de emergencia).",
            true, true, 1.0, vec![
                item("dist", "Distancia máxima", Some(45000.0), "mm", 10000.0, 100000.0, 1000.0, None, "Valor típico 45–60 m según ocupación y rociadores. Verifica NSR-10 Título K."),
            ]),
        rule("C6", G_CIRC, "Recorrido común (sin alternativa)",
            "Tramo desde el puesto hasta llegar al pasillo principal, donde ya hay más de una ruta de escape.",
            true, true, 1.0, vec![
                item("dist", "Distancia máxima", Some(15000.0), "mm", 3000.0, 50000.0, 500.0, None, "Aproximación a la regla de pasillos sin salida."),
            ]),
        rule("C7", G_CIRC, "Todos los puestos accesibles desde la entrada",
            "Cada silla se alcanza caminando desde la puerta principal por pasillos del ancho mínimo.",
            true, true, 1.0, vec![
                toggle("desde_entrada", "Medir desde la puerta principal", "Si se apaga, basta con que el puesto llegue a cualquier salida."),
                toggle("puertas", "Ninguna puerta queda bloqueada", "Cada puerta de oficina y salida se alcanza desde la entrada por un pasillo del ancho mínimo."),
            ]),
        rule("C8", G_CIRC, "Puestos accesibles en silla de ruedas",
            "Una parte de los puestos tiene ruta ancha y espacio de giro.",
            false, false, 3.0, vec![
                item("pct", "Porcentaje mínimo de puestos", Some(2.0), "%", 0.0, 100.0, 1.0, Some(0.0), ""),
                mm("giro", "Diámetro de giro", 1500.0, 1200.0, 2000.0, None, ""),
                mm("ruta", "Ancho de ruta", 900.0, 800.0, 1500.0, None, ""),
            ]),
        // ---------------- Escritorios ----------------
        rule("E1", G_DESK, "Espacio para la silla",
            "Espacio detrás del borde del escritorio, del lado donde se sienta la persona.",
            true, false, 1.0, vec![
                mm("muro", "Del borde del escritorio a muro u obstáculo", 1000.0, 600.0, 2000.0, Some(900.0), "Para poder correr la silla y levantarse."),
                mm("silla", "Huella de la silla ocupada", 500.0, 300.0, 900.0, None, "Se usa para comprobar que el paso detrás quede libre."),
            ]),
        rule("E2", G_DESK, "Separación entre filas de escritorios",
            "Distancia entre bordes de escritorios de filas vecinas.",
            true, false, 1.0, vec![
                mm("silla_silla", "Silla con silla (con circulación)", 1800.0, 1200.0, 3000.0, Some(1500.0), "Dos filas de sillas espalda con espalda con paso en medio."),
                mm("silla_espalda", "Silla con espalda de escritorio", 1200.0, 800.0, 2500.0, Some(1000.0), ""),
                toggle("pegar_espaldas", "Permitir escritorios pegados por la espalda", "Dos escritorios de un lado unidos por el borde trasero."),
            ]),
        rule("E3", G_DESK, "Máximo de puestos seguidos por lado",
            "Corta las bancadas largas con un paso transversal para no caminar demasiado.",
            true, false, 1.0, vec![
                item("max", "Puestos seguidos por lado", Some(8.0), "puestos", 1.0, 30.0, 1.0, Some(12.0), ""),
                mm("corte", "Ancho del paso transversal", 1200.0, 600.0, 2500.0, Some(900.0), ""),
            ]),
        rule("E4", G_DESK, "Distancia máxima de la silla al pasillo",
            "Recorrido entre la silla y el pasillo secundario más cercano.",
            true, false, 1.0, vec![
                mm("dist", "Distancia máxima", 4800.0, 1000.0, 20000.0, Some(7200.0), "≈ 4 puestos de 1,2 m."),
            ]),
        rule("E5", G_DESK, "No mezclar tipos en una bancada",
            "Cada fila de escritorios usa un solo tipo.",
            true, false, 1.0, vec![toggle("activo", "Activo", "")]),
        rule("E6", G_DESK, "Orientaciones permitidas",
            "Direcciones en las que se pueden alinear las filas.",
            true, false, 1.0, vec![
                toggle("muros_inclinados", "Alinear también con muros inclinados", "Si se apaga, solo direcciones ortogonales al muro principal."),
                item("tolerancia", "Agrupar ángulos parecidos", Some(4.0), "°", 0.0, 20.0, 1.0, None, ""),
            ]),
        rule("E7", G_DESK, "Misma dirección en todas las bancadas",
            "Orden visual: todas las filas paralelas. Como regla blanda, penaliza cada orientación adicional.",
            false, false, 2.0, vec![toggle("activo", "Activo", "")]),
        rule("E8", G_DESK, "Escritorios contra el muro",
            "Los lados sin silla pueden tocar muros.",
            true, false, 1.0, vec![
                toggle("tocar", "Permitir tocar el muro por lados sin silla", ""),
                mm("separacion", "Separación si no se permite", 300.0, 0.0, 1500.0, Some(0.0), ""),
            ]),
        // ---------------- Obstáculos ----------------
        rule("O1", G_OBS, "Separación a columnas",
            "Distancia mínima entre escritorios/sillas y columnas.",
            true, false, 1.0, vec![mm("dist", "Separación", 200.0, 0.0, 1500.0, Some(0.0), "La columna nunca invade la zona de silla.")]),
        rule("O2", G_OBS, "Despeje de tableros y gabinetes",
            "Espacio libre al frente de tableros eléctricos, gabinetes contra incendio y extintores.",
            true, true, 1.0, vec![mm("dist", "Despeje", 1000.0, 500.0, 3000.0, None, "")]),
        rule("O3", G_OBS, "Distancia a oficinas fijas",
            "Separación entre escritorios y muros de oficinas internas.",
            true, false, 1.0, vec![mm("dist", "Separación", 600.0, 0.0, 3000.0, Some(0.0), "")]),
        rule("O4", G_OBS, "Zonas reservadas",
            "Impresoras, archivo, lockers, cafetería u otras áreas dibujadas por el usuario quedan libres.",
            true, false, 1.0, vec![toggle("activo", "Respetar zonas reservadas", "")]),
        // ---------------- Ambiente ----------------
        rule("A1", G_AMB, "Ventana al costado del monitor",
            "Evita reflejos y contraluz: ni de frente ni de espaldas a la ventana.",
            false, false, 1.0, vec![
                mm("dist", "Distancia de influencia", 8000.0, 1000.0, 20000.0, None, ""),
                item("tolerancia", "Tolerancia", Some(45.0), "°", 10.0, 80.0, 5.0, None, ""),
            ]),
        rule("A2", G_AMB, "No dar la espalda",
            "Nadie queda de espaldas a la puerta principal o al pasillo principal.",
            false, false, 1.0, vec![
                item("puerta", "A la puerta principal (distancia)", Some(6000.0), "mm", 1000.0, 20000.0, 500.0, None, ""),
                toggle("pasillo", "Al pasillo principal", ""),
            ]),
        rule("A3", G_AMB, "Pantallas no visibles",
            "Las pantallas no se ven desde la entrada ni desde el pasillo principal.",
            false, false, 1.0, vec![
                item("entrada", "Desde la entrada (distancia)", Some(12000.0), "mm", 1000.0, 40000.0, 500.0, None, ""),
                toggle("pasillo", "Desde el pasillo principal", ""),
            ]),
        rule("A4", G_AMB, "Luz natural",
            "Maximiza los puestos cerca de ventanas.",
            false, false, 1.0, vec![mm("dist", "Distancia máxima a ventana", 6000.0, 1000.0, 20000.0, None, "")]),
        rule("A5", G_AMB, "Densidad mínima",
            "Área del piso por puesto. Si se incumple se avisa.",
            false, false, 1.0, vec![item("m2", "m² por puesto (mínimo)", Some(5.0), "m²", 1.0, 30.0, 0.5, Some(3.5), "")]),
        rule("A6", G_AMB, "Reserva de crecimiento",
            "Deja capacidad libre para crecer.",
            false, false, 1.0, vec![item("pct", "Reserva", Some(10.0), "%", 0.0, 100.0, 1.0, Some(0.0), "")]),
        // ---------------- Organización ----------------
        rule("R1", G_ORG, "Rol ↔ tipo de escritorio",
            "Cada rol usa los tipos de escritorio que le asignes (p. ej. PM → L).",
            false, false, 4.0, vec![toggle("activo", "Activo", "")]),
        rule("R2", G_ORG, "Clientes en bloque contiguo",
            "Los equipos de un mismo cliente se sientan juntos.",
            false, false, 3.0, vec![mm("radio", "Radio de vecindad", 3000.0, 1000.0, 10000.0, None, "")]),
        rule("R3", G_ORG, "Afinidad entre roles",
            "Matriz de quién debe estar cerca de quién (dentro del mismo cliente).",
            false, false, 1.0, vec![mm("radio", "Radio de cercanía", 4000.0, 1000.0, 15000.0, None, "")]),
        rule("R4", G_ORG, "Rol cerca de un elemento",
            "PMs cerca de la sala de juntas, recepción cerca de la entrada, etc. (se define en cada rol).",
            false, false, 1.0, vec![toggle("activo", "Activo", "")]),
        rule("R5", G_ORG, "Separación acústica",
            "Roles ruidosos lejos de roles que requieren concentración.",
            false, false, 1.0, vec![mm("dist", "Distancia mínima", 5000.0, 1000.0, 20000.0, None, "")]),
    ]
}

/// Configuración por defecto enfocada en capacidad: solo espacios de circulación y acceso.
/// Las demás reglas existen pero vienen apagadas.
fn capacity_focus(mut rules: Vec<Rule>) -> Vec<Rule> {
    const ON: [&str; 13] = ["C1", "C2", "C3", "C4", "C7", "E1", "E2", "E3", "E6", "E8", "O1", "O2", "O4"];
    const ORG: [&str; 5] = ["R1", "R2", "R3", "R4", "R5"];
    for r in rules.iter_mut() {
        r.enabled = ON.contains(&r.id.as_str()) || ORG.contains(&r.id.as_str());
        let set = |r: &mut Rule, item: &str, v: f64| {
            if let Some(i) = r.items.iter_mut().find(|i| i.id == item) {
                i.value = Some(v);
            }
        };
        match r.id.as_str() {
            "C1" => set(r, "ancho", 1200.0),
            "C3" => {
                set(r, "profundidad", 1500.0);
                set(r, "ancho", 2000.0);
            }
            "C4" => set(r, "extra", 900.0),
            "O3" => {
                r.enabled = true;
                set(r, "dist", 300.0);
            }
            _ => {}
        }
    }
    rules
}

pub fn default_config() -> Config {
    Config {
        catalog: default_catalog(),
        demand: vec![],
        rules: capacity_focus(default_rules()),
        custom_rules: vec![],
        weights: Weights::default(),
        org: Organization::default(),
        settings: Settings::default(),
        pinned: vec![],
    }
}

/// Acceso cómodo a reglas.
pub struct RuleView<'a> {
    pub rules: &'a [Rule],
}

impl<'a> RuleView<'a> {
    pub fn new(rules: &'a [Rule]) -> Self {
        Self { rules }
    }
    pub fn rule(&self, id: &str) -> Option<&'a Rule> {
        self.rules.iter().find(|r| r.id == id)
    }
    pub fn on(&self, id: &str) -> bool {
        self.rule(id).map(|r| r.enabled).unwrap_or(false)
    }
    /// ¿Sub-regla activa (y regla activa)?
    pub fn item_on(&self, id: &str, item: &str) -> bool {
        self.rule(id)
            .filter(|r| r.enabled)
            .and_then(|r| r.items.iter().find(|i| i.id == item))
            .map(|i| i.enabled)
            .unwrap_or(false)
    }
    /// Valor de una sub-regla si está activa.
    pub fn val(&self, id: &str, item: &str) -> Option<f64> {
        self.rule(id)
            .filter(|r| r.enabled)
            .and_then(|r| r.items.iter().find(|i| i.id == item))
            .filter(|i| i.enabled)
            .and_then(|i| i.value)
    }
    pub fn val_or(&self, id: &str, item: &str, default: f64) -> f64 {
        self.val(id, item).unwrap_or(default)
    }
    pub fn hard(&self, id: &str) -> bool {
        self.rule(id).map(|r| r.hard).unwrap_or(false)
    }
    pub fn weight(&self, id: &str) -> f64 {
        self.rule(id).filter(|r| r.enabled).map(|r| r.weight).unwrap_or(0.0)
    }
}
