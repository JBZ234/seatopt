# Seats — office layout optimizer

Desktop app (Tauri 2 + Rust + React), also available as a web page, that takes an office floor plan as **DXF, DWG or vector PDF** and works out **how many seats fit** given the desk geometry, the space people need to sit and walk, the circulation paths and the doors. It can then seat people by **role** and **client**.

```
Plan ──▶ Circulation ──▶ Desks ──▶ Rules ──▶ Organization ──▶ Optimize
(layers,  (paths from     (numbers   (spaces,   (roles, clients,   (result,
 scale,    the entrance    or "as     priority,  optional)          suggestions,
 area)     to the doors)   many as    style)                        export)
                           fit")
```

## Install and build on Windows

1. Requirements (once):
   - [Rust](https://rustup.rs) (MSVC toolchain).
   - **Visual Studio Build Tools** with the "Desktop development with C++" workload.
   - [Node.js](https://nodejs.org) 18 or later.
   - WebView2, already included in Windows 10 and 11.
2. In the project folder:
   ```powershell
   npm install            # installs the Tauri CLI and the interface dependencies
   npm run tauri dev      # opens the app in development mode
   npm run tauri build    # builds the installer (.msi / .exe) in target/release/bundle/
   ```
3. To open **DWG** files install [ODA File Converter](https://www.opendesign.com/guestfiles/oda_file_converter) (free). The app looks for it in `C:\Program Files\ODA\…`; if it is elsewhere, enter the path in *Plan → Settings*. LibreDWG (`dwg2dxf`) also works.

### Use it in the browser with the dev server

Handy for quick tests or sharing on the local network.

```bash
npm --prefix ui install && npm --prefix ui run build
cargo run --release -p seatopt-devserver -- 8787 ui/dist   # open http://127.0.0.1:8787
```

For hot reload while working on the interface, keep the server running and run `npm --prefix ui run dev` (http://localhost:1420).

## Web version (GitHub Pages)

Besides the desktop app there is a web version that opens from a link, with nothing to install.
It uses **the same engine**: `seat_core` compiled to WebAssembly (`crates/web`), with threads (rayon
on Web Workers, one per core), so it gives the same results as the desktop app. The plan is
processed in the browser and never uploaded. The desktop app is unchanged: it keeps using the native
engine.

Differences of the web version:
- It opens DXF and vector PDF; save DWG files as DXF first.
- Save and export download the file (there is no folder dialog).
- Threads need cross-origin isolation; GitHub Pages does not allow custom headers, so
  `coi-serviceworker` adds them (the first visit reloads the page once). If the browser does not
  allow it, the engine runs on a single thread: same result, slower.

**Publish:** push the project to a GitHub repository, open *Settings → Pages* and under *Source*
choose **GitHub Actions**. Every push to `main` builds and publishes (`.github/workflows/pages.yml`).
The site is at `https://<user>.github.io/<repository>/`.

**Try it locally** (Linux/macOS or WSL; needs `rustup component add rust-src`,
`rustup target add wasm32-unknown-unknown` and `cargo install wasm-bindgen-cli --version 0.2.129 --locked`):

```bash
bash scripts/build-wasm.sh       # engine → ui/src/wasm (once, or whenever the engine changes)
npm --prefix ui run dev:web      # http://localhost:1420, with threads
npm --prefix ui run build:web    # static site in ui/dist-web
```

## How to use it

The app opens on a simple screen to **choose the plan** (or a saved project). **Close plan**, top right, goes back to it.

1. **Plan** — the plan opens **as it is in the file, with nothing detected**: you draw the areas, doors and obstacles over it. **Draw without a plan** starts on a blank grid.
   - **Scale (like AutoCAD):** *Units* (mm, cm, m, in, ft; the ones the file declares) and *Scale 1:N* (1:1 when the plan is drawn at real size). If the doors suggest other units, the app offers them (*Use meters 1:1*) but never changes them by itself. **Calibrate with a known length:** two clicks on something you know. Changing it keeps what you drew on the same lines. PDFs use the paper scale.
   - **The drawing:** every layer shows, with its texts, dimensions and leaders, without simplifying anything. In *Drawing layers* each layer has a switch (All / None). With Select, click a line or drag a window over several and press **H** (or Delete) to hide them; *Show hidden lines* brings them back. The file never changes.
   - **Grid, units and zoom:** the grid is decimal (lines every 1 mm, 1 cm, 10 cm, 1 m… as you zoom, heavier every 10); the snap step is chosen apart. Coordinates, lengths and what you type use the units chosen at the bottom right (m, cm or mm). Zoom goes from 1 mm = 10 px out to whole kilometers.
   - **Draw by hand (like AutoCAD):**
     - **Areas to furnish:** *Draw area* (click the corners) or *Rectangle*. Points snap to corners (square) and edges (×) of the drawing; **Ortho** keeps lines horizontal or vertical (F8 toggles it, Shift does the opposite while held). Type a length in meters and Enter (`3.5`, `3,5`), a length and angle (`3.5<90`), an offset (`2x1.5`) or, for a rectangle, width x height (`4x3`). Enter, right-click, double-click, `C` or a click on the first point closes it; Backspace removes the last point; Esc cancels. Outlines that overlap are joined; areas that only touch stay separate. *Edit* selects an area: drag its corners, double-click an edge to add a corner, click a corner and Delete to remove it.
     - **Doors:** *Main entrance*, *Door*, *Exit*: click both ends of the opening (or one end and type the width). They snap onto the edge of the areas and stay along that wall.
     - **Obstacles (optional):** column, fixed office or room, panel or cabinet, reserved area; as a rectangle or an outline.
   - **Detect automatically** (a button, never on its own): the area is found from the walls (sheet frames are ignored). Click inside it, add or remove rooms, and confirm the main entrance the app picked. *Fix it by hand* goes back to drawing with what was detected.
   - **Editing:** with Select, press on a door, obstacle or area and drag it. *Flip side* (F) / *Flip hinge* for doors (their swing is drawn), *Mirror* for obstacles, *Rotate 90°* (R, Shift+R) or *Rotate…* any angle; arrows nudge 10 cm (Shift 50 cm). Areas without a door are furnished too (a warning only).
   - **Check:** a live list of problems (the outline crosses itself, areas overlap, no main entrance or it is not on an area, an area nobody can get into, doors off the areas, obstacles outside…). *Show* zooms to each one. **Continue** (and the other steps) unlock when there are no errors; warnings do not block.
   - Optional settings: layer roles and **Export base plan**.
2. **Circulation** — walking routes, drawn before the desks.
   - The app calculates a path from the main entrance to **every office door and exit**. They run along walls and obstacles like a real corridor, with straight horizontal and vertical runs, and paths that run almost side by side are merged into one band.
   - **Office-to-office paths** (toggle) are secondary: each office door is linked to its two nearest ones. They start unchecked; check the ones the optimizer must keep.
   - **Checked paths are kept clear:** no desk or chair goes on them. Unchecked ones are only drawn.
   - **Edit a path:** click it on the plan (or in the list) to see its nodes. Drag a **node** to move it (it snaps to line up with its neighbours), drag the **bar** in the middle of a run to shift the whole run sideways, double-click a run to add a node, select a node and press **Delete** to remove it. The ends slide along their door.
   - **Widths:** a general width for entrance paths (1.20 m) and for office-to-office paths (0.90 m), and an optional width per path. **Recalculate** keeps the paths you edited by hand.
3. **Desks**
   - For the types you know the number of (for example 5 L desks and 5 pairs), type the number: they are placed first, always.
   - For the others turn on **As many as fit**: the optimizer places all that fit.
   - If you turn it on for a **flexible** type (island or bench, built to whatever length is needed) and a **single** one (individual or pair), the flexible one fills the large areas and the single one fills the small gaps (a filler seat is worth 0.7 of a seat in a row).
   - The bin icon next to the name **deletes the type** (and its desks on the plan).
   - **Estimate capacity** works out how many of each type fit on their own.
   - **Place by hand:** click on the plan to place desks of a type (R rotates 90°, F flips the chair side). They stay pinned: they count toward the numbers and the optimization places the rest around them.
   - Under *Dimensions* of each type, **free space around**: chair side (by default the one in *Rules*), back, left and right.
   - **Studio:** design your own desk from rectangles, with chairs and clearances.
4. **Rules** — simple cards with a small drawing each.
   - **Priority:** *Most seats* (every seat counts, desks go into any gap), *Balanced* (default: a lone desk or an extra row direction must be worth more than one seat) or *Tidy* (clean rows first, may give up a few seats).
   - **Space for people:** chair space, space between rows (chairs back to back, chair to the back of a desk) and the narrowest gap between desks.
   - **Clearances:** walls (0.20 m by default; each wall can have its own value), columns (0.20 m), fixed offices (0.30 m), doors (leaf swing + 0.60 m) and an optional entrance lobby.
   - **Circulation:** path widths and whether the optimizer keeps them clear; optional main corridor.
   - **Layout style:** one direction for all rows, break long rows, desks against walls, row order.
   - **More rules:** the full catalog (egress distances, daylight, privacy, density, organization…), off by default.
   - **Find values that gain seats** tries each measure and shows the gain next to it, with **Apply**.
   - Always true: every seat can be reached (no seat boxed in) and no door is blocked.
5. **Organization** (optional): roles with color, desk type, noise level and an element they should be near; clients; how many people of each role each client has; affinity between roles.
6. **Optimize**
   - The search **stops by itself** when a while passes without improvements (more patience on large plans or with few cores; 2 min cap). **Keep searching** continues from the layout you are looking at.
   - After building the rows, a **gap filler** looks for more desks in both directions, preferring those with their back to a wall or back to back; each one is checked against all the rules.
   - **Add desks by hand** (after optimizing): choose the type and move it over the plan; green means it fits, red tells you what it breaks. When you drop it the engine also checks access and warns you if something fails (Ctrl+Z removes it).
   - **Best layouts found:** the N best different layouts (10 by default) with a preview; click to see one on the plan.
   - **Find better values:** lowers each measure to its reasonable minimum and tries other desk types; each try is a full optimization and can be **Used** directly.
   - **Lock seats:** select desks and use "Pin and lock". In the next optimization they stay in place with their people (role and client) and the rest is optimized.
   - **Export:**
     - a DXF in the coordinates of the original plan, to insert as an external reference;
     - a **scaled vector PDF** (A4 to A0, automatic scale or 1:N) with the plan, circulation paths, numbered seats, colors by type/role/client, legend and indicators;
     - a CSV with number, type, role, client, position and warnings of each seat;
     - the `.seats.json` project (older `.puestos.json` projects still open).

### Editing and selection like AutoCAD

| Action | How |
|---|---|
| Window selection (blue) | Drag **left to right**: takes what is fully inside |
| Crossing selection (green) | Drag **right to left**: takes everything it touches |
| Add / remove | A new selection replaces the previous one; **Ctrl** + click or drag adds, **Shift** + click or drag removes |
| Move | Drag the selection, or arrow keys (100 mm; Shift = 500 mm) |
| Rotate 90° | **R**: selected desks or obstacles, about their center |
| Flip a door | "Flip swing" button with the door selected |
| Pin and lock people | **P** or the selection bar button |
| Delete | **Delete** |
| Undo / redo | **Ctrl+Z** / **Ctrl+Y** (or Ctrl+Shift+Z) |
| Select all desks | **Ctrl+A** |
| Clear selection / leave tool | **Esc** |
| Pan / zoom | Right or middle button + drag / wheel |

Whatever you move or rotate by hand stays pinned. After each edit the app recalculates the warnings (overlaps, access, travel) without moving anything.

### Base plan (simplified)

*Plan → Export base plan* generates the minimal plan for optimizing, as DXF and scaled PDF:

- **Content:** outline of the usable area, offices and rooms, columns, areas with clearance and doors (leaf + swing).
- **Standard layers:** `A-MURO-EXT`, `A-OFICINA-FIJA`, `A-RECINTOS`, `A-EDIFICIO`, `A-COLUMNA`, `A-PUERTA-PPAL`, `A-PUERTA-SALIDA`, `A-PUERTA-OFICINA`, `A-PUERTA-VANO`.
- **Scale:** the PDF keeps layers and scale, so importing it again needs no layer setup or calibration.

You can also export:

- **Single lines:** the original drawing reduced to the essentials. Each wall (double line, facade or glass) becomes **one centerline**; plus columns, stairs and elevators as blocks, doors (leaf + arc) and office labels. Layers `MUROS`, `COLUMNAS`, `NUCLEO`, `PUERTAS`, `A-PUERTA-PPAL`, `TEXTO`. It bounds the same area as the original, so it can be imported again.
- The full original drawing with only the "Existing furniture" layers removed.

In *Plan → Drawing layers* the bin icon **deletes all the lines and objects of that layer** (if the layer affected the area, it is detected again).

### Clearances you can edit on the plan

Click the **area outline**: a gold dimension per edge shows its clearance inward (the general wall clearance by default); curved stretches drawn with many short edges share one value.

Click an **office, column or area**: its clearance shows in gold with one dimension per side (or a single one if it is not rectangular). Click the number to type the value in meters (Enter), or drag the gold line. The same works for a **door** (free depth and width) and a **desk** (chair, back, left, right; it applies to the whole type).

### Grid

Bottom right: show the grid, **Snap** and its size.

- With Snap the cursor snaps to the grid. When drawing, placing or moving it also snaps, first, to **corners** (square) and **edges** (×) of the area, offices, columns and drawing lines; on an edge it stays on the nearest grid line.
- **Hold the space bar** to release snapping for a moment. **Ctrl+Space** turns Snap on or off.

### Theme

Top right: color palette and Light, Dark or System (follows the operating system). It collapses and unfolds when you hover it.

## Recommended layer names

If you draw your plans with these layers the import is direct (the plans in `samples/` use them). With other names you assign them by hand once.

| Layer | Content |
|---|---|
| `A-MURO-EXT` | Closed polyline of the area |
| `A-COLUMNA` | Columns (closed) |
| `A-OFICINA-FIJA` | Offices, meeting rooms, toilets, cores |
| `A-PUERTA-PPAL` / `A-PUERTA-SALIDA` / `A-PUERTA-OFICINA` | Opening + leaf + swing arc |
| `A-VENTANA` | Facade stretches with windows |
| `A-ZONA-RESTRINGIDA` | Electrical panels, fire cabinets |

PDFs exported from AutoCAD keep their layers (Optional Content Groups) and the app reads them just like a DXF.

## How the optimizer works

It is a *test-fit* / generative design approach, like the commercial space-planning tools.

1. **Floor raster** (50–100 mm per cell): free area, obstacles, door clearances, entrance lobby, buffers around columns, panels and offices, and the **kept circulation paths** as clear bands.
2. **Circulation first.** Paths are routed on the empty floor with Dijkstra (preferring to run along walls at half their width), then drawn with orthogonal runs and merged where they run side by side. An optional main corridor can be added too.
3. **Row filling.** Each zone is filled with parallel rows at a candidate angle (the dominant wall angles, 45° included).
   - In each row the desks are grouped in blocks of up to *E3* seats per side, separated by a cross gap.
   - Between rows *E2* is respected (chair to chair, chair to back, or backs together).
   - Checks are O(1) per column thanks to precomputed "vertical runs".
4. **Validation on the furnished floor.**
   - No seat may be boxed in: every chair is reached through gaps at least the narrowest-gap width (chairs move); comfortable aisles are preferred.
   - Optional egress rules: maximum travel to an exit, common travel, wheelchair-accessible seats; doors must not be blocked.
   - A desk with an invalid seat is discarded.
5. **Lexicographic score.** `100,000 × fixed seats not placed + 10,000 × "as many as fit" seats missing (7,000 for filler) + 1,000,000 × hard rules broken + tidiness (in seats: lone desks and extra row directions, set by the priority) + soft metrics`. Placing the fixed desks always dominates; then more seats; with the same seats, tidier layouts with fewer separate desks.
6. **Search.**
   - Parallel simulated annealing over the "genome": angle, offset, start side, alignment, pairing and type order per zone, plus the corridor variant.
   - It starts with a cheap multi-start that uses only the decoder.
   - It caches layouts already evaluated.
7. **Seating people.** Annealing by swaps with unary costs (role ↔ desk type, role near an element, custom rules) and pairwise costs (client blocks, affinity, acoustic separation).

## Engine rules

The *Rules* screen shows the rules that decide capacity and look. The engine keeps the full catalog (saved in the project), but by default only circulation paths, chair space, rows, narrowest gap, walls, columns, offices, doors and reserved areas are on; egress distances, daylight, privacy and density are off.

| ID | Rule | ID | Rule |
|---|---|---|---|
| P1 | Circulation paths (widths; kept clear) | E4 | Maximum distance from chair to aisle |
| C1 | Main corridor (width, exits, offices, straight runs) | E5 | Do not mix types in a bench |
| C2 | Narrowest gap between desks (required) and preferred aisle width | E6 | Allowed directions (slanted walls) |
| C3 | Entrance lobby | E7 | Same direction for all rows |
| C4 | Door clearance (swing + space in front) | E8 | Desks against walls (clearance to walls) |
| C5 | Maximum travel to an exit 🔒 | O1 | Clearance to columns |
| C6 | Common path of travel 🔒 | O2 | Panel and cabinet clearance 🔒 |
| C7 | No seat boxed in, no door blocked 🔒 | O3 | Clearance to fixed offices |
| C8 | Wheelchair-accessible seats | O4 | Reserved areas |
| E1 | Chair space | A1–A6 | Window beside the monitor, backs, screens, daylight, density, reserve |
| E2 | Space between rows | R1–R5 | Role↔type, client blocks, affinity, nearness, acoustics |
| E3 | Maximum seats in a row | | |

The default values are typical references. **Check the egress values against your local building code.**

## Structure

```
crates/core        Rust core (no interface)
  import/            DXF (blocks, bulges, splines), PDF with OCG layers, DWG through a converter
  analyze.rs         layer roles, scale, area by flood fill, doors, windows
  engine/            ctx (raster, corridors, zones), decode (rows), eval (validation and score),
                     search (annealing, exploration, capacity, suggestions), custom (own rules),
                     circulation (paths to the doors), fill (gap filler)
  assign.rs          roles and clients
  export.rs          DXF R12 and CSV
  pdf_out.rs         scaled vector PDF and DXF without furniture
  simple.rs          single-line plan (wall centerlines by thinning)
  api.rs             JSON facade + background jobs
crates/devserver   HTTP server with the same API (browser mode)
crates/web         the same core compiled to WebAssembly (web version)
scripts/           build-wasm.sh: builds crates/web in two variants (threads and single thread)
.github/workflows  pages.yml: publishes the web version on GitHub Pages
src-tauri          desktop app
ui                 React + TypeScript interface
samples            sample plans (DXF, PDF) and their generator
```

Tests: `cargo test --release -p seat_core` (sample plans; tests that need a private real plan are skipped when it is missing).
Console tools: `cargo run --release -p seat_core --example run -- plan.dxf out.svg isla_flex=40,recto_120=10 8` and `--example tidy` (compares the priorities on the samples).

## Known limitations

- **Text in PDFs.** In AutoCAD PDFs text usually comes as strokes, so labels (OF 08, ROOM…) are not read. In DXF they are.
- **Main entrance.** Without a main-entrance layer the widest door is marked, which may not be the entrance; check it.
- **Main corridor.** It is routed at 0°, 45° and 90° to the drawing grid. In plans rotated by an angle that is not a multiple of 45° it comes out stepped (desk rows still follow the wall angle).
- **Speed.** Search time scales with the processor cores. With 8 cores, 10–20 s are enough for a ~800 m² floor.
