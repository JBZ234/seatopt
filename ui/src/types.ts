// Espejo de los tipos del núcleo en Rust (serde camelCase). Medidas en mm.

export interface Pt { x: number; y: number }

export type ObstacleKind = 'column' | 'fixedOffice' | 'restricted' | 'reserved';
export interface Obstacle { id: string; kind: ObstacleKind; poly: Pt[]; label: string; clear?: number[] }

export type DoorKind = 'main' | 'exit' | 'office' | 'ignore';
export interface Door { id: string; kind: DoorKind; a: Pt; b: Pt; label: string; clearDepth?: number | null; clearWidth?: number | null; flip?: boolean }
export interface Window { a: Pt; b: Pt }

export interface Room { poly: Pt[]; label: string }
export type PathKind = 'primary' | 'secondary';
/** Circulation path: centerline + clear width. Kept paths stay free of desks and chairs. */
export interface CircPath { id: string; kind: PathKind; from: string; to: string; pts: Pt[]; width?: number | null; keep: boolean; edited?: boolean }
export interface Plan { region: Pt[][]; obstacles: Obstacle[]; doors: Door[]; windows: Window[]; rooms?: Room[]; building?: Pt[]; regionClear?: number[][]; paths?: CircPath[] }

export type DeskShape = 'straight' | 'l' | 'custom';
/** Silla de un escritorio propio: posición local (mm, +y = lado principal) y hacia dónde mira. */
export interface CustomSeat { x: number; y: number; facing: Pt }
export interface DeskType {
  id: string; name: string; color: string; shape: DeskShape;
  seatWidth: number; depth: number; sides: number; perSide: number;
  flexible: boolean; minPerSide: number; maxPerSide: number;
  returnLength: number; returnDepth: number;
  /** Separaciones libres (mm): lado de la silla (null = regla E1), atrás, izquierda, derecha. */
  clearChair?: number | null; clearBack?: number; clearLeft?: number; clearRight?: number;
  /** Escritorio propio (Studio): contorno local y sillas. */
  body?: Pt[]; seats?: CustomSeat[];
}
export interface Demand { typeId: string; quantity: number; max?: boolean }

export interface RuleItem {
  id: string; label: string; enabled: boolean; value: number | null; unit: string;
  min: number; max: number; step: number; relaxTo: number | null; help: string;
}
export interface Rule {
  id: string; group: string; name: string; description: string;
  enabled: boolean; locked: boolean; hard: boolean; weight: number; items: RuleItem[];
}

export type SelectorKind = 'any' | 'deskType' | 'role' | 'client';
export interface SeatSelector { kind: SelectorKind; value: string }
export type TargetKind = 'mainDoor' | 'exits' | 'officeDoors' | 'windows' | 'columns' | 'obstacleLabel' | 'mainCorridor' | 'seats';
export interface Target { kind: TargetKind; value: string; seats?: SeatSelector | null }
export type Predicate = 'distMax' | 'distMin' | 'facingToward' | 'notBackTo' | 'keepTogether' | 'countMax';
export interface CustomRule {
  id: string; name: string; enabled: boolean; hard: boolean; weight: number;
  subject: SeatSelector; predicate: Predicate; target: Target; value: number;
}

export type Noise = 'normal' | 'loud' | 'quiet';
export interface NearTarget { target: Target; maxDist: number }
export interface Role { id: string; name: string; color: string; deskTypes: string[]; noise: Noise; near: NearTarget[] }
export interface Client { id: string; name: string; color: string }
export interface TeamCount { clientId: string; roleId: string; count: number }
export interface Affinity { a: string; b: string; value: number }
export interface Organization { roles: Role[]; clients: Client[]; teams: TeamCount[]; affinities: Affinity[] }

export interface Weights {
  order: number; daylight: number; windowSide: number; privacy: number;
  travel: number; compact: number; reserve: number; grouping: number; wall: number;
  /** Tidiness in seats: cost of a lone desk or an extra row direction (0 = seats only). */
  tidy?: number;
}
export interface Settings { resolution: number; timeLimit: number; seed: number; alternatives: number; resume?: unknown }

export interface Config {
  catalog: DeskType[]; demand: Demand[]; rules: Rule[]; customRules: CustomRule[];
  weights: Weights; org: Organization; settings: Settings; pinned: Module[];
}
export interface Problem { plan: Plan; config: Config }

export interface Module {
  id: string; typeId: string; center: Pt; angle: number; length: number; depth: number;
  perSide: number; seatsUp: boolean; body: Pt[]; pinned: boolean; people?: SeatPerson[];
}
export interface SeatPerson { pos: Pt; roleId: string; clientId: string }
export interface Seat {
  id: string; number: number; moduleId: string; typeId: string; pos: Pt; facing: Pt; chair: Pt[];
  roleId: string; clientId: string; flags: string[]; locked?: boolean;
}
export interface Kpis {
  maxMode?: boolean; maxSeats: number;
  seatsRequested: number; seatsPlaced: number; modulesRequested: number; modulesPlaced: number;
  unplacedByType: [string, number][]; areaM2: number; m2PerSeat: number;
  avgTravelExitM: number; maxTravelExitM: number; daylightPct: number; windowSidePct: number;
  privacyIssues: number; orientations: number; reserveSeats: number; reservePct: number;
  accessiblePmr: number; hardViolations: number; groupingScore: number; misplacedPeople: number;
  corridorAreaM2: number; score: number;
}
export interface Violation { ruleId: string; message: string; at?: Pt | null; hard: boolean }
export interface Layout {
  modules: Module[]; seats: Seat[]; corridors: Pt[][]; keepouts: Pt[][];
  kpis: Kpis; violations: Violation[]; weights: Weights; genome?: unknown;
  corridorLines?: Pt[][]; corridorWidth?: number; accessPaths?: Pt[][]; alternatives?: Layout[];
}
export interface Suggestion {
  ruleId: string; itemId: string; description: string; newValue: number | null;
  disable: boolean; gainSeats: number; seatsPlaced: number; layout?: Layout | null;
}
export interface Variant { layout: Layout; paretoRank: number }
export interface Capacity { typeId: string; maxSeats: number; maxUnits: number }

export type LayerRole =
  | 'outline' | 'wall' | 'column' | 'office' | 'restricted' | 'reserved'
  | 'door' | 'mainDoor' | 'exitDoor' | 'officeDoor' | 'window' | 'furniture' | 'room' | 'building' | 'ignore';
export interface LayerOut { name: string; color: string; paths: number; role: LayerRole }
export interface ScaleGuess { mmPerUnit: number; paperScale: number | null; reason: string }
export interface ImportOut {
  drawingId: string; name: string; source: 'dxf' | 'dwg' | 'pdf'; layers: LayerOut[];
  scale: ScaleGuess; bounds: [number, number, number, number]; unitNote: string;
  /** mm per drawing unit as the file declares it (null = not declared). */
  declaredMm?: number | null;
}
export interface BgLayer { name: string; color: string; paths: number[][]; /** [x, y, height, rotation°, text] in mm */ texts?: [number, number, number, number, string][] }
export interface AnalyzeOptions { mmPerUnit: number; roles: Record<string, LayerRole>; seed?: Pt | null; gapClose: number }
export interface AnalyzeResult { plan: Plan; notes: string[]; areaM2: number }

export interface Progress {
  phase: string; fraction: number; bestScore: number; seatsPlaced: number;
  seatsRequested: number; iterations: number; message: string;
}
export interface JobState<T = unknown> { done: boolean; progress: Progress; result?: T | null; error?: string | null }
export type JobKind = 'optimize' | 'explore' | 'capacity' | 'suggest';

export interface Defaults { catalog: DeskType[]; rules: Rule[]; weights: Weights; settings: Settings }
