import { analyzePlan, exportCleanDxf, exportPdf, importPlan, loadBackground, openProject, setScale } from '../actions';
import { Fold, Num } from '../components/ui';
import { useApp, type Tool } from '../store';
import type { DoorKind, LayerRole, ObstacleKind } from '../types';

const ROLE_LABEL: Record<LayerRole, string> = {
  outline: 'Contorno del recinto', wall: 'Muro / bloquea', column: 'Columnas', office: 'Oficina fija',
  restricted: 'Zona con despeje', reserved: 'Zona reservada', door: 'Puertas', mainDoor: 'Puerta principal',
  exitDoor: 'Salida de emergencia', officeDoor: 'Puerta de oficina', window: 'Ventanas / fachada',
  furniture: 'Mobiliario existente', ignore: 'Ignorar',
};
const DOOR_LABEL: Record<DoorKind, string> = { main: 'Principal', exit: 'Salida', office: 'Oficina', ignore: 'Ignorar' };
const OBS_LABEL: Record<ObstacleKind, string> = { column: 'Columnas', fixedOffice: 'Oficinas fijas', restricted: 'Zonas con despeje', reserved: 'Zonas reservadas' };

export default function PlanStep() {
  const st = useApp();
  const { drawing, plan, mmPerUnit, roles, background, hiddenLayers, notes } = st;

  if (!drawing && !plan) {
    return (
      <div className="step-body empty">
        <h2>Empieza por el plano</h2>
        <p>Abre la planta de la oficina en DXF, DWG o PDF vectorial exportado desde AutoCAD. La app reconoce las capas, estima la escala y detecta el área a amoblar, las columnas y las puertas.</p>
        <div className="row">
          <button className="primary" onClick={importPlan}>Abrir plano</button>
          <button onClick={openProject}>Abrir proyecto guardado</button>
        </div>
        <p className="hint">Los DWG se convierten con ODA File Converter (gratuito) o LibreDWG. Si está instalado en otra carpeta, indica la ruta en «Configuración» al final de este paso.</p>
      </div>
    );
  }

  const isPdf = drawing?.source === 'pdf';
  const paper = isPdf ? mmPerUnit / (25.4 / 72) : null;
  const tool = (t: Tool, hint: string) => {
    st.set({ tool: st.tool === t ? 'select' : t, calib: [] });
    if (st.tool !== t) st.say(hint);
  };
  const ToolBtn = ({ t, label, hint }: { t: Tool; label: string; hint: string }) => (
    <button className={'chip' + (st.tool === t ? ' active' : '')} onClick={() => tool(t, hint)}>{label}</button>
  );

  const doorsBy = (k: DoorKind) => plan?.doors.filter((d) => d.kind === k).length ?? 0;
  const obsBy = (k: ObstacleKind) => plan?.obstacles.filter((o) => o.kind === k).length ?? 0;

  return (
    <div className="step-body">
      <div className="file-line">
        <strong title={drawing?.name}>{drawing?.name ?? 'Proyecto'}</strong>
        <button className="ghost small" onClick={importPlan}>Abrir otro</button>
      </div>

      <h3>Escala</h3>
      <div className="field-row">
        {isPdf ? (
          <label>1 :
            <Num value={paper ?? 100} unit="" width={64} label="Escala del papel" onChange={(v) => setScale(v * 25.4 / 72)} />
          </label>
        ) : (
          <label>Unidad del dibujo
            <select value={mmPerUnit} onChange={(e) => setScale(parseFloat(e.target.value))}>
              <option value={1}>milímetros</option>
              <option value={10}>centímetros</option>
              <option value={1000}>metros</option>
              <option value={25.4}>pulgadas</option>
              <option value={304.8}>pies</option>
              {![1, 10, 1000, 25.4, 304.8].includes(mmPerUnit) && <option value={mmPerUnit}>calibrada ({mmPerUnit.toPrecision(4)} mm)</option>}
            </select>
          </label>
        )}
        <ToolBtn t="calibrate" label="Calibrar" hint="Haz clic en los dos extremos de una medida conocida (p. ej. un muro o una puerta)." />
      </div>
      {drawing?.scale.reason && <p className="hint">{drawing.scale.reason}</p>}

      <h3>Área a amoblar</h3>
      <div className="row wrap">
        <button onClick={() => analyzePlan(null)} disabled={!drawing?.drawingId}>Detectar automáticamente</button>
        <ToolBtn t="seed" label="Elegir con un clic" hint="Haz clic dentro del área que quieres amoblar." />
      </div>
      <div className="field-row">
        <label title="Las aberturas de este ancho o menores (vanos de puertas) se cierran para separar el recinto de otros espacios.">Cerrar vanos de hasta
          <Num value={st.gapClose} unit="mm" label="Cerrar vanos" onChange={(v) => st.set({ gapClose: v })} />
        </label>
      </div>
      {plan && (
        <p className="stat-line"><span className="big">{st.areaM2.toFixed(0)}</span> m² útiles · {plan.obstacles.length} obstáculos · {plan.doors.length} puertas · {plan.windows.length} tramos de ventana</p>
      )}

      {plan && (
        <>
          <h3>Puertas</h3>
          <p className="hint">La principal debe ser el acceso del personal. Usa las herramientas y haz clic sobre la puerta en el plano.</p>
          <div className="row wrap">
            <ToolBtn t="mainDoor" label="Marcar principal" hint="Haz clic sobre la puerta de acceso del personal." />
            <ToolBtn t="exitDoor" label="Marcar / quitar salida" hint="Haz clic sobre las salidas de emergencia." />
            <ToolBtn t="door" label="Cambiar tipo" hint="Cada clic sobre una puerta cambia su tipo: oficina → salida → principal → ignorar." />
          </div>
          <div className="legend">
            {(['main', 'exit', 'office', 'ignore'] as DoorKind[]).map((k) => (
              <span key={k}><i className={'sw door-' + k} />{DOOR_LABEL[k]} {doorsBy(k)}</span>
            ))}
          </div>
          {doorsBy('main') === 0 && <p className="warn">Falta marcar la puerta principal.</p>}

          <h3>Obstáculos y zonas</h3>
          <p className="hint">Arrastra en el plano para dibujar. «Borrar» elimina el obstáculo, ventana o puerta en que hagas clic.</p>
          <div className="row wrap">
            <ToolBtn t="column" label="Columna" hint="Arrastra para dibujar una columna." />
            <ToolBtn t="fixedOffice" label="Oficina fija" hint="Arrastra para dibujar una oficina o sala que no se amobla." />
            <ToolBtn t="restricted" label="Tablero / gabinete" hint="Arrastra sobre un tablero eléctrico o gabinete: se le deja despeje al frente." />
            <ToolBtn t="reserved" label="Zona reservada" hint="Arrastra para reservar un área (impresoras, archivo, lockers…)." />
            <ToolBtn t="window" label="Ventana" hint="Arrastra a lo largo de la fachada para marcar ventanas." />
            <ToolBtn t="erase" label="Borrar" hint="Haz clic sobre lo que quieras borrar." />
          </div>
          <div className="legend">
            {(['column', 'fixedOffice', 'restricted', 'reserved'] as ObstacleKind[]).map((k) => (
              <span key={k}><i className={'sw ob-' + k} />{OBS_LABEL[k]} {obsBy(k)}</span>
            ))}
          </div>
        </>
      )}

      {drawing && drawing.layers.length > 0 && (
        <Fold title={`Capas del dibujo (${drawing.layers.length})`} open={!plan}>
          <p className="hint">Cada capa cumple un papel. Si el recinto sale mal, corrige aquí y vuelve a detectar.</p>
          <table className="layers">
            <tbody>
              {drawing.layers.map((l) => {
                const visible = !hiddenLayers.includes(l.name);
                return (
                  <tr key={l.name}>
                    <td>
                      <button className="eye" aria-label={visible ? 'Ocultar capa' : 'Mostrar capa'} aria-pressed={visible}
                        onClick={() => st.set({ hiddenLayers: visible ? [...hiddenLayers, l.name] : hiddenLayers.filter((h) => h !== l.name) })}>
                        <i style={{ background: visible ? l.color : 'transparent', borderColor: l.color }} />
                      </button>
                    </td>
                    <td className="lname" title={`${l.name} · ${l.paths} trazos`}>{l.name}</td>
                    <td>
                      <select value={roles[l.name]} onChange={(e) => st.set({ roles: { ...roles, [l.name]: e.target.value as LayerRole } })}>
                        {Object.entries(ROLE_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
                      </select>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="row">
            <button onClick={async () => { await loadBackground(); await analyzePlan(null); }}>Aplicar y volver a detectar</button>
          </div>
        </Fold>
      )}

      {background && (
        <label className="check"><input type="checkbox" checked={st.showBackground} onChange={(e) => st.set({ showBackground: e.target.checked })} /> Mostrar el dibujo de fondo</label>
      )}

      {drawing?.drawingId && (
        <Fold title="Plano sin mobiliario">
          <p className="hint">Exporta el plano original sin las capas marcadas como «Mobiliario existente» (escritorios, sillas, mesas). Revisa los roles de capa antes.</p>
          <p className="hint">Se quitan: {drawing.layers.filter((l) => roles[l.name] === 'furniture').map((l) => l.name).join(', ') || 'ninguna capa'}.</p>
          <div className="row wrap">
            <button onClick={exportCleanDxf}>DXF sin mobiliario</button>
            <button onClick={() => exportPdf({ clean: true })}>PDF sin mobiliario</button>
          </div>
        </Fold>
      )}

      {notes.length > 0 && (
        <Fold title="Notas de la importación">
          <ul className="notes">{notes.map((n, i) => <li key={i}>{n}</li>)}</ul>
        </Fold>
      )}

      <Fold title="Configuración">
        <label className="stack">Ruta de ODA File Converter o dwg2dxf (opcional)
          <input value={st.converter} placeholder="C:\Program Files\ODA\ODAFileConverter 25.x\ODAFileConverter.exe" onChange={(e) => st.set({ converter: e.target.value })} />
        </label>
      </Fold>

      {plan && (
        <div className="next">
          <button className="primary" disabled={doorsBy('main') === 0} onClick={() => st.set({ step: 1, tool: 'select' })}>Continuar: escritorios</button>
        </div>
      )}
    </div>
  );
}
