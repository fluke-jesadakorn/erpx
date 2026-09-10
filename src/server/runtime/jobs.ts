import { drainSocialAgent } from "../social-agent";
import { randomUUID } from 'node:crypto';
import { access } from 'node:fs/promises';
import { join } from 'node:path';
import { resourceSlots, registeredResources } from '../../lib/holding-registry';
import { project, type EventMessage, type JobEnv } from '../job-projector';
import { generateOccurrences } from '../routines';
import { drainExpenseAi } from '../expense-ai';
import { drainRequestAi } from '../request-ai';
import { cleanupBetaResources } from '../beta-maintenance';
import type { SqlDatabase } from './types';

export type PendingEvent = { id:string; event_type:string; aggregate_id:string; aggregate_version:number; payload_json:string; attempts:number; lease_token:string };
export async function claimJob(db: SqlDatabase, now = Date.now()): Promise<PendingEvent | null> {
  return db.prepare(`UPDATE outbox_events SET lease_token=?,claim_until=?,attempts=attempts+1
    WHERE id=(SELECT id FROM outbox_events WHERE processed_at IS NULL AND dead_letter_at IS NULL
      AND next_attempt_at<=? AND (claim_until IS NULL OR claim_until<?)
      ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1)
    RETURNING id,event_type,aggregate_id,aggregate_version,payload_json,attempts,lease_token`)
    .bind(randomUUID(),now+90_000,now,now).first<PendingEvent>();
}
export async function runOne(env: JobEnv, holdingId: string, handler = project): Promise<boolean> {
  const { write } = registeredResources(env, holdingId), row = await claimJob(write);
  if (!row) return false;
  const renewal = setInterval(() => {
    void write.prepare('UPDATE outbox_events SET claim_until=? WHERE id=? AND lease_token=? AND processed_at IS NULL')
      .bind(Date.now()+90_000,row.id,row.lease_token).run().catch(() => {});
  },15_000);
  try {
    const message: EventMessage = { holdingId,id:row.id,eventType:row.event_type,aggregateId:row.aggregate_id,aggregateVersion:row.aggregate_version,payload:JSON.parse(row.payload_json) };
    await handler(env,message);
    await write.prepare('UPDATE outbox_events SET processed_at=?,published_at=?,claim_until=NULL,lease_token=NULL,last_error=NULL WHERE id=? AND lease_token=?')
      .bind(Date.now(),Date.now(),row.id,row.lease_token).run();
  } catch (error) {
    const code = (error as { code?: string; name?: string }).code ?? (error instanceof Error ? error.name : 'JobError');
    await write.prepare('UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=?,last_error=?,dead_letter_at=? WHERE id=? AND lease_token=?')
      .bind(Date.now()+Math.min(60_000,1000*2**Math.min(row.attempts,6)),String(code).slice(0,64),row.attempts>=6?Date.now():null,row.id,row.lease_token).run();
    console.error(JSON.stringify({event:'job_failed',holdingId,eventId:row.id,attempt:row.attempts,code}));
  } finally { clearInterval(renewal); }
  return true;
}
export async function activeSlots(env: JobEnv) {
  const active=[];
  for (const slot of resourceSlots(env)) if (!slot.beta || await env.CONTROL_DB?.prepare("SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'").bind(slot.id).first()) active.push(slot);
  return active;
}
export async function scheduledWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) {
    const { write,files } = registeredResources(env,slot.id);
    await generateOccurrences(write);
    await drainRequestAi(write,env);
    await drainSocialAgent(write,env);
    if(slot.beta) await cleanupBetaResources(write,files,slot.id);
  }
}
export async function maintenance(dataDir: string) {
  try { await access(join(dataDir,'maintenance')); return true; }
  catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false; throw error; }
}

export async function expenseAiWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) {
    const { write, files } = registeredResources(env, slot.id);
    await drainExpenseAi(write, files, env);
  }
}
