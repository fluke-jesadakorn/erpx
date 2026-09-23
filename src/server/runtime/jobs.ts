import { drainProvisioning } from "../technology";
import { drainPeopleLifecycle } from "../people-lifecycle-jobs";
import { drainDirectoryOperations } from "../directory-operations";
import { drainDirectoryJobs } from "../iam-directories";
import type { PostgresDatabase } from "./postgres";
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
export async function directoryWork(env: JobEnv) {
  const control=env.CONTROL_DB;
  if(control){const delivery=await import("../account-invitation-delivery");for(const slot of await activeSlots(env))await delivery.promoteAccountInvitationDeliveries(control,registeredResources(env,slot.id).write,slot.id);await delivery.deliverAccountInvitation(control,env);await (await import("../account-recovery")).deliverAccountRecovery(control,env as JobEnv&{BETTER_AUTH_SECRET:string});}
  if(env.CONTROL_DB) await drainDirectoryJobs(env.CONTROL_DB as PostgresDatabase);
  for(const slot of await activeSlots(env)) {
    const db=registeredResources(env,slot.id).write as PostgresDatabase;
    await drainDirectoryJobs(db);
    if (!slot.beta) { await drainDirectoryOperations(db); await drainProvisioning(db); }
    await (await import("../user-management")).managementWork(db);
  }
}
export async function posQueueWork(env: JobEnv) {
 for(const slot of await activeSlots(env))await (await import("../pos-stripe")).drainPos(registeredResources(env,slot.id).write as PostgresDatabase,slot.id);
}
export async function scheduledWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) if (!slot.beta) await drainPeopleLifecycle(registeredResources(env,slot.id).write as PostgresDatabase);
  for (const slot of await activeSlots(env)) {
    const { write,files } = registeredResources(env,slot.id);
    await (await import("../leave-self-service")).runLeaveAccruals(write,new Date().toISOString().slice(0,10));
    await (await import("../inventory-reservation-alerts")).escalateInventoryReservations(write);
    await (await import("../quality-alerts")).drainQualityAlerts(write);
    await (await import("../iam-access-review-alerts")).escalateAccessReviews(write);
    await (await import("../staff-task-alerts")).escalateStaffTasks(write);
    await generateOccurrences(write);
    await drainRequestAi(write,env);
    if(process.env.NODE_ENV!=='production') {
      await (await import("../executive/generation")).drainExecutiveGeneration(write,env);
      await drainSocialAgent(write,env);
    }
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

export async function billingWork(env: JobEnv) {
  if(!env.CONTROL_DB)return;
  await (await import("../billing-jobs")).drainBilling(env as import("../billing-jobs").BillingJobEnv);
  await env.CONTROL_DB.prepare("INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES('billing',?,'{}') ON CONFLICT(service) DO UPDATE SET heartbeat_at=excluded.heartbeat_at").bind(Date.now()).run();
}

export async function pmsWork(env: JobEnv) {
 for (const slot of await activeSlots(env)) if (!slot.beta) {
   const db=registeredResources(env,slot.id).write as PostgresDatabase;
   await (await import("../pms")).drainPms(db);
   if(process.env.NODE_ENV!=='production') {
     const booking=await import("../pms-booking");
     await booking.deliverBookingEmails(db,{secret: String(env.BETTER_AUTH_SECRET),origin: String(env.ERPX_PUBLIC_ORIGIN),apiKey:typeof env.RESEND_API_KEY === "string" ? env.RESEND_API_KEY : undefined,from:typeof env.RESEND_FROM === "string" ? env.RESEND_FROM : undefined});
   }
 }

}

export async function legalWork(env:JobEnv){if(process.env.NODE_ENV==='production')return;for(const slot of await activeSlots(env)){const {write,files}=registeredResources(env,slot.id);await Promise.allSettled([(await import('../legal/jobs')).legalReminders(write),(await import('../legal/invitations')).drainLegalInvitations(write,env),(await import('../legal/processing')).drainLegalProcessing(write,files),(await import('../legal/retention')).drainLegalDestruction(write as PostgresDatabase,files),(await import('../legal/jobs')).drainLegalAi(write,env)]).then(results=>{if(results.some(r=>r.status==='rejected'))throw new Error('LEGAL_WORK_FAILED')})}}

export async function supportWork(env:JobEnv) {
 if(process.env.NODE_ENV==='production')return;
 for(const slot of await activeSlots(env)) await (await import('../support-sla-alerts')).drainSupportSla(registeredResources(env,slot.id).write);
}

export async function customerNotificationWork(env:JobEnv) {
 if(process.env.NODE_ENV==='production')return;
 for(const slot of await activeSlots(env)) await (await import('../sales-customer-notifications')).drainCustomerNotifications(registeredResources(env,slot.id).write,env);
}
