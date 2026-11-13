import { applicationOrigin } from "@erpx/feature-runtime/lib/application-origins";
import {holdingRoutingEnabled,activeHoldingHosts} from '@erpx/feature-runtime/lib/holding-hosts';
import { drainProvisioning } from "@erpx/server-identity_access/public/technology";
import { drainPeopleLifecycle } from "@erpx/server-people_payroll/public/people-lifecycle-jobs";
import { drainDirectoryOperations } from "@erpx/server-identity_access/public/directory-operations";
import { drainPositionGpo } from "@erpx/server-identity_access/public/position-gpo";
import { drainDirectoryJobs } from "@erpx/server-identity_access/public/iam-directories";
import type { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import { drainSocialAgent } from "@erpx/server-collaboration/public/social-agent";
import { randomUUID } from 'node:crypto';
import { resourceSlots, registeredResources } from "@erpx/feature-runtime/lib/holding-registry";
import { project, type EventMessage, type JobEnv } from "@erpx/server-workflow/public/job-projector";
import { drainExpenseAi } from "@erpx/server-people_payroll/public/expense-ai";
import { drainRequestAi } from "@erpx/server-records/public/request-ai";
import { cleanupBetaResources } from "@erpx/server-manage_erpx/public/beta-maintenance";
import type { SqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";
import {runtimeJobEffects,type RuntimePendingEvent} from "@erpx/server-workflow/public/runtime-job-effects";
import {runtimeLegalJobHealthEffects} from "@erpx/server-legal/public/runtime-job-health-effects";
import { runNativeLegalWorker } from "@erpx/server-runtime/adapters/runtime/native-legal-worker";
import {runtimeBetaSlotActive} from "@erpx/server-manage_erpx/public/runtime-active-slot-source";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const exec = promisify(execFile);

export type PendingEvent = RuntimePendingEvent;
export async function claimJob(db: SqlDatabase, now = Date.now(), excludedSourceSql?: string): Promise<PendingEvent | null> {
  return runtimeJobEffects(db,excludedSourceSql).claim(randomUUID(),now+90_000,now,now);
}
export async function runOne(env: JobEnv, holdingId: string, handler = project, excludedSourceSql?: string): Promise<boolean> {
  const { write } = registeredResources(env, holdingId), row = await claimJob(write, Date.now(), excludedSourceSql);
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
  if(control){const delivery=await import("@erpx/server-identity_access/public/account-invitation-delivery");for(const slot of await activeSlots(env))await delivery.promoteAccountInvitationDeliveries(control,registeredResources(env,slot.id).write,slot.id);await delivery.deliverAccountInvitation(control,env);await (await import("@erpx/server-identity_access/public/account-recovery")).deliverAccountRecovery(control,env as JobEnv&{BETTER_AUTH_SECRET:string});}
  if(env.CONTROL_DB) await drainDirectoryJobs(env.CONTROL_DB as PostgresDatabase);
  for(const slot of await activeSlots(env)) {
    const db=registeredResources(env,slot.id).write as PostgresDatabase;
    await drainDirectoryJobs(db);
    if (!slot.beta) { await drainDirectoryOperations(db); await drainProvisioning(db); await drainPositionGpo(db); }
    await (await import("@erpx/server-identity_access/public/user-management")).managementWork(db);
  }
}
export async function posQueueWork(env: JobEnv) {
 for(const slot of await activeSlots(env))await (await import("@erpx/server-pos/public/pos-stripe")).drainPos(registeredResources(env,slot.id).write as PostgresDatabase,slot.id);
}
export async function scheduledWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) if (!slot.beta) await drainPeopleLifecycle(registeredResources(env,slot.id).write as PostgresDatabase);
  for (const slot of await activeSlots(env)) {
    const { write,files } = registeredResources(env,slot.id);
    await (await import("@erpx/server-finance/public/finance-due-drafts")).drainFinanceDueDrafts(write,slot.id);
    if (!slot.beta) {
      const payroll = await import("@erpx/server-people_payroll/public/payroll-readiness");
      await payroll.drainPayrollSchedules(write);
      await payroll.drainPayrollScheduledPreviews(write);
    }
    if (!slot.beta) await (await import("@erpx/server-people_payroll/public/people-roster")).escalateRosterCoverage(write);
    await (await import("@erpx/server-people_payroll/public/leave-self-service")).runLeaveAccruals(write,new Date().toISOString().slice(0,10));
    await (await import("@erpx/server-inventory/public/inventory-reservation-alerts")).escalateInventoryReservations(write);
    // The due-soon/overdue escalation inserts are native (erpx quality
    // quality_alert_worker); Collaboration still routes their notices until a
    // native Quality notification port lands.
    await (await import("@erpx/server-collaboration/public/quality-notifications")).routeQualityAlertNotifications(write,Date.now());
    await (await import("@erpx/server-identity_access/public/iam-access-review-alerts")).escalateAccessReviews(write);
    await (await import("@erpx/server-operations/public/staff-task-alerts")).escalateStaffTasks(write);
    await (await import("@erpx/server-collaboration/public/inbox")).drainInboxNotices(write);
    await (await import("@erpx/server-work_projection/public/calendar/reminders")).drainCalendarReminders(write as PostgresDatabase);
    if(env.CONTROL_DB)try{
      await (await import("@erpx/server-collaboration/public/web-push")).drainWebPush(write,env.CONTROL_DB);
    }catch(error){
      console.warn(JSON.stringify({event:"web_push_work_failed",holdingId:slot.id,
        code:error instanceof Error?error.name:"PushError"}));
    }
    const inboxMetrics=await (await import("@erpx/server-collaboration/public/inbox")).inboxOperationalMetrics(write);
    if(inboxMetrics.oldestDeliveryLagMs>300000||inboxMetrics.unresolvedEventTypes>0||inboxMetrics.failedJobs>0)
      console.warn(JSON.stringify({event:"inbox_delivery_health",holdingId:slot.id,...inboxMetrics}));
    await drainRequestAi(write,env);
    if (!slot.beta) await (await import("@erpx/server-people_payroll/public/people-evaluation-ai")).drainEvaluationAi(write,env);
    if(!slot.beta) { const financeAi=await import("@erpx/server-finance/public/finance-ai");await financeAi.scanFinanceAi(write,env);await financeAi.drainFinanceAi(write,env); }
    await (await import("@erpx/server-runtime/public/executive/generation")).drainExecutiveGeneration(write,env);
    await drainSocialAgent(write,env,undefined,undefined,files,slot.id);
    if(slot.beta) await cleanupBetaResources(write,files,slot.id);
  }
}
export { maintenance } from "./maintenance";

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
  await (await import("@erpx/server-billing/public/billing-jobs")).drainBilling(env as import("@erpx/server-billing/public/billing-jobs").BillingJobEnv);
  await (await import("@erpx/server-billing/public/billing-attachments")).drainBillingEvidenceScans(env as import("@erpx/server-runtime/adapters/runtime/env").RuntimeEnv);
  if(Date.now()>=usageProjectionNextAt) {
    try {
      const bindings=(await env.CONTROL_DB.prepare(`SELECT holding_id id,slug,display_name "displayName"
        FROM holding_bindings WHERE status='active' ORDER BY holding_id`)
        .all<{id:string;slug:string;displayName:string}>()).results
        .filter(binding=>resourceSlots(env).some(slot=>slot.id===binding.id));
      const projection=await import("@erpx/server-commercial/public/usage-projection");
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
  const attachments = await import("@erpx/server-files/public/attachments/service");
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
 const holdingOrigins=holdingRoutingEnabled(process.env)?{holdingOrigins:Object.fromEntries(activeHoldingHosts().map(host=>[host.holdingSlug,`https://${host.hostname}`]))}:{};
 for (const slot of await activeSlots(env)) if (!slot.beta) {
   const db=registeredResources(env,slot.id).write as PostgresDatabase;
   await (await import("../../../../../packages/server-pms/src/public/pms")).drainPms(db);
   await (await import("@erpx/server-pms/public/pms-booking")).drainBookingPayments(db,{...holdingOrigins,secret:String(env.BETTER_AUTH_SECRET),origin:applicationOrigin("booking",String(env.ERPX_PUBLIC_ORIGIN))!,apiKey:typeof env.RESEND_API_KEY==='string'?env.RESEND_API_KEY:undefined,from:typeof env.RESEND_FROM==='string'?env.RESEND_FROM:undefined});
   await (await import("@erpx/server-pms/public/pms-stripe")).drainPmsStripePayments(db);
   {
     const booking=await import("@erpx/server-pms/public/pms-booking");
     const {simulationBookingEmailProvider}=await import("@erpx/server-pms/public/simulation-email");
     await booking.deliverBookingEmails(db,{...holdingOrigins,secret: String(env.BETTER_AUTH_SECRET),origin: applicationOrigin("booking",String(env.ERPX_PUBLIC_ORIGIN))!,apiKey:typeof env.RESEND_API_KEY === "string" ? env.RESEND_API_KEY : undefined,from:typeof env.RESEND_FROM === "string" ? env.RESEND_FROM : undefined},simulationBookingEmailProvider(env));
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
      ['reminders', async () => (await import("@erpx/server-legal/public/legal/jobs")).legalReminders(write)],
      ['invitations', async () => (await import("@erpx/server-legal/public/legal/invitations")).drainLegalInvitations(write,env)],
      ['documents', async () => runNativeLegalWorker(slot.id)],
      ['ai', async () => (await import("@erpx/server-legal/public/legal/jobs")).drainLegalAi(write,env)],
      ['sources', async () => (await import("@erpx/server-legal/public/legal/law-corpus")).drainLegalSourceChecks(write,files)],
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

export async function customerNotificationWork(env:JobEnv) {
 for(const slot of await activeSlots(env)) {
  try{await runNativeSalesNotificationWorker(slot.id);}
  catch(error){console.error(JSON.stringify({event:"sales_customer_notification_delivery_unavailable",holdingId:slot.id,code:String((error as {code?:string}).code??(error instanceof Error?error.name:"DeliveryError")).slice(0,64)}));}
  try{await runNativeCustomerPortalInvitationWorker(slot.id);}
  catch(error){console.error(JSON.stringify({event:"customer_portal_invitation_delivery_unavailable",holdingId:slot.id,code:String((error as {code?:string}).code??(error instanceof Error?error.name:"DeliveryError")).slice(0,64)}));}
 }
}

/**
 * Runs one bounded native Customer Portal invitation cycle for one exact
 * Holding. The native worker owns recovery, lease, source/token checks and the
 * terminal commit; this transport only supplies the reviewed manifest path.
 * A missing native worker is a hard failure, never a TypeScript delivery.
 */
async function runNativeCustomerPortalInvitationWorker(holdingId:string):Promise<void>{
 const binary=process.env.ERPX_NATIVE_SERVER_BIN,manifest=process.env.ERPX_DATABASE_MANIFEST;
 if(!binary||!manifest)throw Object.assign(new Error("The native Customer Portal invitation worker is not configured."),{code:"NATIVE_CUSTOMER_PORTAL_INVITATION_WORKER_REQUIRED"});
 let stdout:string;
 try{({stdout}=await exec(binary,["customer-portal-invitation-worker","--once","--database-manifest",manifest,"--holding",holdingId],{timeout:120_000,maxBuffer:2*1024*1024,env:process.env,windowsHide:true}));}
 catch{throw Object.assign(new Error("The native Customer Portal invitation worker did not complete."),{code:"NATIVE_CUSTOMER_PORTAL_INVITATION_WORKER_FAILED"});}
 let outcome:unknown;
 try{outcome=JSON.parse(stdout);}
 catch{throw Object.assign(new Error("The native Customer Portal invitation worker response is invalid."),{code:"NATIVE_CUSTOMER_PORTAL_INVITATION_WORKER_RESPONSE_INVALID"});}
 const holdings=outcome&&typeof outcome==="object"&&!Array.isArray(outcome)?(outcome as {holdings?:unknown}).holdings:undefined;
 if(!Array.isArray(holdings)||holdings.length!==1||!holdings[0]||typeof holdings[0]!=="object"||(holdings[0] as {holdingId?:unknown}).holdingId!==holdingId)throw Object.assign(new Error("The native Customer Portal invitation worker target is invalid."),{code:"NATIVE_CUSTOMER_PORTAL_INVITATION_WORKER_TARGET_MISMATCH"});
}

/**
 * Runs one bounded native Sales customer-notice cycle for one exact Holding.
 * The native worker owns claim, lease, authority, provider dispatch and the
 * terminal commit; this transport only supplies the reviewed manifest path.
 * A missing native worker is a hard failure, never a TypeScript delivery.
 */
async function runNativeSalesNotificationWorker(holdingId:string):Promise<void>{
 const binary=process.env.ERPX_NATIVE_SERVER_BIN,manifest=process.env.ERPX_DATABASE_MANIFEST;
 if(!binary||!manifest)throw Object.assign(new Error("The native Sales customer-notice worker is not configured."),{code:"NATIVE_SALES_NOTIFICATION_WORKER_REQUIRED"});
 let stdout:string;
 try{({stdout}=await exec(binary,["sales-notification-worker","--once","--database-manifest",manifest,"--holding",holdingId],{timeout:120_000,maxBuffer:2*1024*1024,env:process.env,windowsHide:true}));}
 catch{throw Object.assign(new Error("The native Sales customer-notice worker did not complete."),{code:"NATIVE_SALES_NOTIFICATION_WORKER_FAILED"});}
 let outcome:unknown;
 try{outcome=JSON.parse(stdout);}
 catch{throw Object.assign(new Error("The native Sales customer-notice worker response is invalid."),{code:"NATIVE_SALES_NOTIFICATION_WORKER_RESPONSE_INVALID"});}
 const holdings=outcome&&typeof outcome==="object"&&!Array.isArray(outcome)?(outcome as {holdings?:unknown}).holdings:undefined;
 if(!Array.isArray(holdings)||holdings.length!==1||!holdings[0]||typeof holdings[0]!=="object"||(holdings[0] as {holdingId?:unknown}).holdingId!==holdingId)throw Object.assign(new Error("The native Sales customer-notice worker target is invalid."),{code:"NATIVE_SALES_NOTIFICATION_WORKER_TARGET_MISMATCH"});
}
