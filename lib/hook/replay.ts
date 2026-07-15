import 'server-only';
import { bumpHookEventReplay, loadHookEvent, markHookEventProcessed } from './persist';

export type ReplayOutcome =
  | { ok: true; action: 'replayed' | 'processed'; id: number; eventType: string }
  | { ok: false; reason: 'not_found' | 'not_signed' | 'not_replayable'; id?: number };

export async function replayHookEvent(id: number, actor: string): Promise<ReplayOutcome> {
  const row = await loadHookEvent(id);
  if (!row) return { ok: false, reason: 'not_found' };
  if (!row.signature_ok) return { ok: false, reason: 'not_signed', id };
  if (row.status === 'rejected') return { ok: false, reason: 'not_replayable', id };
  await bumpHookEventReplay(id);
  await markHookEventProcessed(id, actor);
  return { ok: true, action: 'replayed', id, eventType: row.event_type };
}