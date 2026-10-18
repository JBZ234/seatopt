import { useEffect, useRef } from 'react';
import { analyzePlan, deleteLayer, exportCleanDxf, exportPdf, exportPlanDxf, exportSimple, importPlan, loadBackground, openProject, setScale } from '../actions';
import { Fold, Num } from '../components/ui';
import { useApp, type Tool } from '../store';
import type { DoorKind, LayerRole, ObstacleKind } from '../types';

const ROLE_LABEL: Record<LayerRole, string> = {
  outline: 'Contorno del recinto', wall: 'Muro / bloquea', column: 'Columnas', office: 'Oficina fija',
  restricted: 'Zona con despeje', reserved: 'Zona reservada', door: 'Puertas', mainDoor: 'Puerta principal',
  exitDoor: 'Salida de emergencia', officeDoor: 'Puerta de oficina', window: 'Ventanas / fachada',
  furniture: 'Mobiliario existente', room: 'Recintos (referencia)', building: 'Edificio (referencia)', ignore: 'Ignorar',
};
const DOOR_LABEL: Record<DoorKind, string> = { main: 'Principal', exit: 'Salida', office: 'Oficina', ignore: 'Ignorada' };
const OBS_LABEL: Record<ObstacleKind, string> = { column: 'Columnas', fixedOffice: 'Oficinas fijas', restricted: 'Zonas con despeje', reserved: 'Zonas reservadas' };

type TaskId = 'scale' | 'area' | 'door';

export default function PlanStep() {
  const st = useApp();
  const { drawing, plan, mmPerUnit, roles, background, hiddenLayers, notes } = st;

  const areaOk = !!plan && plan.region.length > 0 && plan.region[0].length >= 3;
  const mainOk = !!plan && plan.doors.some((d) => d.kind === 'main');
  const scaleKnown = st.scaleConfirmed || (drawing?.scale.reason ?? '').includes('guardada') || (drawing?.source !== 'pdf' && (drawing?.unitNote ?? '').startsWith('Unidades del archivo'));
  const pending: TaskId | null = !areaOk ? 'area' : !mainOk || !st.mainConfirmed ? 'door' : null;

  // activar la herramienta de la tarea pendiente (una vez por tarea)
  const lastAuto = useRef<string>('');
  useEffect(() => {
    if (!drawing && !plan) return;
    const key = `${drawing?.drawingId}|${pending}`;
    if (lastAuto.current === key) return;
    lastAuto.current = key;
    if (pending === 'area' && background) st.set({ tool: 'seed' });
    else if (pending === 'door') st.set({ tool: plan && plan.doors.length > 0 ? 'mainDoor' : 'drawDoor' });
    else if (pending === null && ['seed', 'mainDoor', 'drawDoor', 'outline'].includes(st.tool)) st.set({ tool: 'select' });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending, drawing?.drawingId, !!background]);

  if (!drawing && !plan) {
    return (
      <div className="step-body empty">
        <h2>Empieza por el plano</h2>
        <p>Abre la planta de la oficina en DXF, DWG o PDF. Después la app te guía en tres pasos: confirmar la escala, marcar el área a amoblar y señalar la puerta principal.</p>
        <div className="row">
          <button className="primary" onClick={importPlan}>Abrir plano</button>
          <button onClick={openProject}>Abrir proyecto guardado</button>
        </div>
        <p className="hint">Los DWG se convierten con ODA File Converter (gratuito) o LibreDWG. Si está instalado en otra carpeta, indica la ruta en «Configuración».</p>
        <Fold title="Configuración">
          <label className="stack">Ruta de ODA File Converter o dwg2dxf (opcional)
            <input value={st.converter} placeholder="C:\Program Files\ODA\ODAFileConverter 25.x\ODAFileConverter.exe" onChange={(e) => st.set({ converter: e.target.value })} />
          </label>
        </Fold>
      </div>
    );
  }

  const isPdf = drawing?.source === 'pdf';
  const paper = isPdf ? mmPerUnit / (25.4 / 72) : null;
  const useTool = (t: Tool) => st.set({ tool: st.tool === t ? 'select' : t, calib: [] });
  const ToolBtn = ({ t, label, primary }: { t: Tool; label: string; primary?: boolean }) => (
    <button className={(primary ? 'primary ' : '') + (st.tool === t ? 'active-tool' : '')} aria-pressed={st.tool === t} onClick={() => useTool(t)}>{label}</button>
  );
  const ChipTool = ({ t, label }: { t: Tool; label: string }) => (
    <button className={'chip' + (st.tool === t ? ' active' : '')} aria-pressed={st.tool === t} onClick={() => useTool(t)}>{label}</button>
  );
  const doorsBy = (k: DoorKind) => plan?.doors.filter((d) => d.kind === k).length ?? 0;
  const obsBy = (k: ObstacleKind) => plan?.obstacles.filter((o) => o.kind === k).length ?? 0;

  const Task = ({ id, n, title, done, warn, children }: { id: TaskId; n: number; title: string; done: boolean; warn?: boolean; children: React.ReactNode }) => (
    <li className={'task' + (done ? ' done' : '') + (pending === id || (warn && done && id !== 'scale' && pending === null) ? ' current' : '') + (warn ? ' unsure' : '')}>
      <span className="task-mark" aria-hidden>{done ? (warn ? '!' : '✓') : n}</span>
      <div className="task-body">
        <div className="task-title">{title}</div>
        {children}
      </div>
    </li>
  );

  return (
    <div className="step-body">
      <div className="file-line">
        <strong title={drawing?.name}>{drawing?.name ?? 'Proyecto'}</strong>
      </div>

      <ol className="tasks">
        <Task id="scale" n={1} title="Escala" done warn={!scaleKnown}>
          <div className="field-row">
            {isPdf ? (
              <label>1 :
                <Num value={Math.round(paper ?? 100)} unit="" width={60} label="Escala del papel" onChange={(v) => { st.set({ scaleConfirmed: true }); setScale(v * 25.4 / 72); }} />
              </label>
            ) : (
              <select aria-label="Unidad del dibujo" value={mmPerUnit} onChange={(e) => { st.set({ scaleConfirmed: true }); setScale(parseFloat(e.target.value)); }}>
                <option value={1}>milímetros</option>
                <option value={10}>centímetros</option>
                <option value={1000}>metros</option>
                <option value={25.4}>pulgadas</option>
                <option value={304.8}>pies</option>
                {![1, 10, 1000, 25.4, 304.8].includes(mmPerUnit) && <option value={mmPerUnit}>calibrada</option>}
              </select>
            )}
            <ChipTool t="calibrate" label="Calibrar con una medida" />
            {!scaleKnown && <button className="ghost small" onClick={() => st.set({ scaleConfirmed: true })}>Es correcta</button>}
          </div>
          <p className="hint">{scaleKnown ? (drawing?.scale.reason.includes('guardada') ? drawing.scale.reason : 'Escala confirmada.') : `${drawing?.scale.reason ?? ''}`}</p>
        </Task>

        <Task id="area" n={2} title="Área a amoblar" done={areaOk}>
          {areaOk ? (
            <p className="stat-line"><span className="big">{st.areaM2.toFixed(0)}</span> m² · {plan!.obstacles.length} obstáculos · {plan!.rooms?.length ?? 0} recintos</p>
          ) : (
            <p className="hint">Haz clic dentro de la zona de la oficina que se va a amoblar. Si no queda bien cerrada, dibuja su contorno.</p>
          )}
          <div className="row wrap">
            <ToolBtn t="seed" label={areaOk ? 'Elegir otra zona' : 'Clic dentro del área'} primary={!areaOk} />
            <ToolBtn t="outline" label="Dibujar contorno" />
            {drawing?.drawingId && <button className="ghost" onClick={() => analyzePlan(null)}>Detectar sola</button>}
          </div>
        </Task>

        <Task id="door" n={3} title="Puerta principal" done={mainOk} warn={mainOk && !st.mainConfirmed}>
          {mainOk && !st.mainConfirmed ? (
            <>
              <p className="hint">Se eligió la puerta más ancha (en verde en el plano). ¿Es el acceso del personal?</p>
              <div className="row wrap">
                <button className="primary" onClick={() => st.set({ mainConfirmed: true })}>Sí, es esa</button>
              </div>
            </>
          ) : mainOk ? (
            <p className="hint">Listo. Puedes marcar también las salidas de emergencia (opcional, abajo).</p>
          ) : (
            <p className="hint">{plan && plan.doors.length > 0 ? `Se detectaron ${plan.doors.length} puertas. Haz clic en la de acceso del personal.` : 'No se detectaron puertas: arrastra sobre el vano de la puerta de acceso.'}</p>
          )}
          {areaOk && (
            <div className="row wrap">
              {plan!.doors.length > 0 && <ToolBtn t="mainDoor" label={mainOk ? 'Cambiar principal' : 'Marcar en el plano'} primary={!mainOk && plan!.doors.length > 0} />}
              <ToolBtn t="drawDoor" label="Dibujar puerta" primary={!mainOk && plan!.doors.length === 0} />
            </div>
          )}
        </Task>
      </ol>

      {areaOk && mainOk && (
        <div className="next">
          <button className="primary" onClick={() => st.set({ step: 1, tool: 'select' })}>Continuar: escritorios</button>
        </div>
      )}

      <h3 className="section-label">Ajustes opcionales</h3>

      {plan && (
        <Fold title={`Puertas y salidas (${plan.doors.length})`}>
          <div className="row wrap">
            <ChipTool t="exitDoor" label="Marcar / quitar salida" />
            <ChipTool t="door" label="Cambiar tipo" />
            <ChipTool t="drawDoor" label="Dibujar puerta" />
            <ChipTool t="erase" label="Borrar" />
          </div>
          <div className="legend">
            {(['main', 'exit', 'office', 'ignore'] as DoorKind[]).map((k) => (
              <span key={k}><i className={'sw door-' + k} />{DOOR_LABEL[k]} {doorsBy(k)}</span>
            ))}
          </div>
        </Fold>
      )}

      {plan && (
        <Fold title="Obstáculos y zonas">
          <p className="hint">Arrastra en el plano para dibujar; las esquinas se enganchan al dibujo.</p>
          <div className="row wrap">
            <ChipTool t="column" label="Columna" />
            <ChipTool t="fixedOffice" label="Oficina fija" />
            <ChipTool t="restricted" label="Tablero / gabinete" />
            <ChipTool t="reserved" label="Zona reservada" />
            <ChipTool t="window" label="Ventana" />
            <ChipTool t="erase" label="Borrar" />
          </div>
          <div className="legend">
            {(['column', 'fixedOffice', 'restricted', 'reserved'] as ObstacleKind[]).map((k) => (
              <span key={k}><i className={'sw ob-' + k} />{OBS_LABEL[k]} {obsBy(k)}</span>
            ))}
          </div>
        </Fold>
      )}

      {drawing && drawing.layers.length > 0 && (
        <Fold title={`Capas del dibujo (${drawing.layers.length})`}>
          <p className="hint">Si el área sale mal, revisa qué capas son muros y cuáles se ignoran, y vuelve a detectar.</p>
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
                    <td>
                      <button className="ghost small danger icon-btn" title={`Borrar todas las líneas y objetos de «${l.name}»`} aria-label={`Borrar la capa ${l.name}`} onClick={() => deleteLayer(l.name)}>
                        <svg viewBox="0 0 16 16" aria-hidden><path d="M3 4h10M6 4V2.5h4V4M4.5 4l.7 9.5h5.6l.7-9.5" /></svg>
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="row">
            <button onClick={async () => { await loadBackground(); await analyzePlan(null); }}>Aplicar y volver a detectar</button>
          </div>
          <div className="field-row">
            <label title="Las aberturas de este ancho o menores (vanos de puertas) se cierran para separar el área de otros espacios.">Cerrar vanos de hasta
              <Num value={st.gapClose} unit="mm" label="Cerrar vanos" onChange={(v) => st.set({ gapClose: v })} />
            </label>
          </div>
          {background && (
            <label className="check"><input type="checkbox" checked={st.showBackground} onChange={(e) => st.set({ showBackground: e.target.checked })} /> Mostrar el dibujo de fondo</label>
          )}
        </Fold>
      )}

      {plan && areaOk && (
        <Fold title="Exportar plano base">
          <p className="hint">Plano mínimo para optimizar: contorno, oficinas y recintos, columnas y puertas, con capas estándar. Se vuelve a importar sin ajustes (el PDF guarda la escala).</p>
          <div className="row wrap">
            <button onClick={exportPlanDxf}>DXF simplificado</button>
            <button onClick={() => exportPdf({ clean: true })}>PDF simplificado</button>
          </div>
          {drawing?.drawingId && (
            <>
              <p className="hint">Dibujo original en líneas simples: cada muro como una sola línea, más columnas, puertas y oficinas. Sin mobiliario, textos ni detalles.</p>
              <div className="row wrap">
                <button onClick={() => exportSimple('dxf')}>DXF líneas simples</button>
                <button onClick={() => exportSimple('pdf')}>PDF líneas simples</button>
              </div>
              <p className="hint">Dibujo original completo sin las capas de mobiliario ({drawing.layers.filter((l) => roles[l.name] === 'furniture').map((l) => l.name).join(', ') || 'ninguna'}):</p>
              <div className="row wrap">
                <button className="ghost" onClick={exportCleanDxf}>DXF completo sin mobiliario</button>
                <button className="ghost" onClick={() => exportPdf({ clean: true, full: true })}>PDF completo sin mobiliario</button>
              </div>
            </>
          )}
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
    </div>
  );
}
