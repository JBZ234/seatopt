import { isWeb } from '../api';
import { useEffect, useMemo, useRef } from 'react';
import { analyzePlan, deleteArea, fixAllOverlaps, fixOverlap, deleteLayer, exportCleanDxf, exportPdf, exportPlanDxf, exportSimple, importPlan, loadBackground, openProject, setPlanMode, setScale } from '../actions';
import { Fold, Num, Switch } from '../components/ui';
import { areaCenter, areaGroups, checkPlan, ringArea } from '../draw';
import { emptySel, fmtLen, STEP, useApp, type Tool } from '../store';
import type { DoorKind, LayerRole, ObstacleKind, Pt } from '../types';

const ROLE_LABEL: Record<LayerRole, string> = {
  outline: 'Area outline', wall: 'Wall / blocks', column: 'Columns', office: 'Fixed office',
  restricted: 'Area with clearance', reserved: 'Reserved area', door: 'Doors', mainDoor: 'Main entrance',
  exitDoor: 'Emergency exit', officeDoor: 'Office door', window: 'Windows / facade',
  furniture: 'Existing furniture', room: 'Rooms (reference)', building: 'Building (reference)', ignore: 'Ignore',
};
const DOOR_LABEL: Record<DoorKind, string> = { main: 'Main', exit: 'Exit', office: 'Office', ignore: 'Ignored' };
const OBS_LABEL: Record<ObstacleKind, string> = { column: 'Columns', fixedOffice: 'Fixed offices', restricted: 'Areas with clearance', reserved: 'Reserved areas' };
const OBS_ONE: Record<ObstacleKind, string> = { column: 'Column', fixedOffice: 'Fixed office or room', restricted: 'Panel or cabinet (with clearance)', reserved: 'Reserved area' };

type TaskId = 'scale' | 'area' | 'door' | 'obs';
const UNITS: [number, string][] = [[1, 'millimeters'], [10, 'centimeters'], [1000, 'meters'], [25.4, 'inches'], [304.8, 'feet']];

export default function PlanStep() {
  const st = useApp();
  const { drawing, plan, mmPerUnit, roles, background, hiddenLayers, notes } = st;

  const draw = st.planMode === 'draw';
  const groups = useMemo(() => (plan ? areaGroups(plan.region) : []), [plan]);
  const issues = useMemo(() => checkPlan(plan), [plan]);
  const errors = issues.filter((i) => i.level === 'error');
  const areaOk = groups.length > 0 && !!plan && plan.region[groups[0].outer].length >= 3;
  const mainOk = !!plan && plan.doors.some((d) => d.kind === 'main');
  const scaleKnown = !drawing || st.scaleConfirmed || (drawing?.scale.reason ?? '').includes('saved');
  const pending: TaskId | null = !areaOk ? 'area' : !mainOk || !st.mainConfirmed ? 'door' : null;
  const blocker = errors.length ? `Fix ${errors.length === 1 ? 'the problem' : `the ${errors.length} problems`} above to continue.`
    : !draw && !st.mainConfirmed ? 'Confirm the main entrance first.' : '';

  // activar la herramienta de la tarea pendiente (una vez por tarea)
  const lastAuto = useRef<string>('');
  useEffect(() => {
    if (!drawing && !plan) return;
    const key = `${drawing?.drawingId}|${pending}|${st.planMode}`;
    if (lastAuto.current === key) return;
    lastAuto.current = key;
    if (draw) return; // drawing: you pick the tool
    if (pending === 'area' && background) st.set({ tool: 'seed' });
    else if (pending === 'door') st.set({ tool: plan && plan.doors.length > 0 ? 'mainDoor' : 'drawMain' });
    else if (pending === null && ['seed', 'mainDoor', 'drawMain', 'outline'].includes(st.tool)) st.set({ tool: 'select' });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending, drawing?.drawingId, !!background, st.planMode]);

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
  /** A drawing tool with its shape (outline by points or rectangle). */
  const DrawBtn = ({ t, shape, label, primary }: { t: Tool; shape?: 'poly' | 'rect'; label: string; primary?: boolean }) => {
    const on = st.tool === t && (!shape || st.drawShape === shape);
    return (
      <button className={(primary && !on ? 'primary ' : '') + (on ? 'active-tool' : '')} aria-pressed={on}
        onClick={() => st.set(on ? { tool: 'select' } : { tool: t, ...(shape ? { drawShape: shape } : {}), selection: emptySel() })}>{label}</button>
    );
  };
  const doorsBy = (k: DoorKind) => plan?.doors.filter((d) => d.kind === k).length ?? 0;
  const obsBy = (k: ObstacleKind) => plan?.obstacles.filter((o) => o.kind === k).length ?? 0;
  const show = (p?: Pt) => { if (p) st.set({ focus: { p, n: Date.now() } }); };

  const Task = ({ id, n, title, done, warn, optional, children }: { id: TaskId; n: number; title: string; done: boolean; warn?: boolean; optional?: boolean; children: React.ReactNode }) => (
    <li className={'task' + (done ? ' done' : '') + (pending === id || (warn && done && id !== 'scale' && pending === null) ? ' current' : '') + (warn ? ' unsure' : '') + (optional && !done ? ' optional' : '')}>
      <span className="task-mark" aria-hidden>{done ? (warn ? '!' : '✓') : n}</span>
      <div className="task-body">
        <div className="task-title">{title}</div>
        {children}
      </div>
    </li>
  );

  const canDetect = !!drawing?.drawingId;
  const scaleN = mmPerUnit / (st.unitMm || 1);
  const scaleTask = drawing && (
    <Task id="scale" n={1} title="Scale" done warn={!scaleKnown}>
      <div className="field-row">
        {isPdf ? (
          <label>1 :
            <Num value={Math.round(paper ?? 100)} unit="" width={60} label="Paper scale" onChange={(v) => { st.set({ scaleConfirmed: true }); setScale(v * 25.4 / 72); }} />
          </label>
        ) : (
          <>
            <label>Units
              <select aria-label="Drawing units" value={UNITS.some(([v]) => v === st.unitMm) ? st.unitMm : 1} onChange={(e) => { const u = parseFloat(e.target.value); st.set({ unitMm: u, scaleConfirmed: true }); setScale(u * scaleN); }}>
                {UNITS.map(([v, n]) => <option key={v} value={v}>{n}</option>)}
              </select>
            </label>
            <label title="Real size per drawing unit: 1:1 when the plan is drawn at real size (the usual in AutoCAD model space)">Scale 1 :
              <Num value={Math.round(scaleN * 10000) / 10000} unit="" width={64} label="Scale" onChange={(v) => { if (v > 0) { st.set({ scaleConfirmed: true }); setScale(st.unitMm * v); } }} />
            </label>
          </>
        )}
        <ChipTool t="calibrate" label="Calibrate with a known length" />
        {!scaleKnown && <button className="ghost small" onClick={() => st.set({ scaleConfirmed: true })}>It is correct</button>}
      </div>
      <p className="hint">{isPdf
        ? (scaleKnown ? 'Scale confirmed.' : drawing?.scale.reason ?? '')
        : `1 drawing unit = ${fmtLen(mmPerUnit, mmPerUnit >= 1000 ? 'm' : mmPerUnit >= 10 ? 'cm' : 'mm')}. ${drawing?.declaredMm ? `The file says ${UNITS.find(([v]) => v === drawing.declaredMm)?.[1] ?? 'its units'}.` : 'The file does not say its units.'}`}</p>
      {!isPdf && st.scaleHint && Math.abs(st.scaleHint.mmPerUnit / mmPerUnit - 1) > 0.05 && (() => {
        const h = st.scaleHint!;
        const u = UNITS.find(([v]) => Math.abs(h.mmPerUnit / v - 1) < 0.02);
        return (
          <div className="scale-hint">
            <span>The doors in the drawing measure as if it were drawn in {u ? u[1] : `1:${(h.mmPerUnit / st.unitMm).toFixed(2)}`}.</span>
            <button className="small" onClick={() => { if (u) st.set({ unitMm: u[0] }); st.set({ scaleConfirmed: true }); setScale(h.mmPerUnit); }}>Use {u ? `${u[1]} 1:1` : 'it'}</button>
          </div>
        );
      })()}
    </Task>
  );
  const off = drawing ? 1 : 0; // task numbers without the scale

  const allNames = background?.map((l) => l.name) ?? [];
  const shownCount = allNames.filter((n) => !hiddenLayers.includes(n)).length;
  const layersFold = drawing && drawing.layers.length > 0 && (
    <Fold key={draw ? 'ld' : 'lt'} open={draw} title={`Drawing layers (${shownCount}/${allNames.length || drawing.layers.length} shown)`}
      right={background && <span className="row">
        <button className="ghost small" onClick={() => st.set({ hiddenLayers: [] })}>All</button>
        <button className="ghost small" onClick={() => st.set({ hiddenLayers: allNames })}>None</button>
      </span>}>
      <p className="hint">{draw ? 'The drawing as it is in the file, to trace over. Turn layers off to see better. To hide single lines: with Select, click a line (or drag a window over several) and press H.' : 'If the area comes out wrong, check which layers are walls and which are ignored, and detect again.'}</p>
      {st.hiddenPaths.length > 0 && <div className="row"><button className="small" onClick={() => st.set({ hiddenPaths: [] })}>Show {st.hiddenPaths.length} hidden line{st.hiddenPaths.length > 1 ? 's' : ''}</button></div>}
      <table className="layers layer-sw">
        <tbody>
          {drawing.layers.map((l) => {
            const visible = !hiddenLayers.includes(l.name);
            const has = allNames.includes(l.name);
            return (
              <tr key={l.name} className={visible ? '' : 'off'}>
                <td><Switch small on={visible && has} disabled={!has} label={(visible ? 'Hide ' : 'Show ') + l.name}
                  onChange={(v) => st.set({ hiddenLayers: v ? hiddenLayers.filter((h) => h !== l.name) : [...hiddenLayers, l.name] })} /></td>
                <td><i className="dot" style={{ background: l.color }} /></td>
                <td className="lname" title={`${l.name} · ${l.paths} items`}>{l.name}</td>
                <td>
                  <select value={roles[l.name]} aria-label={`Role of ${l.name}`} title="What the layer is (used to detect automatically and for the exports)" onChange={(e) => st.set({ roles: { ...roles, [l.name]: e.target.value as LayerRole } })}>
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
      {!draw && (
        <>
          <div className="row">
            <button onClick={async () => { await loadBackground(); await analyzePlan(null); }}>Apply and detect again</button>
          </div>
          <div className="field-row">
            <label title="Openings of this width or less (door openings) are closed to separate the area from other spaces.">Close openings up to
              <Num value={st.gapClose} unit="mm" label="Close openings" onChange={(v) => st.set({ gapClose: v })} />
            </label>
          </div>
        </>
      )}
      {background && (
        <label className="check"><input type="checkbox" checked={st.showBackground} onChange={(e) => st.set({ showBackground: e.target.checked })} /> Show the background drawing</label>
      )}
    </Fold>
  );


  return (
    <div className="step-body">
      <div className="file-line">
        <strong title={drawing?.name}>{drawing?.name ?? 'Blank plan'}</strong>
      </div>

      {draw && plan && (plan.region.length > 0 || plan.doors.length > 0) && (
        <div className="mode-row"><button className="ghost small" onClick={() => setPlanMode('draw', true)}>Start over</button></div>
      )}

      {draw ? (
        <ol className="tasks">
          {scaleTask}
          <Task id="area" n={1 + off} title="Areas to furnish" done={areaOk}>
            {areaOk
              ? <p className="stat-line"><span className="big">{st.areaM2.toFixed(0)}</span> m²{groups.length > 1 ? ` in ${groups.length} areas` : ''}</p>
              : <p className="hint">Draw the outline of each area you will furnish: click its corners on the walls of the drawing (they snap to the lines), then Enter or click the first point.</p>}
            <div className="tool-row">
              <DrawBtn t="drawArea" shape="poly" label="Draw area" primary={!areaOk} />
              <DrawBtn t="drawArea" shape="rect" label="Rectangle" />
              {canDetect && <button className="ghost" title="Look for the area in the lines of the drawing (replaces what you drew)" onClick={() => setPlanMode('detect')}>Detect automatically</button>}
            </div>
            {groups.length > 0 && (
              <ul className="area-list">
                {groups.map((g, gi) => {
                  const r = plan!.region[g.outer];
                  const a = (ringArea(r) - g.holes.reduce((x, h) => x + ringArea(plan!.region[h]), 0)) / 1e6;
                  return (
                    <li key={g.outer}>
                      <span className="n">Area {gi + 1} · {a.toFixed(1)} m²</span>
                      <button className="ghost small" onClick={() => { show(areaCenter(r)); st.set({ selection: { ...emptySel(), region: [g.outer] }, tool: 'select' }); }}>Edit</button>
                      <button className="ghost small danger icon-btn" title={`Delete area ${gi + 1}`} aria-label={`Delete area ${gi + 1}`} onClick={() => deleteArea(g.outer)}>
                        <svg viewBox="0 0 16 16" aria-hidden><path d="M3 4h10M6 4V2.5h4V4M4.5 4l.7 9.5h5.6l.7-9.5" /></svg>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            {groups.length > 0 && <p className="hint small">Outlines that overlap are joined into one area; areas that only touch stay separate and people walk between them. To fix one: Edit, then drag its corners (double-click an edge adds a corner).</p>}
          </Task>

          <Task id="door" n={2 + off} title="Doors" done={mainOk}>
            <p className="hint">{mainOk
              ? `${doorsBy('main')} main entrance · ${doorsBy('office')} doors · ${doorsBy('exit')} exits. Draw every door of the areas: offices, rooms and exits.`
              : 'Draw the main entrance on the edge of an area: click one end of the opening and then the other (or type its width).'}</p>
            <div className="tool-row">
              <DrawBtn t="drawMain" label="Main entrance" primary={areaOk && !mainOk} />
              <DrawBtn t="drawDoor" label="Door" />
              <DrawBtn t="drawExit" label="Exit" />
              <ChipTool t="door" label="Change type" />
            </div>
            <div className="legend">
              {(['main', 'office', 'exit'] as DoorKind[]).map((k) => (
                <span key={k}><i className={'sw door-' + k} />{DOOR_LABEL[k]} {doorsBy(k)}</span>
              ))}
            </div>
          </Task>

          <Task id="obs" n={3 + off} title="Obstacles (optional)" done={(plan?.obstacles.length ?? 0) > 0} optional>
            <p className="hint">Columns, fixed rooms, panels or anything inside the areas where desks cannot go.</p>
            <div className="tool-row">
              <select aria-label="Kind of obstacle" value={st.obsKind} onChange={(e) => st.set({ obsKind: e.target.value as ObstacleKind })}>
                {(['column', 'fixedOffice', 'restricted', 'reserved'] as ObstacleKind[]).map((k) => <option key={k} value={k}>{OBS_ONE[k]}</option>)}
              </select>
              <DrawBtn t="drawObs" shape="rect" label="Rectangle" />
              <DrawBtn t="drawObs" shape="poly" label="Outline" />
            </div>
            {(plan?.obstacles.length ?? 0) > 0 && (
              <div className="legend">
                {(['column', 'fixedOffice', 'restricted', 'reserved'] as ObstacleKind[]).filter((k) => obsBy(k)).map((k) => (
                  <span key={k}><i className={'sw ob-' + k} />{OBS_LABEL[k]} {obsBy(k)}</span>
                ))}
              </div>
            )}
          </Task>
          <li className="task-tools">
            <ChipTool t="erase" label="Erase" />
            <span className="hint small">Click an area, door or obstacle to delete it · Ctrl+Z undoes</span>
          </li>
        </ol>
      ) : (
        <ol className="tasks">
          {scaleTask}
          <Task id="area" n={1 + off} title="Area to furnish" done={areaOk}>
            {areaOk ? (
              <p className="stat-line"><span className="big">{st.areaM2.toFixed(0)}</span> m²{plan!.region.length > 1 ? ` in ${plan!.region.length} areas` : ''} · {plan!.obstacles.length} obstacles · {plan!.doors.length} doors</p>
            ) : (
              <p className="hint">Click inside the part of the office to furnish. If it is not found properly, draw it by hand.</p>
            )}
            {areaOk && <p className="hint small">Only some rooms? Click inside one with “Choose another area”, then “+ Add area” for each of the others. Rooms that touch through a door are joined.</p>}
            <div className="row wrap">
              <ToolBtn t="seed" label={areaOk ? 'Choose another area' : 'Click inside the area'} primary={!areaOk} />
              {areaOk && canDetect && <ToolBtn t="seedAdd" label="+ Add area" />}
              {areaOk && canDetect && (st.areaSeeds.length > 1 || plan!.region.length > 1) && <ToolBtn t="seedRemove" label="− Remove area" />}
              {canDetect && <button className="ghost" onClick={() => analyzePlan(null)}>Detect again</button>}
              <button className="ghost" onClick={() => setPlanMode('draw')}>Fix it by hand</button>
              <button className="ghost" onClick={() => setPlanMode('draw', true)}>Back to drawing (empty)</button>
            </div>
          </Task>

          <Task id="door" n={2 + off} title="Main entrance" done={mainOk} warn={mainOk && !st.mainConfirmed}>
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
              <p className="hint">{plan && plan.doors.length > 0 ? `${plan.doors.length} doors were found. Click the staff entrance.` : 'No doors were found: draw the entrance door.'}</p>
            )}
            {areaOk && (
              <div className="row wrap">
                {plan!.doors.length > 0 && <ToolBtn t="mainDoor" label={mainOk ? 'Change main entrance' : 'Mark it on the plan'} primary={!mainOk && plan!.doors.length > 0} />}
                <ToolBtn t="drawMain" label="Draw it" primary={!mainOk && plan!.doors.length === 0} />
              </div>
            )}
          </Task>
        </ol>
      )}

      {draw && layersFold}

      <section className="check-sec">
        <div className="space-head">
          <h3 className="section-label">Check</h3>
          {issues.filter((i) => i.overlap).length > 1 && <button className="small" onClick={fixAllOverlaps}>Trim all overlaps</button>}
        </div>
        {issues.length === 0 ? (
          <ul className="checks ok"><li><span className="ico">✓</span><span>Areas, doors and obstacles are valid.</span></li></ul>
        ) : (
          <ul className="checks">
            {issues.map((i, k) => (
              <li key={k + i.text} className={i.level}>
                <span className="ico" aria-label={i.level === 'error' ? 'Error' : 'Warning'}>{i.level === 'error' ? '×' : '!'}</span>
                <span>{i.text}
                  {i.overlap && (
                    <span className="fix-row">
                      <button className="small" onClick={() => fixOverlap(i.overlap!.keep, i.overlap!.cut, 'trim')}>Trim {i.overlap.cutName}</button>
                      <button className="small ghost" onClick={() => fixOverlap(i.overlap!.keep, i.overlap!.cut, 'join')}>Join them</button>
                    </span>
                  )}
                </span>
                {i.at ? <button className="ghost small" onClick={() => show(i.at)}>Show</button> : <span />}
              </li>
            ))}
          </ul>
        )}
      </section>

      <div className="next">
        <button className="primary" disabled={!!blocker} onClick={() => st.set({ step: STEP.paths, tool: 'select', selection: emptySel() })}>Continue: circulation</button>
        {blocker && <p className="why">{blocker}</p>}
      </div>

      <h3 className="section-label">Optional settings</h3>

      {plan && !draw && (
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

      {plan && !draw && (
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

      {!draw && layersFold}

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
