//! Tipos de datos compartidos entre el núcleo, la app Tauri y la interfaz.
//! Todas las medidas están en milímetros salvo que se indique otra cosa.

use crate::geom::Pt;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Plano
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ObstacleKind {
    /// Columna o pilar.
    Column,
    /// Oficina interna fija, sala, baño, núcleo… (no se amobla).
    FixedOffice,
    /// Zona restringida que pide despeje frontal (tablero eléctrico, gabinete contra incendio).
    Restricted,
    /// Zona reservada dibujada por el usuario (impresoras, archivo, lockers…).
    Reserved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Obstacle {
    pub id: String,
    pub kind: ObstacleKind,
    pub poly: Vec<Pt>,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DoorKind {
    Main,
    Exit,
    Office,
    Ignore,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Door {
    pub id: String,
    pub kind: DoorKind,
    /// Extremos del vano.
    pub a: Pt,
    pub b: Pt,
    #[serde(default)]
    pub label: String,
}

impl Door {
    pub fn width(&self) -> f64 {
        self.a.dist(self.b)
    }
    pub fn mid(&self) -> Pt {
        self.a.lerp(self.b, 0.5)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub a: Pt,
    pub b: Pt,
}

/// Plano normalizado y editable: recinto + obstáculos + puertas + ventanas.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Contornos del área amoblable (regla par-impar: los contornos internos son huecos).
    pub region: Vec<Vec<Pt>>,
    #[serde(default)]
    pub obstacles: Vec<Obstacle>,
    #[serde(default)]
    pub doors: Vec<Door>,
    #[serde(default)]
    pub windows: Vec<Window>,
    /// Referencia (no la usa el optimizador): oficinas y recintos cerrados fuera del área amoblable.
    #[serde(default)]
    pub rooms: Vec<Room>,
    /// Referencia: contorno exterior del edificio.
    #[serde(default)]
    pub building: Vec<Pt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub poly: Vec<Pt>,
    #[serde(default)]
    pub label: String,
}

// ---------------------------------------------------------------------------
// Catálogo de escritorios
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeskShape {
    /// Superficie rectangular; puestos sobre uno o dos lados largos.
    Straight,
    /// Escritorio en L (un puesto por módulo, en la esquina interna).
    L,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeskType {
    pub id: String,
    pub name: String,
    pub color: String,
    pub shape: DeskShape,
    /// Ancho por puesto a lo largo de la fila.
    pub seat_width: f64,
    /// Fondo de la superficie por lado.
    pub depth: f64,
    /// 1 = puestos en un solo lado, 2 = cara a cara (isla / bench).
    pub sides: u8,
    /// Puestos por lado en un módulo (para tipos fijos).
    pub per_side: u32,
    /// Si es flexible, el optimizador arma islas de `min_per_side`..`max_per_side` puestos por lado
    /// y la cantidad pedida se interpreta en PUESTOS; si no, en MÓDULOS.
    #[serde(default)]
    pub flexible: bool,
    #[serde(default = "one")]
    pub min_per_side: u32,
    #[serde(default = "one")]
    pub max_per_side: u32,
    /// Para escritorios en L: largo del retorno (perpendicular a la fila).
    #[serde(default)]
    pub return_length: f64,
    /// Para escritorios en L: fondo del retorno.
    #[serde(default)]
    pub return_depth: f64,
}

fn one() -> u32 {
    1
}

impl DeskType {
    /// Largo de un módulo con `k` puestos por lado.
    pub fn module_length(&self, k: u32) -> f64 {
        match self.shape {
            DeskShape::Straight => self.seat_width * k as f64,
            DeskShape::L => self.seat_width,
        }
    }
    /// Fondo total del cuerpo del módulo.
    pub fn module_depth(&self) -> f64 {
        match self.shape {
            DeskShape::Straight => self.depth * self.sides as f64,
            DeskShape::L => self.return_length.max(self.depth),
        }
    }
    pub fn seats_per_module(&self, k: u32) -> u32 {
        match self.shape {
            DeskShape::Straight => k * self.sides as u32,
            DeskShape::L => 1,
        }
    }
    pub fn per_side_range(&self) -> (u32, u32) {
        if self.flexible && self.shape == DeskShape::Straight {
            (self.min_per_side.max(1), self.max_per_side.max(self.min_per_side.max(1)))
        } else {
            (self.per_side.max(1), self.per_side.max(1))
        }
    }
    pub fn is_two_sided(&self) -> bool {
        self.shape == DeskShape::Straight && self.sides >= 2
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Demand {
    pub type_id: String,
    /// Módulos (tipos fijos) o puestos (tipos flexibles).
    pub quantity: u32,
    /// «Máximo posible»: ignora la cantidad y ubica todos los que quepan
    /// (después de asegurar los tipos con cantidad fija).
    #[serde(default)]
    pub max: bool,
}

// ---------------------------------------------------------------------------
// Reglas
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    /// Valor numérico (None = sub-regla de solo encendido/apagado).
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub min: f64,
    #[serde(default)]
    pub max: f64,
    #[serde(default)]
    pub step: f64,
    /// Valor más bajo al que el sugeridor puede relajarla (None = no relajable).
    #[serde(default)]
    pub relax_to: Option<f64>,
    #[serde(default)]
    pub help: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub group: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// Bloqueada = el sugeridor nunca propone relajarla (p. ej. evacuación).
    pub locked: bool,
    /// Dura = se cumple siempre; blanda = penalización con peso.
    pub hard: bool,
    #[serde(default = "one_f")]
    pub weight: f64,
    pub items: Vec<RuleItem>,
}

fn one_f() -> f64 {
    1.0
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectorKind {
    Any,
    DeskType,
    Role,
    Client,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatSelector {
    pub kind: SelectorKind,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    MainDoor,
    Exits,
    OfficeDoors,
    Windows,
    Columns,
    /// Obstáculo (oficina fija / zona) cuyo rótulo contiene `value`.
    ObstacleLabel,
    MainCorridor,
    /// Otros puestos que cumplan `seats`.
    Seats,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub kind: TargetKind,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub seats: Option<SeatSelector>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Predicate {
    /// Cada puesto de A a no más de `value` mm del objetivo.
    DistMax,
    /// Cada puesto de A a no menos de `value` mm del objetivo.
    DistMin,
    /// La persona mira hacia el objetivo (ángulo < 60°).
    FacingToward,
    /// La persona NO le da la espalda al objetivo dentro de `value` mm.
    NotBackTo,
    /// Los puestos de A deben quedar juntos (cada uno con otro de A a ≤ `value` mm).
    KeepTogether,
    /// Como máximo `value` puestos de A.
    CountMax,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub hard: bool,
    pub weight: f64,
    pub subject: SeatSelector,
    pub predicate: Predicate,
    pub target: Target,
    pub value: f64,
}

// ---------------------------------------------------------------------------
// Roles y clientes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Noise {
    #[default]
    Normal,
    /// Mucho teléfono / reuniones.
    Loud,
    /// Requiere concentración.
    Quiet,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearTarget {
    pub target: Target,
    pub max_dist: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Tipos de escritorio permitidos/preferidos (vacío = cualquiera).
    #[serde(default)]
    pub desk_types: Vec<String>,
    #[serde(default)]
    pub noise: Noise,
    #[serde(default)]
    pub near: Vec<NearTarget>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamCount {
    /// Vacío = sin cliente (personal transversal).
    #[serde(default)]
    pub client_id: String,
    pub role_id: String,
    pub count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Affinity {
    pub a: String,
    pub b: String,
    /// -2 (lejos) … +2 (cerca)
    pub value: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(default)]
    pub clients: Vec<Client>,
    #[serde(default)]
    pub teams: Vec<TeamCount>,
    #[serde(default)]
    pub affinities: Vec<Affinity>,
}

// ---------------------------------------------------------------------------
// Pesos (sliders)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Weights {
    /// Orden visual: pocas orientaciones, filas alineadas.
    pub order: f64,
    /// Luz natural (A4).
    pub daylight: f64,
    /// Ventana al costado del monitor (A1).
    pub window_side: f64,
    /// Privacidad: espaldas y pantallas (A2, A3).
    pub privacy: f64,
    /// Recorridos cortos a salidas.
    pub travel: f64,
    /// Cercanía a la puerta principal.
    pub compact: f64,
    /// Capacidad libre de reserva (A6).
    pub reserve: f64,
    /// Agrupación por roles y clientes (asignación).
    pub grouping: f64,
}

impl Default for Weights {
    fn default() -> Self {
        // foco: la mayor cantidad de puestos; solo un leve orden visual de desempate
        Self { order: 1.0, daylight: 0.0, window_side: 0.0, privacy: 0.0, travel: 0.0, compact: 0.0, reserve: 0.0, grouping: 6.0 }
    }
}

impl Weights {
    pub fn as_vec(&self) -> [f64; 8] {
        [self.order, self.daylight, self.window_side, self.privacy, self.travel, self.compact, self.reserve, self.grouping]
    }
    pub fn from_vec(v: [f64; 8]) -> Self {
        Self { order: v[0], daylight: v[1], window_side: v[2], privacy: v[3], travel: v[4], compact: v[5], reserve: v[6], grouping: v[7] }
    }
}

// ---------------------------------------------------------------------------
// Configuración completa de una corrida
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Tamaño de celda del raster de cálculo (mm). 0 = automático.
    #[serde(default)]
    pub resolution: f64,
    /// Tiempo de búsqueda (s).
    #[serde(default = "default_time")]
    pub time_limit: f64,
    #[serde(default)]
    pub seed: u64,
}

fn default_time() -> f64 {
    8.0
}

impl Default for Settings {
    fn default() -> Self {
        Self { resolution: 0.0, time_limit: 8.0, seed: 0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub catalog: Vec<DeskType>,
    pub demand: Vec<Demand>,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub custom_rules: Vec<CustomRule>,
    #[serde(default)]
    pub weights: Weights,
    #[serde(default)]
    pub org: Organization,
    #[serde(default)]
    pub settings: Settings,
    /// Módulos fijados por el usuario (se conservan tal cual).
    #[serde(default)]
    pub pinned: Vec<Module>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub plan: Plan,
    pub config: Config,
}

// ---------------------------------------------------------------------------
// Resultado
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    pub id: String,
    pub type_id: String,
    /// Centro del cuerpo del módulo.
    pub center: Pt,
    /// Ángulo de la fila en grados (eje largo del módulo).
    pub angle: f64,
    pub length: f64,
    pub depth: f64,
    pub per_side: u32,
    /// Para tipos de un lado: true si los puestos miran hacia +normal (lado "superior" local).
    pub seats_up: bool,
    /// Polígono del cuerpo (para dibujo y exportación).
    pub body: Vec<Pt>,
    #[serde(default)]
    pub pinned: bool,
    /// Personas bloqueadas en los puestos de este módulo (solo para módulos fijados).
    #[serde(default)]
    pub people: Vec<SeatPerson>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatPerson {
    /// Posición del puesto (para reconocerlo aunque cambie la numeración).
    pub pos: Pt,
    #[serde(default)]
    pub role_id: String,
    #[serde(default)]
    pub client_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seat {
    pub id: String,
    /// Número visible (001, 002…)
    pub number: u32,
    pub module_id: String,
    pub type_id: String,
    /// Posición del ocupante (centro de la silla).
    pub pos: Pt,
    /// Vector unitario hacia donde mira la persona (hacia el escritorio).
    pub facing: Pt,
    pub chair: Vec<Pt>,
    #[serde(default)]
    pub role_id: String,
    #[serde(default)]
    pub client_id: String,
    /// Avisos sobre este puesto (reglas blandas incumplidas).
    #[serde(default)]
    pub flags: Vec<String>,
    /// Puesto bloqueado: su escritorio está fijado y la persona no se reasigna.
    #[serde(default)]
    pub locked: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kpis {
    /// Puestos pedidos con cantidad fija (los de «máximo posible» no cuentan).
    pub seats_requested: u32,
    /// Hay tipos en «máximo posible».
    #[serde(default)]
    pub max_mode: bool,
    /// Puestos ubicados de los tipos en «máximo posible».
    #[serde(default)]
    pub max_seats: u32,
    pub seats_placed: u32,
    pub modules_requested: u32,
    pub modules_placed: u32,
    pub unplaced_by_type: Vec<(String, u32)>,
    pub area_m2: f64,
    pub m2_per_seat: f64,
    pub avg_travel_exit_m: f64,
    pub max_travel_exit_m: f64,
    pub daylight_pct: f64,
    pub window_side_pct: f64,
    pub privacy_issues: u32,
    pub orientations: u32,
    pub reserve_seats: u32,
    pub reserve_pct: f64,
    pub accessible_pmr: u32,
    pub hard_violations: u32,
    pub grouping_score: f64,
    pub misplaced_people: u32,
    pub corridor_area_m2: f64,
    pub score: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Violation {
    pub rule_id: String,
    pub message: String,
    #[serde(default)]
    pub at: Option<Pt>,
    pub hard: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub modules: Vec<Module>,
    pub seats: Vec<Seat>,
    /// Polígonos (contornos) del pasillo principal, para dibujo.
    pub corridors: Vec<Vec<Pt>>,
    /// Zonas de despeje (puertas, entrada, columnas…), para dibujo.
    pub keepouts: Vec<Vec<Pt>>,
    pub kpis: Kpis,
    pub violations: Vec<Violation>,
    /// Pesos con los que se obtuvo.
    pub weights: Weights,
    #[serde(default)]
    pub genome: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub rule_id: String,
    pub item_id: String,
    pub description: String,
    /// Nuevo valor propuesto (None = apagar la regla/sub-regla).
    pub new_value: Option<f64>,
    pub disable: bool,
    pub gain_seats: i32,
    pub seats_placed: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub layout: Layout,
    pub pareto_rank: u32,
}
