"""Genera planos de prueba (DXF + PDF vectorial a escala) para el optimizador de puestos.

Convención de capas (unidades: milímetros, INSUNITS=4):
  A-MURO-EXT        polilínea cerrada: contorno del recinto
  A-COLUMNA         polilíneas cerradas: columnas / pilares
  A-OFICINA-FIJA    polilíneas cerradas: oficinas internas, salas, baños, cocinas
  A-PUERTA-PPAL     línea sobre el muro: vano de la puerta principal (+ arco de giro)
  A-PUERTA-SALIDA   línea sobre el muro: salida de emergencia
  A-PUERTA-OFICINA  línea sobre el muro de una oficina fija: acceso a esa oficina
  A-VENTANA         línea sobre el muro exterior (para reglas de luz natural)
  A-ZONA-RESTRINGIDA polilíneas cerradas: áreas donde no se puede poner nada (tableros, cuarto técnico)
  A-TEXTO           etiquetas
"""
import math, os
import ezdxf
from ezdxf.enums import TextEntityAlignment
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.backends.backend_pdf import PdfPages

OUT = os.path.dirname(os.path.abspath(__file__))
os.makedirs(OUT, exist_ok=True)

LAYERS = {
    "A-MURO-EXT": 7, "A-COLUMNA": 1, "A-OFICINA-FIJA": 5, "A-PUERTA-PPAL": 3,
    "A-PUERTA-SALIDA": 2, "A-PUERTA-OFICINA": 4, "A-VENTANA": 140,
    "A-ZONA-RESTRINGIDA": 6, "A-TEXTO": 8,
}
COLORS = {"A-MURO-EXT": "black", "A-COLUMNA": "red", "A-OFICINA-FIJA": "blue",
          "A-PUERTA-PPAL": "green", "A-PUERTA-SALIDA": "orange",
          "A-PUERTA-OFICINA": "cyan", "A-VENTANA": "deepskyblue",
          "A-ZONA-RESTRINGIDA": "magenta", "A-TEXTO": "gray"}


def rect(x, y, w, h):
    return [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]


class Plan:
    def __init__(self, name, desc):
        self.name, self.desc = name, desc
        self.outline = []
        self.items = []  # (layer, kind, geom, label)

    def wall(self, pts): self.outline = pts
    def column(self, x, y, s=600, sy=None): self.items.append(("A-COLUMNA", "poly", rect(x - s/2, y - (sy or s)/2, s, sy or s), None))
    def office(self, x, y, w, h, label): self.items.append(("A-OFICINA-FIJA", "poly", rect(x, y, w, h), label))
    def restricted(self, x, y, w, h, label): self.items.append(("A-ZONA-RESTRINGIDA", "poly", rect(x, y, w, h), label))

    def door(self, layer, p1, p2, swing_side=1, label=None):
        """p1-p2 vano sobre un muro. swing_side=+1/-1 lado hacia donde abre (normal izquierda/derecha)."""
        self.items.append((layer, "door", (p1, p2, swing_side), label))

    def window(self, p1, p2): self.items.append(("A-VENTANA", "line", (p1, p2), None))

    # ---------- DXF ----------
    def to_dxf(self):
        doc = ezdxf.new("R2018", setup=True)
        doc.header["$INSUNITS"] = 4  # mm
        doc.header["$MEASUREMENT"] = 1
        for n, c in LAYERS.items():
            doc.layers.add(n, color=c)
        msp = doc.modelspace()
        msp.add_lwpolyline(self.outline, close=True, dxfattribs={"layer": "A-MURO-EXT"})
        for layer, kind, g, label in self.items:
            if kind == "poly":
                msp.add_lwpolyline(g, close=True, dxfattribs={"layer": layer})
                if label:
                    cx = sum(p[0] for p in g) / 4; cy = sum(p[1] for p in g) / 4
                    msp.add_text(label, height=250, dxfattribs={"layer": "A-TEXTO"}).set_placement((cx, cy), align=TextEntityAlignment.MIDDLE_CENTER)
            elif kind == "line":
                msp.add_line(*g, dxfattribs={"layer": layer})
            elif kind == "door":
                p1, p2, side = g
                msp.add_line(p1, p2, dxfattribs={"layer": layer})
                # hoja y arco de giro (90°) desde p1
                dx, dy = p2[0] - p1[0], p2[1] - p1[1]
                L = math.hypot(dx, dy)
                nx, ny = -dy / L * side, dx / L * side
                leaf_end = (p1[0] + nx * L, p1[1] + ny * L)
                msp.add_line(p1, leaf_end, dxfattribs={"layer": layer})
                a0 = math.degrees(math.atan2(dy, dx)); a1 = math.degrees(math.atan2(ny, nx))
                start, end = (a0, a1) if (a1 - a0) % 360 <= 180 else (a1, a0)
                msp.add_arc(p1, L, start, end, dxfattribs={"layer": layer})
                if label:
                    mx, my = (p1[0] + p2[0]) / 2 + nx * 600, (p1[1] + p2[1]) / 2 + ny * 600
                    msp.add_text(label, height=200, dxfattribs={"layer": "A-TEXTO"}).set_placement((mx, my), align=TextEntityAlignment.MIDDLE_CENTER)
        xs = [p[0] for p in self.outline]; ys = [p[1] for p in self.outline]
        msp.add_text(f"{self.name} - {self.desc}", height=350, dxfattribs={"layer": "A-TEXTO"}).set_placement((min(xs), min(ys) - 900))
        path = f"{OUT}/{self.name}.dxf"
        doc.saveas(path)
        return path

    # ---------- dibujo (PNG preview + PDF a escala) ----------
    def draw(self, ax):
        xs = [p[0] for p in self.outline] + [self.outline[0][0]]
        ys = [p[1] for p in self.outline] + [self.outline[0][1]]
        ax.plot(xs, ys, color="black", lw=1.6)
        for layer, kind, g, label in self.items:
            c = COLORS[layer]
            if kind == "poly":
                px = [p[0] for p in g] + [g[0][0]]; py = [p[1] for p in g] + [g[0][1]]
                ax.fill(px, py, color=c, alpha=0.15); ax.plot(px, py, color=c, lw=1)
                if label:
                    ax.text(sum(p[0] for p in g)/4, sum(p[1] for p in g)/4, label, ha="center", va="center", fontsize=6, color=c)
            elif kind == "line":
                ax.plot([g[0][0], g[1][0]], [g[0][1], g[1][1]], color=c, lw=3)
            elif kind == "door":
                p1, p2, side = g
                ax.plot([p1[0], p2[0]], [p1[1], p2[1]], color=c, lw=4)
                dx, dy = p2[0]-p1[0], p2[1]-p1[1]; L = math.hypot(dx, dy)
                nx, ny = -dy/L*side, dx/L*side
                a0 = math.atan2(dy, dx); a1 = math.atan2(ny, nx)
                d = (a1 - a0 + math.pi) % (2*math.pi) - math.pi
                ts = [a0 + d*i/30 for i in range(31)]
                ax.plot([p1[0]+L*math.cos(t) for t in ts], [p1[1]+L*math.sin(t) for t in ts], color=c, lw=0.8, ls="--")
                ax.plot([p1[0], p1[0]+nx*L], [p1[1], p1[1]+ny*L], color=c, lw=1)
                if label:
                    ax.text((p1[0]+p2[0])/2+nx*700, (p1[1]+p2[1])/2+ny*700, label, ha="center", va="center", fontsize=6, color=c)
        ax.set_aspect("equal"); ax.axis("off")

    def bbox(self):
        xs = [p[0] for p in self.outline]; ys = [p[1] for p in self.outline]
        return min(xs), min(ys), max(xs), max(ys)

    def to_png(self):
        fig, ax = plt.subplots(figsize=(10, 7))
        self.draw(ax); ax.set_title(f"{self.name}: {self.desc}", fontsize=9)
        fig.savefig(f"{OUT}/preview_{self.name}.png", dpi=110, bbox_inches="tight"); plt.close(fig)

    def to_pdf_scaled(self, scale=100):
        """PDF vectorial con escala real 1:scale (A3 / A2 según tamaño) + barra de escala."""
        x0, y0, x1, y1 = self.bbox()
        margin_mm = 25
        w_mm = (x1 - x0) / scale + 2 * margin_mm
        h_mm = (y1 - y0) / scale + 2 * margin_mm + 15
        fig = plt.figure(figsize=(w_mm / 25.4, h_mm / 25.4))
        ax = fig.add_axes([margin_mm / w_mm, (margin_mm + 15) / h_mm, (x1 - x0) / scale / w_mm, (y1 - y0) / scale / h_mm])
        self.draw(ax)
        ax.set_xlim(x0, x1); ax.set_ylim(y0, y1); ax.set_aspect("auto")
        # barra de escala 5 m
        bx = fig.add_axes([margin_mm / w_mm, 8 / h_mm, 5000 / scale / w_mm, 4 / h_mm])
        for i in range(5):
            bx.add_patch(plt.Rectangle((i, 0), 1, 1, color="black" if i % 2 == 0 else "white", ec="black"))
        bx.set_xlim(0, 5); bx.set_ylim(0, 1); bx.axis("off")
        fig.text(margin_mm / w_mm, 14 / h_mm, f"ESCALA 1:{scale}  |  0 ─ 5 m  |  {self.name}", fontsize=7)
        path = f"{OUT}/{self.name}_esc1-{scale}.pdf"
        with PdfPages(path) as pdf:
            pdf.savefig(fig)
        plt.close(fig)
        return path


plans = []

# 1) Sala rectangular simple — prueba básica
p = Plan("01_rectangular_simple", "12x8 m, 1 puerta principal, 1 salida, sin obstaculos")
p.wall(rect(0, 0, 12000, 8000))
p.door("A-PUERTA-PPAL", (5500, 0), (6500, 0), 1, "PPAL")
p.door("A-PUERTA-SALIDA", (12000, 6500), (12000, 7400), 1, "SALIDA")
p.window((0, 1000), (0, 7000))
plans.append(p)

# 2) Sala con columnas en grilla — similar a la foto del pilar decorado
p = Plan("02_columnas_grilla", "20x14 m, grilla de 6 columnas 60x60 cm")
p.wall(rect(0, 0, 20000, 14000))
for cx in (6500, 13500):
    for cy in (3500, 7000, 10500):
        p.column(cx, cy, 600)
p.door("A-PUERTA-PPAL", (0, 6400), (0, 7600), 1, "PPAL")
p.door("A-PUERTA-SALIDA", (19100, 14000), (18200, 14000), 1, "SALIDA")
p.window((2000, 0), (18000, 0)); p.window((20000, 1000), (20000, 13000))
plans.append(p)

# 3) Planta en L con oficinas fijas perimetrales (parecido a la oficina de las fotos)
p = Plan("03_planta_L_oficinas", "Planta en L, 3 oficinas fijas, sala de juntas, columna")
p.wall([(0, 0), (26000, 0), (26000, 9000), (11000, 9000), (11000, 18000), (0, 18000)])
p.office(0, 14000, 4500, 4000, "GERENCIA")
p.office(4500, 14000, 3500, 4000, "OFICINA PM")
p.office(21000, 5500, 5000, 3500, "SALA JUNTAS")
p.office(0, 0, 3500, 3000, "BANOS")
p.door("A-PUERTA-OFICINA", (1500, 14000), (2400, 14000), -1, "")
p.door("A-PUERTA-OFICINA", (5500, 14000), (6400, 14000), -1, "")
p.door("A-PUERTA-OFICINA", (21000, 6500), (21000, 7400), 1, "")
p.door("A-PUERTA-OFICINA", (3500, 1500), (3500, 2400), -1, "")
p.column(11000 + 300, 4500, 600, 900)  # columna rectangular (pilar decorado)
p.column(18000, 4500, 600)
p.door("A-PUERTA-PPAL", (15000, 0), (16600, 0), 1, "PPAL (doble)")
p.door("A-PUERTA-SALIDA", (0, 9500), (0, 10400), -1, "SALIDA")
p.window((26000, 500), (26000, 5000)); p.window((11000, 10000), (11000, 17500))
plans.append(p)

# 4) Open plan grande con núcleo central (escaleras/ascensores) — capacidad ~100
p = Plan("04_openplan_nucleo", "36x24 m, nucleo central fijo, 12 columnas, 2 salidas")
p.wall(rect(0, 0, 36000, 24000))
p.office(14000, 8500, 8000, 7000, "NUCLEO (ascensores/escaleras)")
p.door("A-PUERTA-OFICINA", (17500, 8500), (18500, 8500), -1, "")
p.door("A-PUERTA-OFICINA", (17500, 15500), (18500, 15500), 1, "")
for cx in (6000, 12000, 24000, 30000):
    for cy in (6000, 12000, 18000):
        p.column(cx, cy, 700)
p.office(0, 18000, 5000, 6000, "SALA 1"); p.office(31000, 18000, 5000, 6000, "SALA 2")
p.door("A-PUERTA-OFICINA", (5000, 19500), (5000, 20400), 1, "")
p.door("A-PUERTA-OFICINA", (31000, 19500), (31000, 20400), -1, "")
p.restricted(33500, 0, 2500, 1500, "TABLERO ELEC.")
p.door("A-PUERTA-PPAL", (17000, 0), (19000, 0), 1, "PPAL")
p.door("A-PUERTA-SALIDA", (0, 2000), (0, 3000), -1, "SALIDA 1")
p.door("A-PUERTA-SALIDA", (36000, 13000), (36000, 14000), 1, "SALIDA 2")
p.window((1000, 24000), (35000, 24000))
plans.append(p)

# 5) Irregular con muro inclinado — prueba de geometría no ortogonal
p = Plan("05_irregular_diagonal", "Poligono con muro a 30 grados y esquina recortada")
p.wall([(0, 0), (22000, 0), (22000, 6000), (16000, 6000 + 6000 * math.tan(math.radians(30))), (16000, 14000), (3000, 14000), (0, 11000)])
p.column(8000, 7000, 600); p.column(14000, 4000, 500)
p.office(3000, 0, 4000, 3000, "CUARTO TI")
p.door("A-PUERTA-OFICINA", (7000, 1000), (7000, 1900), -1, "")
p.door("A-PUERTA-PPAL", (9000, 14000), (10200, 14000), -1, "PPAL")
p.door("A-PUERTA-SALIDA", (22000, 1500), (22000, 2400), 1, "SALIDA")
plans.append(p)

# 6) Pasillo angosto — caso donde NO caben todos (para probar sugerencias de reglas)
p = Plan("06_angosto_capacidad", "Sala angosta 18x5.2 m - forzar falta de espacio")
p.wall(rect(0, 0, 18000, 5200))
p.column(6000, 2600, 500); p.column(12000, 2600, 500)
p.door("A-PUERTA-PPAL", (0, 2100), (0, 3100), -1, "PPAL")
p.door("A-PUERTA-SALIDA", (18000, 2100), (18000, 3000), 1, "SALIDA")
plans.append(p)

for p in plans:
    print(p.to_dxf()); p.to_png()
# PDF vectoriales a escala para dos casos
print(plans[2].to_pdf_scaled(100))
print(plans[3].to_pdf_scaled(150))
