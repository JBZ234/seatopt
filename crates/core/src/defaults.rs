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
        clear_right: 0.0, body: vec![], seats: vec![],
    };
    let mut isla = st("isla_flex", "Face-to-face island (flexible)", "#5e9c7f", 1200.0, 600.0, 2, 4);
    isla.flexible = true;
    isla.min_per_side = 2;
    isla.max_per_side = 4;
    let mut lineal = st("lineal_flex", "One-sided bench (flexible)", "#c49a4e", 1200.0, 600.0, 1, 4);
    lineal.flexible = true;
    lineal.min_per_side = 2;
    lineal.max_per_side = 6;
    vec![
        st("recto_120", "Single straight 120×60", "#5b80a8", 1200.0, 600.0, 1, 1),
        st("recto_140", "Single straight 140×70", "#8a74ad", 1400.0, 700.0, 1, 1),
        DeskType {
            id: "l_150".into(),
            name: "L desk 150×150".into(),
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
            clear_right: 0.0, body: vec![], seats: vec![],
        },
        st("pareja", "Face-to-face pair 2×(120×60)", "#4b9ea5", 1200.0, 600.0, 2, 1),
        st("isla_8", "Island 2×4 (8 seats)", "#3f6788", 1200.0, 600.0, 2, 4),
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

pub const G_CIRC: &str = "Circulation and egress";
pub const G_DESK: &str = "Desks and chairs";
pub const G_OBS: &str = "Obstacles";
pub const G_AMB: &str = "Comfort and environment";
pub const G_ORG: &str = "Organization (roles and clients)";

pub fn default_rules() -> Vec<Rule> {
    vec![
        // ---------------- Circulación ----------------
        rule("C1", G_CIRC, "Main corridor",
            "Backbone joining the main entrance with the exits and office doors. It is laid out before the desks.",
            true, false, 1.0, vec![
                mm("ancho", "Minimum width", 1500.0, 900.0, 3000.0, Some(1200.0), "Typical 1.2–1.5 m. Check your local egress code."),
                toggle("salidas", "Connect to emergency exits", "The main corridor reaches every exit."),
                {
                    let mut t = toggle("oficinas", "Take the main corridor to every office door", "If off, it is enough that every door can be reached by a secondary aisle (rules C4/C7).");
                    t.enabled = false;
                    t
                },
                toggle("recto", "Prefer straight runs", "Penalizes turns so the corridor is tidy."),
            ]),
        rule("C2", G_CIRC, "Aisles to the seats",
            "Width of the ways to reach each seat. The narrowest gap is required (no seat is boxed in); the preferred width is only preferred.",
            true, false, 1.0, vec![
                mm("ancho", "Preferred width", 900.0, 600.0, 2000.0, Some(800.0), "Reaching each seat through aisles this wide without moving chairs is preferred, but not required."),
                item("paso_sillas", "Narrowest gap between desks", Some(600.0), "mm", 400.0, 1200.0, 50.0, Some(500.0),
                    "Narrowest gap people can walk through between desks, walls or obstacles. Chairs do not count: they move."),
            ]),
        rule("P1", G_CIRC, "Circulation paths",
            "Paths from the entrance to every office door and exit (and, if you keep them, between offices). Kept paths stay clear: no desk or chair goes on them.",
            true, false, 1.0, vec![
                mm("primary", "Entrance paths width", 1200.0, 600.0, 3000.0, Some(900.0), "Clear width of paths from the entrance to doors."),
                mm("secondary", "Office-to-office paths width", 900.0, 600.0, 3000.0, Some(800.0), "Clear width of paths between offices."),
            ]),
        rule("C3", G_CIRC, "Entrance lobby",
            "Free area in front of the main entrance so it is not blocked.",
            true, false, 1.0, vec![
                mm("profundidad", "Depth inward", 2000.0, 0.0, 6000.0, Some(1500.0), ""),
                mm("ancho", "Width", 2400.0, 0.0, 8000.0, Some(1800.0), "Measured along the wall, centered on the door."),
            ]),
        rule("C4", G_CIRC, "Door clearance",
            "No door is blocked: the leaf swing and a free space in front are respected.",
            true, false, 1.0, vec![
                toggle("barrido", "Respect the leaf swing", "Area of the opening arc (radius = door width)."),
                mm("extra", "Extra free space in front", 1200.0, 0.0, 3000.0, Some(900.0), ""),
                toggle("salidas", "Apply to emergency exits", ""),
                toggle("oficinas", "Apply to office doors", ""),
            ]),
        rule("C5", G_CIRC, "Maximum travel to an exit",
            "Walking distance from any seat to the nearest exit (main entrance or emergency exit).",
            true, true, 1.0, vec![
                item("dist", "Maximum distance", Some(45000.0), "mm", 10000.0, 100000.0, 1000.0, None, "Typically 45–60 m depending on occupancy and sprinklers. Check your local egress code."),
            ]),
        rule("C6", G_CIRC, "Common path of travel",
            "Stretch from the seat to the main corridor, where there is more than one escape route.",
            true, true, 1.0, vec![
                item("dist", "Maximum distance", Some(15000.0), "mm", 3000.0, 50000.0, 500.0, None, "Approximation of the dead-end rule."),
            ]),
        rule("C7", G_CIRC, "No seat boxed in",
            "Every chair can be reached by some route (not necessarily straight from the entrance; some take a longer way). It only requires no gaps narrower than the narrowest gap between desks.",
            true, true, 1.0, vec![
                toggle("desde_entrada", "Count from the main entrance", "If off, reaching it from any door or exit is enough."),
                toggle("puertas", "No door is blocked", "Every office door and exit is reached through a way of the preferred width."),
            ]),
        rule("C8", G_CIRC, "Wheelchair-accessible seats",
            "Some seats get a wide route and turning space.",
            false, false, 3.0, vec![
                item("pct", "Minimum share of seats", Some(2.0), "%", 0.0, 100.0, 1.0, Some(0.0), ""),
                mm("giro", "Turning diameter", 1500.0, 1200.0, 2000.0, None, ""),
                mm("ruta", "Route width", 900.0, 800.0, 1500.0, None, ""),
            ]),
        // ---------------- Escritorios ----------------
        rule("E1", G_DESK, "Chair space",
            "Space behind the desk edge, on the side where the person sits.",
            true, false, 1.0, vec![
                mm("muro", "From the desk edge to a wall or obstacle", 1000.0, 600.0, 2000.0, Some(900.0), "To push the chair back and stand up."),
                mm("silla", "Footprint of an occupied chair", 500.0, 300.0, 900.0, None, "Used to check that the way behind stays free."),
            ]),
        rule("E2", G_DESK, "Space between desk rows",
            "Distance between desk edges of neighbouring rows.",
            true, false, 1.0, vec![
                mm("silla_silla", "Chairs back to back (with walkway)", 1800.0, 1200.0, 3000.0, Some(1500.0), "Two rows of chairs back to back with a way between."),
                mm("silla_espalda", "Chair to the back of a desk", 1200.0, 800.0, 2500.0, Some(1000.0), ""),
                toggle("pegar_espaldas", "Allow desks back to back", "Two single-sided desks joined at their back edge."),
            ]),
        rule("E3", G_DESK, "Maximum seats in a row",
            "Breaks long benches with a cross gap so nobody walks too far.",
            true, false, 1.0, vec![
                item("max", "Seats in a row per side", Some(8.0), "seats", 1.0, 30.0, 1.0, Some(12.0), ""),
                mm("corte", "Cross gap width", 1200.0, 600.0, 2500.0, Some(900.0), ""),
            ]),
        rule("E4", G_DESK, "Maximum distance from chair to aisle",
            "Walk between the chair and the nearest secondary aisle.",
            true, false, 1.0, vec![
                mm("dist", "Maximum distance", 4800.0, 1000.0, 20000.0, Some(7200.0), "≈ 4 seats of 1.2 m."),
            ]),
        rule("E5", G_DESK, "Do not mix types in a bench",
            "Each row of desks uses a single type.",
            true, false, 1.0, vec![toggle("activo", "On", "")]),
        rule("E6", G_DESK, "Allowed directions",
            "Directions the rows can follow.",
            true, false, 1.0, vec![
                toggle("muros_inclinados", "Also align with slanted walls", "If off, only directions square to the main wall."),
                item("tolerancia", "Merge similar angles", Some(4.0), "°", 0.0, 20.0, 1.0, None, ""),
            ]),
        rule("E7", G_DESK, "Same direction for all rows",
            "Visual order: all rows parallel. As a soft rule it penalizes each extra direction.",
            false, false, 2.0, vec![toggle("activo", "On", "")]),
        rule("E8", G_DESK, "Desks against walls",
            "Sides without chairs may touch walls.",
            true, false, 1.0, vec![
                toggle("tocar", "Allow touching walls on sides without chairs", ""),
                mm("separacion", "Clearance if not allowed", 300.0, 0.0, 1500.0, Some(0.0), ""),
            ]),
        // ---------------- Obstáculos ----------------
        rule("O1", G_OBS, "Clearance to columns",
            "Minimum distance between desks/chairs and columns.",
            true, false, 1.0, vec![mm("dist", "Clearance", 200.0, 0.0, 1500.0, Some(0.0), "A column never invades the chair zone.")]),
        rule("O2", G_OBS, "Panel and cabinet clearance",
            "Free space in front of electrical panels, fire cabinets and extinguishers.",
            true, true, 1.0, vec![mm("dist", "Clearance", 1000.0, 500.0, 3000.0, None, "")]),
        rule("O3", G_OBS, "Clearance to fixed offices",
            "Clearance between desks and the walls of internal offices.",
            true, false, 1.0, vec![mm("dist", "Clearance", 600.0, 0.0, 3000.0, Some(0.0), "")]),
        rule("O4", G_OBS, "Reserved areas",
            "Printers, filing, lockers, pantry or other areas you draw stay free.",
            true, false, 1.0, vec![toggle("activo", "Respect reserved areas", "")]),
        // ---------------- Ambiente ----------------
        rule("A1", G_AMB, "Window beside the monitor",
            "Avoids glare and backlight: neither facing nor with the back to the window.",
            false, false, 1.0, vec![
                mm("dist", "Influence distance", 8000.0, 1000.0, 20000.0, None, ""),
                item("tolerancia", "Tolerance", Some(45.0), "°", 10.0, 80.0, 5.0, None, ""),
            ]),
        rule("A2", G_AMB, "Do not turn your back",
            "Nobody sits with their back to the main entrance or the main corridor.",
            false, false, 1.0, vec![
                item("puerta", "To the main entrance (distance)", Some(6000.0), "mm", 1000.0, 20000.0, 500.0, None, ""),
                toggle("pasillo", "To the main corridor", ""),
            ]),
        rule("A3", G_AMB, "Screens not visible",
            "Screens cannot be seen from the entrance or the main corridor.",
            false, false, 1.0, vec![
                item("entrada", "From the entrance (distance)", Some(12000.0), "mm", 1000.0, 40000.0, 500.0, None, ""),
                toggle("pasillo", "From the main corridor", ""),
            ]),
        rule("A4", G_AMB, "Daylight",
            "Maximizes seats near windows.",
            false, false, 1.0, vec![mm("dist", "Maximum distance to a window", 6000.0, 1000.0, 20000.0, None, "")]),
        rule("A5", G_AMB, "Minimum density",
            "Floor area per seat. A warning is shown if not met.",
            false, false, 1.0, vec![item("m2", "m² per seat (minimum)", Some(5.0), "m²", 1.0, 30.0, 0.5, Some(3.5), "")]),
        rule("A6", G_AMB, "Growth reserve",
            "Leaves free capacity to grow.",
            false, false, 1.0, vec![item("pct", "Reserve", Some(10.0), "%", 0.0, 100.0, 1.0, Some(0.0), "")]),
        // ---------------- Organización ----------------
        rule("R1", G_ORG, "Role ↔ desk type",
            "Each role uses the desk types you assign (e.g. PM → L).",
            false, false, 4.0, vec![toggle("activo", "On", "")]),
        rule("R2", G_ORG, "Clients in one block",
            "Teams of the same client sit together.",
            false, false, 3.0, vec![mm("radio", "Neighbourhood radius", 3000.0, 1000.0, 10000.0, None, "")]),
        rule("R3", G_ORG, "Affinity between roles",
            "Who should be near whom (within the same client).",
            false, false, 1.0, vec![mm("radio", "Nearness radius", 4000.0, 1000.0, 15000.0, None, "")]),
        rule("R4", G_ORG, "Role near an element",
            "PMs near the meeting room, reception near the entrance, etc. (set in each role).",
            false, false, 1.0, vec![toggle("activo", "On", "")]),
        rule("R5", G_ORG, "Acoustic separation",
            "Loud roles away from roles that need focus.",
            false, false, 1.0, vec![mm("dist", "Minimum distance", 5000.0, 1000.0, 20000.0, None, "")]),
    ]
}

/// Configuración por defecto: la que da más puestos. Solo quedan encendidas las medidas físicas
/// mínimas (espacio de la silla y entre filas), que ningún puesto quede encerrado ni ninguna puerta
/// bloqueada, y respetar las zonas reservadas que se dibujen. Todo lo demás viene apagado y se
/// enciende a gusto en «Espacios».
fn capacity_focus(mut rules: Vec<Rule>) -> Vec<Rule> {
    const ON: [&str; 10] = ["C2", "C4", "C7", "E1", "E2", "E8", "O1", "O3", "O4", "P1"];
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
            // doors: the leaf swing plus 0.60 m clear in front (main entrance, exits and offices)
            "C4" => set(r, "extra", 600.0),
            "O1" => set(r, "dist", 200.0),
            "O3" => set(r, "dist", 300.0),
            // paredes del recinto: 0,20 m libres (los lados sin silla ya no tocan el muro)
            "E8" => {
                set(r, "separacion", 200.0);
                if let Some(i) = r.items.iter_mut().find(|i| i.id == "tocar") {
                    i.enabled = false;
                }
            }
            _ => {}
        }
    }
    rules
}

pub fn default_config() -> Config {
    Config {
        catalog: default_catalog(),
        // por defecto: el escritorio recto básico, todos los que quepan
        demand: vec![Demand { type_id: "recto_120".into(), quantity: 0, max: true }],
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
