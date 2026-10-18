import { applicationOrigin } from "@/lib/runtime/application-origins";
import { drainProvisioning } from "@/server/modules/identity_access/public/technology";
import { drainPeopleLifecycle } from "@/server/modules/people_payroll/public/people-lifecycle-jobs";
import { drainDirectoryOperations } from "@/server/modules/identity_access/public/directory-operations";
import { drainPositionGpo } from "@/server/modules/identity_access/public/position-gpo";
import { drainDirectoryJobs } from "@/server/modules/identity_access/public/iam-directories";
import type { PostgresDatabase } from "@/server/modules/runtime/adapters/runtime/postgres";
import { drainSocialAgent } from "@/server/modules/collaboration/public/social-agent";
import { randomUUID } from 'node:crypto';
import { access } from 'node:fs/promises';
import { join } from 'node:path';
import { resourceSlots, registeredResources } from "@/lib/runtime/holding-registry";
import { project, type EventMessage, type JobEnv } from "@/server/modules/workflow/public/job-projector";
import { generateOccurrences } from "@/server/modules/operations/public/routines";
import { drainExpenseAi } from "@/server/modules/people_payroll/public/expense-ai";
import { drainRequestAi } from "@/server/modules/records/public/request-ai";
import { cleanupBetaResources } from "@/server/modules/manage_erpx/public/beta-maintenance";
import type { SqlDatabase } from "@/server/modules/runtime/adapters/runtime/types";
import {runtimeJobEffects,type RuntimePendingEvent} from "@/server/modules/workflow/public/runtime-job-effects";
import {runtimeLegalJobHealthEffects} from "@/server/modules/legal/public/runtime-job-health-effects";
import {runtimeBetaSlotActive} from "@/server/modules/manage_erpx/public/runtime-active-slot-source";

export type PendingEvent = RuntimePendingEvent;
export async function claimJob(db: SqlDatabase, now = Date.now()): Promise<PendingEvent | null> {
  return runtimeJobEffects(db).claim(randomUUID(),now+90_000,now,now);
}
export async function runOne(env: JobEnv, holdingId: string, handler = project): Promise<boolean> {
  const { write } = registeredResources(env, holdingId), row = await claimJob(write);
  if (!row) return false;
  const renewal = setInterval(() => {
    void runtimeJobEffects(write).renew(Date.now()+90_000,row.id,row.lease_token).catch(() => {});
  },15_000);
  try {
    const message: EventMessage = { holdingId,id:row.id,eventType:row.event_type,aggregateId:row.aggregate_id,aggregateVersion:row.aggregate_version,payload:JSON.parse(row.payload_json) };
    await handler(env,message);
    await runtimeJobEffects(write).complete(Date.now(),Date.now(),row.id,row.lease_token);
  } catch (error) {
    const code = (error as { code?: string; name?: string }).code ?? (error instanceof Error ? error.name : 'JobError');
    await runtimeJobEffects(write).failed(Date.now()+Math.min(60_000,1000*2**Math.min(row.attempts,6)),String(code).slice(0,64),row.attempts>=6?Date.now():null,row.id,row.lease_token);
    console.error(JSON.stringify({event:'job_failed',holdingId,eventId:row.id,attempt:row.attempts,code}));
  } finally { clearInterval(renewal); }
  return true;
}
export async function activeSlots(env: JobEnv) {
  const active=[];
  for (const slot of resourceSlots(env)) if (!slot.beta || (env.CONTROL_DB && await runtimeBetaSlotActive(env.CONTROL_DB,slot.id))) active.push(slot);
  return active;
}
export async function directoryWork(env: JobEnv) {
  const control=env.CONTROL_DB;
  if(control){const delivery=await import("@/server/modules/identity_access/public/account-invitation-delivery");for(const slot of await activeSlots(env))await delivery.promoteAccountInvitationDeliveries(control,registeredResources(env,slot.id).write,slot.id);await delivery.deliverAccountInvitation(control,env);await (await import("@/server/modules/identity_access/public/account-recovery")).deliverAccountRecovery(control,env as JobEnv&{BETTER_AUTH_SECRET:string});}
  if(env.CONTROL_DB) await drainDirectoryJobs(env.CONTROL_DB as PostgresDatabase);
  for(const slot of await activeSlots(env)) {
    const db=registeredResources(env,slot.id).write as PostgresDatabase;
    await drainDirectoryJobs(db);
    if (!slot.beta) { await drainDirectoryOperations(db); await drainProvisioning(db); await drainPositionGpo(db); }
    await (await import("@/server/modules/identity_access/public/user-management")).managementWork(db);
  }
}
export async function posQueueWork(env: JobEnv) {
 for(const slot of await activeSlots(env))await (await import("@/server/modules/pos/public/pos-stripe")).drainPos(registeredResources(env,slot.id).write as PostgresDatabase,slot.id);
}
export async function scheduledWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) if (!slot.beta) await drainPeopleLifecycle(registeredResources(env,slot.id).write as PostgresDatabase);
  for (const slot of await activeSlots(env)) {
    const { write,files } = registeredResources(env,slot.id);
    await (await import("@/server/modules/finance/public/finance-due-drafts")).drainFinanceDueDrafts(write,slot.id);
    if (!slot.beta) {
      const payroll = await import("@/server/modules/people_payroll/public/payroll-readiness");
      await payroll.drainPayrollSchedules(write);
      await payroll.drainPayrollScheduledPreviews(write);
    }
    if (!slot.beta) await (await import("@/server/modules/people_payroll/public/people-roster")).escalateRosterCoverage(write);
    await (await import("@/server/modules/people_payroll/public/leave-self-service")).runLeaveAccruals(write,new Date().toISOString().slice(0,10));
    await (await import("@/server/modules/inventory/public/inventory-reservation-alerts")).escalateInventoryReservations(write);
    await (await import("@/server/modules/quality/public/quality-alerts")).drainQualityAlerts(write);
    await (await import("@/server/modules/identity_access/public/iam-access-review-alerts")).escalateAccessReviews(write);
    await (await import("@/server/modules/operations/public/staff-task-alerts")).escalateStaffTasks(write);
    await (await import("@/server/modules/collaboration/public/inbox")).drainInboxNotices(write);
    await (await import("@/server/modules/work_projection/public/calendar/reminders")).drainCalendarReminders(write as PostgresDatabase);
    if(env.CONTROL_DB)try{
      await (await import("@/server/modules/collaboration/public/web-push")).drainWebPush(write,env.CONTROL_DB);
    }catch(error){
      console.warn(JSON.stringify({event:"web_push_work_failed",holdingId:slot.id,
        code:error instanceof Error?error.name:"PushError"}));
    }
    const inboxMetrics=await (await import("@/server/modules/collaboration/public/inbox")).inboxOperationalMetrics(write);
    if(inboxMetrics.oldestDeliveryLagMs>300000||inboxMetrics.unresolvedEventTypes>0||inboxMetrics.failedJobs>0)
      console.warn(JSON.stringify({event:"inbox_delivery_health",holdingId:slot.id,...inboxMetrics}));
    await generateOccurrences(write);
    await drainRequestAi(write,env);
    if (!slot.beta) await (await import("@/server/modules/people_payroll/public/people-evaluation-ai")).drainEvaluationAi(write,env);
    if(!slot.beta) { const financeAi=await import("@/server/modules/finance/public/finance-ai");await financeAi.scanFinanceAi(write,env);await financeAi.drainFinanceAi(write,env); }
    await (await import("@/server/modules/runtime/public/executive/generation")).drainExecutiveGeneration(write,env);
    await drainSocialAgent(write,env,undefined,undefined,files,slot.id);
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

let usageProjectionNextAt = 0;
let usageHistoryBootstrapped = false;
export async function billingWork(env: JobEnv) {
  if(!env.CONTROL_DB)return;
  await (await import("@/server/modules/billing/public/billing-jobs")).drainBilling(env as import("@/server/modules/billing/public/billing-jobs").BillingJobEnv);
  await (await import("@/server/modules/billing/public/billing-attachments")).drainBillingEvidenceScans(env as import("@/server/modules/runtime/adapters/runtime/env").RuntimeEnv);
  if(Date.now()>=usageProjectionNextAt) {
    try {
      const bindings=(await env.CONTROL_DB.prepare(`SELECT holding_id id,slug,display_name "displayName"
        FROM holding_bindings WHERE status='active' ORDER BY holding_id`)
        .all<{id:string;slug:string;displayName:string}>()).results
        .filter(binding=>resourceSlots(env).some(slot=>slot.id===binding.id));
      const projection=await import("@/server/modules/commercial/public/usage-projection");
      const recent=await projection.refreshRecentUsageProjections(env.CONTROL_DB,bindings);
      const history=usageHistoryBootstrapped
        ? {refreshed:0,unavailable:[] as string[]}
        : await projection.bootstrapUsageProjectionHistory(env.CONTROL_DB,bindings);
      const unavailable=[...recent.unavailable,...history.unavailable];
      if(!history.unavailable.length)usageHistoryBootstrapped=true;
      usageProjectionNextAt=Date.now()+(unavailable.length?5*60_000:60*60_000);
      if(unavailable.length)console.warn(JSON.stringify({event:"usage_projection_partial",unavailable}));
    } catch(error) {
      usageProjectionNextAt=Date.now()+5*60_000;
      console.warn(JSON.stringify({event:"usage_projection_failed",code:error instanceof Error?error.name:"ProjectionError"}));
    }
  }
  await env.CONTROL_DB.prepare("INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES('billing',?,'{}') ON CONFLICT(service) DO UPDATE SET heartbeat_at=excluded.heartbeat_at").bind(Date.now()).run();
}

let attachmentCleanupAt = 0;
export async function attachmentWork(env: JobEnv) {
  const attachments = await import("@/server/modules/files/public/attachments/service");
  const cleanupDue = Date.now() - attachmentCleanupAt >= 60 * 60_000;
  let cleaned = false;
  for (const slot of await activeSlots(env)) {
    const { write, files } = registeredResources(env, slot.id);
    try {
      for (let processed = 0; processed < 25 && await attachments.drainAttachmentScans(write, files); processed++);
      if (cleanupDue) {
        await attachments.cleanupAttachmentDrafts(write, files);
        cleaned = true;
      }
    } catch (error) {
      console.error(JSON.stringify({
        event: "attachment_work_failed",
        holdingId: slot.id,
        code: error instanceof Error ? error.message.slice(0, 160) : "AttachmentWorkError",
      }));
    }
  }
  if (cleanupDue && cleaned) attachmentCleanupAt = Date.now();
}

export async function pmsWork(env: JobEnv) {
 for (const slot of await activeSlots(env)) if (!slot.beta) {
   const db=registeredResources(env,slot.id).write as PostgresDatabase;
   await (await import("@/server/modules/pms/public/pms")).drainPms(db);
   await (await import("@/server/modules/pms/public/pms-booking")).drainBookingPayments(db,{secret:String(env.BETTER_AUTH_SECRET),origin:applicationOrigin("booking",String(env.ERPX_PUBLIC_ORIGIN))!,apiKey:typeof env.RESEND_API_KEY==='string'?env.RESEND_API_KEY:undefined,from:typeof env.RESEND_FROM==='string'?env.RESEND_FROM:undefined});
   await (await import("@/server/modules/pms/public/pms-stripe")).drainPmsStripePayments(db);
   {
     const booking=await import("@/server/modules/pms/public/pms-booking");
     const {simulationBookingEmailProvider}=await import("@/server/modules/pms/public/simulation-email");
     await booking.deliverBookingEmails(db,{secret: String(env.BETTER_AUTH_SECRET),origin: applicationOrigin("booking",String(env.ERPX_PUBLIC_ORIGIN))!,apiKey:typeof env.RESEND_API_KEY === "string" ? env.RESEND_API_KEY : undefined,from:typeof env.RESEND_FROM === "string" ? env.RESEND_FROM : undefined},simulationBookingEmailProvider(env));
   }
 }

}

export async function legalWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) {
    let resources: ReturnType<typeof registeredResources>;
    try { resources = registeredResources(env,slot.id); }
    catch (error) {
      console.error(JSON.stringify({event:'legal_holding_unavailable',holdingId:slot.id,
        code:String((error as {code?:string}).code ?? 'LEGAL_RESOURCES_UNAVAILABLE').slice(0,64)}));
      continue;
    }
    const { write, files } = resources;
    const tasks: [string, () => Promise<unknown>][] = [
      ['reminders', async () => (await import("@/server/modules/legal/public/legal/jobs")).legalReminders(write)],
      ['invitations', async () => (await import("@/server/modules/legal/public/legal/invitations")).drainLegalInvitations(write,env)],
      ['documents', async () => (await import("@/server/modules/legal/public/legal/processing")).drainLegalProcessing(write,files)],
      ['ai', async () => (await import("@/server/modules/legal/public/legal/jobs")).drainLegalAi(write,env)],
      ['sources', async () => (await import("@/server/modules/legal/public/legal/law-corpus")).drainLegalSourceChecks(write,files)],
    ];
    // Destruction is invoked explicitly by isolated rehearsals. The recurring
    // v1 worker retains evidence in every environment.
    await Promise.allSettled(tasks.map(async ([task, run]) => {
      const now = Date.now();
      const renewal = setInterval(() => {
        void runtimeLegalJobHealthEffects(write).renew(Date.now(),task).catch(() => {});
      },15_000);
      try {
        await runtimeLegalJobHealthEffects(write).started(task,now);
        await run();
        await runtimeLegalJobHealthEffects(write).succeeded(Date.now(),Date.now(),task);
      } catch (error) {
        const code = String((error as {code?: string}).code ?? (error instanceof Error ? error.name : 'LegalJobError')).slice(0,64);
        console.error(JSON.stringify({event:'legal_work_failed',holdingId:slot.id,task,code}));
        await runtimeLegalJobHealthEffects(write).failed(Date.now(),code,task).catch(() => {});
      } finally { clearInterval(renewal); }
    }));
  }
}

export async function supportWork(env:JobEnv) {
 for(const slot of await activeSlots(env)) await (await import("@/server/modules/support/public/support-sla-alerts")).drainSupportSla(registeredResources(env,slot.id).write);
}

export async function customerNotificationWork(env:JobEnv) {
 for(const slot of await activeSlots(env)) await (await import("@/server/modules/sales/public/sales-customer-notifications")).drainCustomerNotifications(registeredResources(env,slot.id).write,env);
}
