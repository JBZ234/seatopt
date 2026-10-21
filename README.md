# Optimizador de Puestos

App de escritorio (Tauri 2 + Rust + React) que recibe la planta de una oficina en **DXF, DWG o PDF vectorial** y calcula **cuántos puestos caben como máximo** según la geometría de los escritorios, los espacios para caminar y las entradas. Después sienta a las personas por **rol** y **cliente**.

```
Plano ──▶ Escritorios ──▶ Espacios ──▶ Organización ──▶ Optimizar
(capas,    (cantidades o    (pasillos,    (roles, clientes,   (resultado,
 escala,    «máximo          sillas,       opcional)           sugerencias,
 recinto)   posible»)        puertas)                          exportar)
```

## Instalar y compilar en Windows

1. Requisitos (una sola vez):
   - [Rust](https://rustup.rs) (toolchain MSVC).
   - **Visual Studio Build Tools**, con la carga de trabajo «Desarrollo para el escritorio con C++».
   - [Node.js](https://nodejs.org) 18 o superior.
   - WebView2, que ya viene en Windows 10 y 11.
2. En la carpeta del proyecto:
   ```powershell
   npm install            # instala la CLI de Tauri y las dependencias de la interfaz
   npm run tauri dev      # abre la app en modo desarrollo
   npm run tauri build    # genera el instalador (.msi / .exe) en target/release/bundle/
   ```
3. Para abrir **DWG**, instala [ODA File Converter](https://www.opendesign.com/guestfiles/oda_file_converter), que es gratuito. La app lo busca en `C:\Program Files\ODA\…`. Si está en otra ruta, indícala en *Plano → Configuración*. LibreDWG (`dwg2dxf`) también sirve.

### Usarla en el navegador, sin Tauri

Sirve para probar rápido o para compartir en la red local.

```bash
npm --prefix ui install && npm --prefix ui run build
cargo run --release -p seatopt-devserver -- 8787 ui/dist   # abre http://127.0.0.1:8787
```

Para desarrollar la interfaz con recarga en caliente, deja el servidor corriendo y ejecuta `npm --prefix ui run dev` (http://localhost:1420).

## Versión web (GitHub Pages)

Además de la app de escritorio hay una versión web que se abre con un enlace, sin instalar nada.
Usa **el mismo motor**: `seat_core` compilado a WebAssembly (`crates/web`), con hilos (rayon sobre
Web Workers, uno por núcleo), así que da los mismos resultados que la app de escritorio. El plano se
procesa en el navegador y no se sube a ningún lado. La app de escritorio no cambia: sigue usando el
motor nativo.

Diferencias de la versión web:
- Abre DXF y PDF vectorial; los DWG hay que guardarlos antes como DXF.
- Guardar y exportar descargan el archivo (no hay diálogo de carpeta).
- Los hilos necesitan aislamiento de origen; GitHub Pages no deja poner cabeceras, así que
  `coi-serviceworker` las agrega (la primera visita recarga la página una vez). Si el navegador no
  lo permite, el motor corre en un solo hilo: mismo resultado, más lento.

**Publicar:** sube el proyecto a un repositorio de GitHub, entra a *Settings → Pages* y en *Source*
elige **GitHub Actions**. Cada `push` a `main` compila y publica (`.github/workflows/pages.yml`).
El enlace queda en `https://<usuario>.github.io/<repositorio>/`.

**Probar en local** (Linux/macOS o WSL; necesita `rustup component add rust-src`,
`rustup target add wasm32-unknown-unknown` y `cargo install wasm-bindgen-cli --version 0.2.129 --locked`):

```bash
bash scripts/build-wasm.sh       # motor → ui/src/wasm (una vez, o cuando cambie el motor)
npm --prefix ui run dev:web      # http://localhost:1420, con hilos
npm --prefix ui run build:web    # sitio estático en ui/dist-web
```

## Flujo de uso

Al abrir la app aparece una pantalla simple para **seleccionar el plano** (o un proyecto guardado). Arriba a la derecha, **Cerrar plano** vuelve a esa pantalla.

1. **Plano**: la app te guía con tres tareas, cada una con su estado. La tarea pendiente activa sola su herramienta y muestra la instrucción sobre el plano.
   - **Escala:** se toma del archivo (unidades del DXF o la escala guardada en PDF exportados por la app) o se estima por el tamaño de las puertas. **Calibrar con una medida:** dos clics sobre algo que conozcas.
   - **Área a amoblar:** se detecta sola (ignora el marco de la hoja). Si no queda bien, **clic dentro del área**, o **Dibujar contorno**, con enganche a los vértices del dibujo.
   - **Puerta principal:** si la app eligió la más ancha, pide confirmar. **Marcar en el plano** o **Dibujar puerta** si no se detectó.
   - Ajustes opcionales: salidas, obstáculos y zonas, roles de capas y **Exportar plano base**.
2. **Escritorios**
   - Para los tipos que ya sabes cuántos van (por ejemplo 5 en L y 5 parejas), escribe la cantidad: se ubican primero y siempre.
   - Para los demás activa **Máximo posible**: el optimizador mete todos los que quepan.
   - Si marcas un tipo **flexible** (isla o bench, que se arma del largo que haga falta) y uno **suelto** (individual o pareja), el flexible llena las áreas grandes y el suelto rellena los huecos pequeños (un puesto de relleno vale 0,7 de uno en fila).
   - El ícono de papelera junto al nombre **borra el tipo** (y sus escritorios en el plano).
   - **Estimar capacidad** calcula cuántos caben de cada tipo por separado.
   - **Colocar a mano**: clic en el plano para poner escritorios de un tipo (R gira 90°, F cambia el lado de la silla). Quedan fijados: cuentan dentro de la cantidad y la optimización ubica el resto alrededor.
   - En *Medidas* de cada tipo, **separación libre alrededor**: lado de la silla (por defecto la de *Espacios*), atrás, izquierda y derecha.
3. **Espacios**: solo las medidas que deciden la capacidad, cada una con un dibujo: pasillo principal, pasillos hacia los puestos, espacio para la silla, separación entre filas, corte de filas largas, entrada libre, frente de las puertas y separación a columnas y oficinas.
   - Siempre se cumple que todos los puestos se alcancen desde la entrada y que ninguna puerta quede bloqueada.
   - **Calcular cuántos puestos gana cada medida** prueba cada ajuste y muestra la ganancia junto a la medida, con **Aplicar**.
4. **Organización** (opcional): roles con color, tipo de escritorio, nivel de ruido y elemento al que deben estar cerca; clientes; matriz de cuántas personas de cada rol tiene cada cliente; afinidad entre roles.
5. **Optimizar**
   - La búsqueda **se detiene sola** cuando pasa un rato sin mejoras (más paciencia en planos grandes o con pocos núcleos; tope de 2 min). **Seguir buscando** continúa desde la solución que estás viendo.
   - Controles: **orden de las filas** (0 = meter escritorios en cualquier hueco; alto = solo filas ordenadas y escritorios apoyados) y **pegar a los muros**.
   - Presiona **Optimizar**. Después de armar las filas, un **relleno de huecos** busca dónde caben más escritorios en las dos orientaciones, prefiriendo los que quedan con la espalda contra un muro o espalda con espalda; cada uno se valida con todas las reglas.
   - **Mostrar**: pasillo principal (franja suave con su eje), **recorridos a oficinas** (camino de referencia desde la entrada hasta cada puerta de oficina por el piso ya amoblado; solo se muestra, no cambia la optimización) y despejes de puertas.
   - **Agregar a mano** (después de optimizar): eliges el tipo y lo mueves por el plano; en verde cabe, en rojo dice qué incumple (recinto, oficinas, columnas y sus separaciones, despejes de puertas, pasillo, otros escritorios). Al soltarlo el motor revisa también el acceso; si algo falla lo avisa (Ctrl+Z lo quita).
   - **Mejores soluciones encontradas**: las N mejores distintas (10 por defecto) con vista previa; clic para verla en el plano.
   - **Buscar mejores valores**: prueba bajar cada medida de *Espacios* a su mínimo razonable y cambiar de tipo; cada prueba es una optimización completa y se puede **Usar** directamente.
   - Si no caben los fijos, la app prueba relajar cada medida y cambiar de tipo de escritorio, y muestra cuántos puestos gana cada cambio, con el botón **Aplicar**.
   - Con equipos cargados en *Organización* aparece un único control: cuánto pesa sentar a cada equipo junto.
   - **Bloquear puestos:** selecciona escritorios y usa «Fijar y bloquear». En la próxima optimización se conservan en su sitio con sus personas (rol y cliente) y se optimiza el resto.
   - **Exportar** genera:
     - un DXF en las coordenadas del plano original, para insertarlo como referencia externa;
     - un **PDF vectorial a escala** (A4 a A0, escala automática o 1:N) con plano, puestos numerados, colores por tipo/rol/cliente, convenciones e indicadores;
     - un CSV con número, tipo, rol, cliente, posición y avisos de cada puesto;
     - el proyecto `.puestos.json`.

### Edición y selección como en AutoCAD

| Acción | Cómo |
|---|---|
| Selección por ventana (azul) | Arrastrar de **izquierda a derecha**: toma lo que queda completamente adentro |
| Selección por captura (verde) | Arrastrar de **derecha a izquierda**: toma todo lo que toque |
| Agregar / quitar | Una selección nueva reemplaza la anterior; **Ctrl** + clic o barrido suma, **Shift** + clic o barrido quita |
| Mover | Arrastrar lo seleccionado, o flechas (100 mm; Shift = 500 mm) |
| Girar 90° | **R**: escritorios u obstáculos seleccionados, sobre su centro |
| Invertir puerta | Botón «Invertir sentido» con la puerta seleccionada |
| Fijar y bloquear personas | **P** o botón de la barra de selección |
| Borrar | **Supr** |
| Deshacer / rehacer | **Ctrl+Z** / **Ctrl+Y** (o Ctrl+Shift+Z) |
| Seleccionar todos los escritorios | **Ctrl+A** |
| Limpiar selección / salir de herramienta | **Esc** |
| Desplazar / zoom | Botón derecho o central + arrastrar / rueda |

Lo que se mueve o gira a mano queda fijado. Después de cada edición la app recalcula los avisos (superposiciones, accesos, recorridos) sin mover nada.

### Plano base (simplificado)

En *Plano → Exportar plano base* se genera el plano mínimo para optimizar, en DXF y PDF a escala:

- **Contenido:** contorno del área amoblable, oficinas y recintos, columnas, zonas con despeje y puertas (hoja + barrido).
- **Capas estándar:** `A-MURO-EXT`, `A-OFICINA-FIJA`, `A-RECINTOS`, `A-EDIFICIO`, `A-COLUMNA`, `A-PUERTA-PPAL`, `A-PUERTA-SALIDA`, `A-PUERTA-OFICINA`, `A-PUERTA-VANO`.
- **Escala:** el PDF guarda capas y escala, así que reimportarlo no pide asignar capas ni calibrar.

También se puede exportar:

- **Líneas simples:** el dibujo original reducido a lo esencial. Cada muro (doble línea, fachada o vidrio) queda como **una sola línea** a eje; además columnas, escaleras y ascensores como bloque, puertas (hoja + arco) y el rótulo de las oficinas. Capas `MUROS`, `COLUMNAS`, `NUCLEO`, `PUERTAS`, `A-PUERTA-PPAL`, `TEXTO`. Delimita la misma área que el original, así que también sirve para reimportar.
- El dibujo original completo quitando solo las capas de «Mobiliario existente».

En *Plano → Capas del dibujo*, el ícono de papelera **borra todas las líneas y objetos de esa capa** (si la capa afectaba el área, se vuelve a detectar).

### Separaciones editables en el plano

Clic sobre el **borde del recinto**: aparece una cota dorada por arista con la separación hacia adentro (0 por defecto); los tramos curvos, dibujados con muchas aristas cortas, comparten una sola medida.

Clic sobre una **oficina, columna o zona**: su separación aparece en dorado con una cota por lado (o una sola si no es rectangular). Clic en el número para escribir la medida en metros (Enter). Lo mismo con una **puerta** (fondo y ancho libres) y con un **escritorio** (silla, atrás, izquierda, derecha; se aplica a todo su tipo).

### Cuadrícula

Abajo a la derecha: mostrar la cuadrícula, **Snap** y su tamaño.

- Con Snap, el cursor se engancha a la cuadrícula. Al dibujar, colocar o mover también se engancha, con prioridad, a **vértices** (cuadrado) y **bordes** (×) del recinto, oficinas, columnas y líneas del dibujo; sobre un borde se queda en la línea de cuadrícula más cercana.
- **Barra espaciadora sostenida**: sin enganche mientras la mantienes. **Ctrl+Espacio**: activa o desactiva el Snap.

### Tema

Arriba a la derecha: Claro, Oscuro o Sistema (sigue la configuración de Windows).

## Convención de capas recomendada

Si dibujas tus plantas con estas capas, la importación es directa (los planos de `samples/` las usan). Con otros nombres se asignan a mano una sola vez.

| Capa | Contenido |
|---|---|
| `A-MURO-EXT` | Polilínea cerrada del recinto |
| `A-COLUMNA` | Columnas (cerradas) |
| `A-OFICINA-FIJA` | Oficinas, salas, baños, núcleos |
| `A-PUERTA-PPAL` / `A-PUERTA-SALIDA` / `A-PUERTA-OFICINA` | Vano + hoja + arco de giro |
| `A-VENTANA` | Tramos de fachada con ventana |
| `A-ZONA-RESTRINGIDA` | Tableros, gabinetes contra incendio |

Los PDF exportados desde AutoCAD conservan las capas (Optional Content Groups) y la app las lee igual que en un DXF.

## Cómo funciona el optimizador

Es un enfoque de *test-fit* / diseño generativo, como el de las herramientas comerciales de planeación de espacios.

1. **Raster del piso** (50–100 mm por celda): área libre, obstáculos, despejes de puertas, vestíbulo de entrada y buffers alrededor de columnas, tableros y oficinas.
2. **Pasillo principal primero.** Es un árbol de caminos desde la puerta principal hasta las salidas, calculado con Dijkstra con penalización de giros. Hay tres estilos: el más corto, por el centro y paralelo a los muros. Lo que queda se divide en **zonas**.
3. **Llenado por filas.** Cada zona se llena con filas paralelas a un ángulo candidato (los ángulos dominantes de los muros, incluido 45°).
   - En cada fila los módulos se agrupan en bloques de hasta *E3* puestos por lado, separados por un paso transversal.
   - Entre filas se respetan *E2* (silla-silla, silla-espalda o espaldas pegadas).
   - Las verificaciones son O(1) por columna gracias a «corridas verticales» precalculadas.
4. **Validación sobre el piso amoblado.**
   - Cada silla debe alcanzarse desde la puerta principal por pasillos del ancho mínimo (transformada de distancia + Dijkstra por cubetas).
   - Se verifican el recorrido máximo a una salida, el recorrido común, las puertas no bloqueadas y los puestos accesibles en silla de ruedas.
   - Un módulo con un puesto inválido se descarta.
5. **Puntaje lexicográfico.** `100 000 × puestos fijos sin ubicar + 10 000 × puestos «máximo posible» que faltan (7 000 si es relleno) + 1 000 000 × reglas duras incumplidas + métricas blandas`. Ubicar los fijos siempre domina; después, más puestos; a igual cantidad, menos módulos sueltos.
6. **Búsqueda.**
   - Recocido simulado en paralelo sobre el «genoma»: ángulo, desfase, lado de inicio, alineación, emparejamiento y orden de tipos por zona, más la variante de pasillo.
   - Arranca con un multi-inicio barato que usa solo el decodificador.
   - Usa caché de disposiciones ya evaluadas.
7. **Asignación de personas.** Recocido por intercambios con costos unarios (rol ↔ tipo de escritorio, rol cerca de un elemento, reglas propias) y por pares (bloques por cliente, afinidad, separación acústica).

## Reglas del motor

La pantalla *Espacios* muestra solo las medidas de capacidad. El motor conserva el catálogo completo de reglas (en el proyecto `.puestos.json`), pero por defecto solo están activas las de circulación, sillas, puertas, columnas y oficinas; las de evacuación por distancia, luz, privacidad y densidad vienen apagadas.

| ID | Regla | ID | Regla |
|---|---|---|---|
| C1 | Pasillo principal (ancho, salidas, oficinas, tramos rectos) | E5 | No mezclar tipos en una bancada |
| C2 | Paso mínimo entre escritorios (obligatorio) y ancho preferido de pasillos | E6 | Orientaciones permitidas (muros inclinados) |
| C3 | Vestíbulo libre en la puerta principal | E7 | Misma dirección en todas las bancadas |
| C4 | Despeje en puertas (barrido + espacio al frente) | E8 | Escritorios contra el muro |
| C5 | Recorrido máximo hasta una salida 🔒 | O1 | Separación a columnas |
| C6 | Recorrido común 🔒 | O2 | Despeje de tableros y gabinetes 🔒 |
| C7 | Todos los puestos y puertas accesibles 🔒 | O3 | Distancia a oficinas fijas |
| C8 | Puestos accesibles en silla de ruedas | O4 | Zonas reservadas |
| E1 | Espacio para la silla | A1–A6 | Ventana lateral, espaldas, pantallas, luz, densidad, reserva |
| E2 | Separación entre filas | R1–R5 | Rol↔tipo, clientes en bloque, afinidad, cercanía, acústica |
| E3 | Máx. puestos seguidos por lado | | |
| E4 | Distancia máx. de la silla al pasillo | | |

Los valores por defecto son referencias típicas. **Verifica los de evacuación con la NSR-10, Título K.**

## Estructura

```
crates/core        núcleo en Rust (sin interfaz)
  import/            DXF (bloques, bulges, splines), PDF con capas OCG, DWG vía convertidor
  analyze.rs         roles de capas, escala, recinto por inundación, puertas, ventanas
  engine/            ctx (raster, pasillos, zonas), decode (filas), eval (validación y puntaje),
                     search (recocido, exploración, capacidad, sugerencias), custom (reglas propias)
  assign.rs          roles y clientes
  export.rs          DXF R12 y CSV
  pdf_out.rs         PDF vectorial a escala y DXF sin mobiliario
  simple.rs          plano de líneas simples (muros a eje por adelgazamiento)
  engine/fill.rs     relleno de huecos después de las filas
  api.rs             fachada JSON + trabajos en segundo plano
crates/devserver   servidor HTTP con la misma API (modo navegador)
crates/web         el mismo núcleo compilado a WebAssembly (versión web)
scripts/           build-wasm.sh: compila crates/web en dos variantes (con hilos y de un hilo)
.github/workflows  pages.yml: publica la versión web en GitHub Pages
src-tauri          app de escritorio
ui                 interfaz React + TypeScript
samples            planos de prueba (DXF, PDF) y su generador
```

Pruebas: `cargo test --release -p seat_core` (incluye el PDF real y los planos de muestra).
Herramientas de consola: `cargo run --release -p seat_core --example run -- plano.dxf salida.svg isla_flex=40,recto_120=10 8`.

## Limitaciones conocidas

- **Textos en PDF.** En los PDF de AutoCAD los textos suelen venir como trazos, así que no se leen los rótulos (OF 08, SALA…). En DXF sí se leen.
- **Puerta principal.** Si no hay capa de puerta principal, se marca la puerta más ancha, que puede no ser el acceso; revísalo.
- **Pasillo principal.** Se traza a 0°, 45° y 90° de la rejilla del dibujo. En plantas giradas un ángulo no múltiplo de 45° queda escalonado (las filas de escritorios sí siguen el ángulo de los muros).
- **Velocidad.** El tiempo de búsqueda escala con los núcleos del procesador. Con 8 núcleos, 10–20 s bastan para una planta de ~800 m².
