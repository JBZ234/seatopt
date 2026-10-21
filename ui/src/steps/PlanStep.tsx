import { isWeb } from '../api';
import { useEffect, useRef } from 'react';
import { analyzePlan, deleteLayer, exportCleanDxf, exportPdf, exportPlanDxf, exportSimple, importPlan, loadBackground, openProject, setScale } from '../actions';
import { Fold, Num } from '../components/ui';
import { STEP, useApp, type Tool } from '../store';
import type { DoorKind, LayerRole, ObstacleKind } from '../types';

const ROLE_LABEL: Record<LayerRole, string> = {
  outline: 'Area outline', wall: 'Wall / blocks', column: 'Columns', office: 'Fixed office',
  restricted: 'Area with clearance', reserved: 'Reserved area', door: 'Doors', mainDoor: 'Main entrance',
  exitDoor: 'Emergency exit', officeDoor: 'Office door', window: 'Windows / facade',
  furniture: 'Existing furniture', room: 'Rooms (reference)', building: 'Building (reference)', ignore: 'Ignore',
};
const DOOR_LABEL: Record<DoorKind, string> = { main: 'Main', exit: 'Exit', office: 'Office', ignore: 'Ignored' };
const OBS_LABEL: Record<ObstacleKind, string> = { column: 'Columns', fixedOffice: 'Fixed offices', restricted: 'Areas with clearance', reserved: 'Reserved areas' };

type TaskId = 'scale' | 'area' | 'door';

export default function PlanStep() {
  const st = useApp();
  const { drawing, plan, mmPerUnit, roles, background, hiddenLayers, notes } = st;

  const areaOk = !!plan && plan.region.length > 0 && plan.region[0].length >= 3;
  const mainOk = !!plan && plan.doors.some((d) => d.kind === 'main');
  const scaleKnown = st.scaleConfirmed || (drawing?.scale.reason ?? '').includes('saved') || (drawing?.source !== 'pdf' && (drawing?.unitNote ?? '').startsWith('File units'));
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
        <h2>Start with the plan</h2>
        <p>Open the office floor plan as DXF, DWG or PDF. Then the app guides you in three steps: confirm the scale, mark the area to furnish and point out the main entrance.</p>
        <div className="row">
          <button className="primary" onClick={importPlan}>Open plan</button>
          <button onClick={openProject}>Open saved project</button>
        </div>
        {isWeb ? <p className="hint">Web version: open DXF or vector PDF. Save DWG files as DXF first.</p> : <>
        <p className="hint">DWG files are converted with ODA File Converter (free) or LibreDWG. If it is installed elsewhere, enter the path under “Settings”.</p>
        <Fold title="Settings">
          <label className="stack">Path to ODA File Converter or dwg2dxf (optional)
            <input value={st.converter} placeholder="C:\Program Files\ODA\ODAFileConverter 25.x\ODAFileConverter.exe" onChange={(e) => st.set({ converter: e.target.value })} />
          </label>
        </Fold>
        </>}
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
        <strong title={drawing?.name}>{drawing?.name ?? 'Project'}</strong>
      </div>

      <ol className="tasks">
        <Task id="scale" n={1} title="Scale" done warn={!scaleKnown}>
          <div className="field-row">
            {isPdf ? (
              <label>1 :
                <Num value={Math.round(paper ?? 100)} unit="" width={60} label="Paper scale" onChange={(v) => { st.set({ scaleConfirmed: true }); setScale(v * 25.4 / 72); }} />
              </label>
            ) : (
              <select aria-label="Drawing unit" value={mmPerUnit} onChange={(e) => { st.set({ scaleConfirmed: true }); setScale(parseFloat(e.target.value)); }}>
                <option value={1}>millimeters</option>
                <option value={10}>centimeters</option>
                <option value={1000}>meters</option>
                <option value={25.4}>inches</option>
                <option value={304.8}>feet</option>
                {![1, 10, 1000, 25.4, 304.8].includes(mmPerUnit) && <option value={mmPerUnit}>calibrated</option>}
              </select>
            )}
            <ChipTool t="calibrate" label="Calibrate with a known length" />
            {!scaleKnown && <button className="ghost small" onClick={() => st.set({ scaleConfirmed: true })}>It is correct</button>}
          </div>
          <p className="hint">{scaleKnown ? (drawing?.scale.reason.includes('saved') ? drawing.scale.reason : 'Scale confirmed.') : `${drawing?.scale.reason ?? ''}`}</p>
        </Task>

        <Task id="area" n={2} title="Area to furnish" done={areaOk}>
          {areaOk ? (
            <p className="stat-line"><span className="big">{st.areaM2.toFixed(0)}</span> m² · {plan!.obstacles.length} obstacles · {plan!.rooms?.length ?? 0} rooms</p>
          ) : (
            <p className="hint">Click inside the part of the office to furnish. If it is not closed properly, draw its outline.</p>
          )}
          <div className="row wrap">
            <ToolBtn t="seed" label={areaOk ? 'Choose another area' : 'Click inside the area'} primary={!areaOk} />
            <ToolBtn t="outline" label="Draw outline" />
            {drawing?.drawingId && <button className="ghost" onClick={() => analyzePlan(null)}>Detect automatically</button>}
          </div>
        </Task>

        <Task id="door" n={3} title="Main entrance" done={mainOk} warn={mainOk && !st.mainConfirmed}>
          {mainOk && !st.mainConfirmed ? (
            <>
              <p className="hint">The widest door was chosen (green on the plan). Is it the staff entrance?</p>
              <div className="row wrap">
                <button className="primary" onClick={() => st.set({ mainConfirmed: true })}>Yes, that one</button>
              </div>
            </>
          ) : mainOk ? (
            <p className="hint">Done. You can also mark the emergency exits (optional, below).</p>
          ) : (
            <p className="hint">{plan && plan.doors.length > 0 ? `${plan.doors.length} doors were found. Click the staff entrance.` : 'No doors were found: drag across the opening of the entrance door.'}</p>
          )}
          {areaOk && (
            <div className="row wrap">
              {plan!.doors.length > 0 && <ToolBtn t="mainDoor" label={mainOk ? 'Change main entrance' : 'Mark it on the plan'} primary={!mainOk && plan!.doors.length > 0} />}
              <ToolBtn t="drawDoor" label="Draw door" primary={!mainOk && plan!.doors.length === 0} />
            </div>
          )}
        </Task>
      </ol>

      {areaOk && mainOk && (
        <div className="next">
          <button className="primary" onClick={() => st.set({ step: STEP.paths, tool: 'select' })}>Continue: circulation</button>
        </div>
      )}

      <h3 className="section-label">Optional settings</h3>

      {plan && (
        <Fold title={`Doors and exits (${plan.doors.length})`}>
          <div className="row wrap">
            <ChipTool t="exitDoor" label="Mark / unmark exit" />
            <ChipTool t="door" label="Change type" />
            <ChipTool t="drawDoor" label="Draw door" />
            <ChipTool t="erase" label="Delete" />
          </div>
          <div className="legend">
            {(['main', 'exit', 'office', 'ignore'] as DoorKind[]).map((k) => (
              <span key={k}><i className={'sw door-' + k} />{DOOR_LABEL[k]} {doorsBy(k)}</span>
            ))}
          </div>
        </Fold>
      )}

      {plan && (
        <Fold title="Obstacles and areas">
          <p className="hint">Drag on the plan to draw; corners snap to the drawing.</p>
          <div className="row wrap">
            <ChipTool t="column" label="Column" />
            <ChipTool t="fixedOffice" label="Fixed office" />
            <ChipTool t="restricted" label="Panel / cabinet" />
            <ChipTool t="reserved" label="Reserved area" />
            <ChipTool t="window" label="Window" />
            <ChipTool t="erase" label="Delete" />
          </div>
          <div className="legend">
            {(['column', 'fixedOffice', 'restricted', 'reserved'] as ObstacleKind[]).map((k) => (
              <span key={k}><i className={'sw ob-' + k} />{OBS_LABEL[k]} {obsBy(k)}</span>
            ))}
          </div>
        </Fold>
      )}

      {drawing && drawing.layers.length > 0 && (
        <Fold title={`Drawing layers (${drawing.layers.length})`}>
          <p className="hint">If the area comes out wrong, check which layers are walls and which are ignored, and detect again.</p>
          <table className="layers">
            <tbody>
              {drawing.layers.map((l) => {
                const visible = !hiddenLayers.includes(l.name);
                return (
                  <tr key={l.name}>
                    <td>
                      <button className="eye" aria-label={visible ? 'Hide layer' : 'Show layer'} aria-pressed={visible}
                        onClick={() => st.set({ hiddenLayers: visible ? [...hiddenLayers, l.name] : hiddenLayers.filter((h) => h !== l.name) })}>
                        <i style={{ background: visible ? l.color : 'transparent', borderColor: l.color }} />
                      </button>
                    </td>
                    <td className="lname" title={`${l.name} · ${l.paths} strokes`}>{l.name}</td>
                    <td>
                      <select value={roles[l.name]} onChange={(e) => st.set({ roles: { ...roles, [l.name]: e.target.value as LayerRole } })}>
                        {Object.entries(ROLE_LABEL).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
                      </select>
                    </td>
                    <td>
                      <button className="ghost small danger icon-btn" title={`Delete all lines and objects of “${l.name}”`} aria-label={`Delete layer ${l.name}`} onClick={() => deleteLayer(l.name)}>
                        <svg viewBox="0 0 16 16" aria-hidden><path d="M3 4h10M6 4V2.5h4V4M4.5 4l.7 9.5h5.6l.7-9.5" /></svg>
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="row">
            <button onClick={async () => { await loadBackground(); await analyzePlan(null); }}>Apply and detect again</button>
          </div>
          <div className="field-row">
            <label title="Openings of this width or less (door openings) are closed to separate the area from other spaces.">Close openings up to
              <Num value={st.gapClose} unit="mm" label="Close openings" onChange={(v) => st.set({ gapClose: v })} />
            </label>
          </div>
          {background && (
            <label className="check"><input type="checkbox" checked={st.showBackground} onChange={(e) => st.set({ showBackground: e.target.checked })} /> Show the background drawing</label>
          )}
        </Fold>
      )}

      {plan && areaOk && (
        <Fold title="Export base plan">
          <p className="hint">Minimal plan for optimizing: outline, offices and rooms, columns and doors, on standard layers. It imports again with no adjustments (the PDF keeps the scale).</p>
          <div className="row wrap">
            <button onClick={exportPlanDxf}>Simplified DXF</button>
            <button onClick={() => exportPdf({ clean: true })}>Simplified PDF</button>
          </div>
          {drawing?.drawingId && (
            <>
              <p className="hint">Original drawing in single lines: each wall as one line, plus columns, doors and offices. No furniture, text or details.</p>
              <div className="row wrap">
                <button onClick={() => exportSimple('dxf')}>Single-line DXF</button>
                <button onClick={() => exportSimple('pdf')}>Single-line PDF</button>
              </div>
              <p className="hint">Full original drawing without the furniture layers ({drawing.layers.filter((l) => roles[l.name] === 'furniture').map((l) => l.name).join(', ') || 'none'}):</p>
              <div className="row wrap">
                <button className="ghost" onClick={exportCleanDxf}>Full DXF without furniture</button>
                <button className="ghost" onClick={() => exportPdf({ clean: true, full: true })}>Full PDF without furniture</button>
              </div>
            </>
          )}
        </Fold>
      )}

      {notes.length > 0 && (
        <Fold title="Import notes">
          <ul className="notes">{notes.map((n, i) => <li key={i}>{n}</li>)}</ul>
        </Fold>
      )}

      <Fold title="Settings">
        <label className="stack">Path to ODA File Converter or dwg2dxf (optional)
          <input value={st.converter} placeholder="C:\Program Files\ODA\ODAFileConverter 25.x\ODAFileConverter.exe" onChange={(e) => st.set({ converter: e.target.value })} />
        </label>
      </Fold>
    </div>
  );
}
