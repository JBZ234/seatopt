// Meeting table with n seats: chairs on both long sides and, if wanted, one at each end.
import type { CustomSeat, DeskType } from './types';

const PITCH = 700; // from chair to chair along the table
const CHAIR = 500; // depth of a chair (as the Studio draws them)

export function meetingTable(n: number, ends: boolean, id: string): DeskType & { parts: { x: number; y: number; w: number; h: number }[] } {
  n = Math.max(2, Math.min(40, Math.round(n)));
  const endSeats = ends && n >= 4 ? 2 : 0;
  const long = n - endSeats;
  const top = Math.ceil(long / 2), bottom = long - top;
  const L = Math.max(top, 1) * PITCH + 300;
  const D = n <= 6 ? 900 : n <= 10 ? 1100 : n <= 16 ? 1300 : 1500;
  const seats: CustomSeat[] = [];
  const row = (k: number, y: number, fy: number) => {
    for (let i = 0; i < k; i++) seats.push({ x: (i - (k - 1) / 2) * PITCH, y, facing: { x: 0, y: fy } });
  };
  row(top, D / 2 + CHAIR / 2, -1);
  row(bottom, -D / 2 - CHAIR / 2, 1);
  if (endSeats) {
    seats.push({ x: -L / 2 - CHAIR / 2, y: 0, facing: { x: 1, y: 0 } });
    seats.push({ x: L / 2 + CHAIR / 2, y: 0, facing: { x: -1, y: 0 } });
  }
  return {
    id, name: `Meeting table · ${n} seats`, color: '#9a7bb5', shape: 'custom',
    seatWidth: L, depth: D, sides: 2, perSide: 1, flexible: false, minPerSide: 1, maxPerSide: 1,
    returnLength: 0, returnDepth: 0,
    // room for the chairs at the ends (chair + pushing it back)
    clearChair: null, clearBack: 0, clearLeft: endSeats ? CHAIR + 300 : 0, clearRight: endSeats ? CHAIR + 300 : 0,
    body: [{ x: -L / 2, y: -D / 2 }, { x: L / 2, y: -D / 2 }, { x: L / 2, y: D / 2 }, { x: -L / 2, y: D / 2 }],
    seats,
    parts: [{ x: -L / 2, y: -D / 2, w: L, h: D }],
  };
}
