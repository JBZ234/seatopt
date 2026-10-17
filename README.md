# Optimizador de Puestos

App de escritorio (Tauri 2 + Rust + React) que recibe la planta de una oficina en **DXF, DWG o PDF vectorial** y propone la mejor ubicación de los escritorios según su geometría y un conjunto de reglas configurables. Después sienta a las personas por **rol** y **cliente**.

```
Plano ──▶ Escritorios ──▶ Reglas ──▶ Organización ──▶ Optimizar
(capas,    (catálogo y     (31 reglas   (roles, clientes,   (sliders, variantes,
 escala,    cantidades,     + reglas     equipos, afinidad)   sugerencias, exportar)
 recinto)   capacidad)      propias)
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

## Flujo de uso

1. **Plano**
   - Abre el archivo. La app sugiere un rol para cada capa (muro, columna, puerta, ventana, mobiliario existente…) y estima la escala con el radio de las puertas (≈ 0,90 m). En PDF propone 1:N.
   - Verifica la escala con **Calibrar**: dos clics sobre una medida conocida.
   - **Detectar automáticamente** inunda el recinto interior más grande y cierra los vanos de puertas. Con **Elegir con un clic** marcas el área que quieres.
   - Marca la **puerta principal** y las **salidas**.
   - Dibuja lo que falte: columnas, oficinas fijas, tableros con despeje, zonas reservadas y ventanas.
2. **Escritorios**
   - Se muestran los tipos disponibles con su vista previa y tú escribes cuántos ubicar.
   - Las islas **flexibles** se piden en *puestos* y el optimizador arma islas de distinto largo. Los demás tipos se piden en *unidades*.
   - **Estimar capacidad** calcula cuántos caben de cada tipo.
3. **Reglas**
   - Cada regla tiene un interruptor y sub-reglas con mini interruptores y medidas.
   - El **candado** impide que el sugeridor proponga relajarla.
   - En *Tus reglas* creas reglas propias sin tocar código: «[quién] [condición] [referencia] [valor]».
4. **Organización** (opcional): roles con color, tipo de escritorio, nivel de ruido y elemento al que deben estar cerca; clientes; matriz de cuántas personas de cada rol tiene cada cliente; afinidad entre roles.
5. **Optimizar**
   - Mueve los sliders y presiona **Optimizar**.
   - Si no caben todos, la app prueba relajar cada regla (que no esté bloqueada) y cambiar de tipo de escritorio, y muestra cuántos puestos gana cada cambio, con el botón **Aplicar**.
   - **Explorar variantes** corre el optimizador con otros pesos y muestra el frente de Pareto.
   - **Fijar escritorios** conserva los que elijas en las próximas corridas.
   - **Exportar** genera:
     - un DXF en las coordenadas del plano original, para insertarlo como referencia externa;
     - un CSV con número, tipo, rol, cliente, posición y avisos de cada puesto;
     - el proyecto `.puestos.json`.

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
5. **Puntaje lexicográfico.** `10 000 × puestos sin ubicar + 50 000 × reglas duras incumplidas + Σ pesos × métricas blandas`. Ubicar a todos siempre domina.
6. **Búsqueda.**
   - Recocido simulado en paralelo sobre el «genoma»: ángulo, desfase, lado de inicio, alineación, emparejamiento y orden de tipos por zona, más la variante de pasillo.
   - Arranca con un multi-inicio barato que usa solo el decodificador.
   - Usa caché de disposiciones ya evaluadas.
7. **Asignación de personas.** Recocido por intercambios con costos unarios (rol ↔ tipo de escritorio, rol cerca de un elemento, reglas propias) y por pares (bloques por cliente, afinidad, separación acústica).

**Por qué la exploración de sliders no es circular.** Si el optimizador ajustara los sliders para mejorar su propio puntaje, bajaría los pesos de lo que no logra cumplir. Por eso cada variante se mide con **indicadores fijos** (puestos, recorrido, luz, privacidad, direcciones, capacidad libre, agrupación) y se muestra el **frente de Pareto**: las variantes que no se pueden mejorar en algo sin empeorar otra cosa. Tú eliges, y «Usar pesos» copia los sliders de esa variante.

## Reglas incluidas

| ID | Regla | ID | Regla |
|---|---|---|---|
| C1 | Pasillo principal (ancho, salidas, oficinas, tramos rectos) | E5 | No mezclar tipos en una bancada |
| C2 | Pasillos secundarios y paso detrás de sillas | E6 | Orientaciones permitidas (muros inclinados) |
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
  api.rs             fachada JSON + trabajos en segundo plano
crates/devserver   servidor HTTP con la misma API (modo navegador)
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
