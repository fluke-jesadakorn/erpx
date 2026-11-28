import { nativeRuntimeStatement } from '@erpx/platform-runtime-ui/adapters/server/native-runtime-command';
import { applicationOrigin } from "@erpx/platform-runtime-ui/lib/application-origins";
import {holdingRoutingEnabled,activeHoldingHosts} from '@erpx/platform-runtime-ui/lib/holding-hosts';
import { drainProvisioning } from "@erpx/identity-access-server/public/technology";
import { drainPeopleLifecycle } from "@erpx/people-payroll-server/public/people-lifecycle-jobs";
import { drainDirectoryOperations } from "@erpx/identity-access-server/public/directory-operations";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/adapters/runtime/postgres";
import { drainSocialAgent } from "@erpx/collaboration-server/public/social-agent";
import { randomUUID } from 'node:crypto';
import { resourceSlots, registeredResources } from "@erpx/platform-runtime-ui/lib/holding-registry";
import { project, type EventMessage, type JobEnv } from "@erpx/workflow-server/public/job-projector";
import { drainExpenseAi } from "@erpx/people-payroll-server/public/expense-ai";
import { drainRequestAi } from "@erpx/records-server/public/request-ai";
import { cleanupBetaResources } from "@erpx/manage-erpx-server/public/beta-maintenance";
import type { SqlDatabase } from "@erpx/platform-runtime-server/adapters/runtime/types";
import {runtimeJobEffects,type RuntimePendingEvent} from "@erpx/workflow-server/public/runtime-job-effects";
import {runtimeLegalJobHealthEffects} from "@erpx/legal-server/public/runtime-job-health-effects";
import { runNativeLegalWorker } from "@erpx/platform-runtime-server/adapters/runtime/native-legal-worker";
import {runtimeBetaSlotActive} from "@erpx/manage-erpx-server/public/runtime-active-slot-source";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { holdingBindingsSql } from '../read-tail-sql';
import { withHolding } from '@erpx/platform-runtime-server/public/runtime/holding-scope';

const exec = promisify(execFile);

export type PendingEvent = RuntimePendingEvent;
export async function claimJob(db: SqlDatabase, now = Date.now(), excludedSourceSql?: string, holdingId?: string): Promise<PendingEvent | null> {
  return runtimeJobEffects(db,excludedSourceSql,holdingId).claim(randomUUID(),now+90_000,now,now);
}
export async function runOne(env: JobEnv, holdingId: string, handler = project, excludedSourceSql?: string): Promise<boolean> {
  return withHolding(holdingId, () => runOneInHolding(env, holdingId, handler, excludedSourceSql));
}
async function runOneInHolding(env: JobEnv, holdingId: string, handler = project, excludedSourceSql?: string): Promise<boolean> {
  const { write } = registeredResources(env, holdingId), row = await claimJob(write, Date.now(), excludedSourceSql, holdingId);
  if (!row) return false;
  const renewal = setInterval(() => {
    void runtimeJobEffects(write, undefined, holdingId).renew(Date.now()+90_000,row.id,row.lease_token).catch(() => {});
  },15_000);
  try {
    const message: EventMessage = { holdingId,id:row.id,eventType:row.event_type,aggregateId:row.aggregate_id,aggregateVersion:row.aggregate_version,payload:JSON.parse(row.payload_json) };
    await handler(env,message);
    await runtimeJobEffects(write, undefined, holdingId).complete(Date.now(),Date.now(),row.id,row.lease_token);
  } catch (error) {
    const code = (error as { code?: string; name?: string }).code ?? (error instanceof Error ? error.name : 'JobError');
    await runtimeJobEffects(write, undefined, holdingId).failed(Date.now()+Math.min(60_000,1000*2**Math.min(row.attempts,6)),String(code).slice(0,64),row.attempts>=6?Date.now():null,row.id,row.lease_token);
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
  if(control){const delivery=await import("@erpx/identity-access-server/public/account-invitation-delivery");for(const slot of await activeSlots(env))await delivery.promoteAccountInvitationDeliveries(control,registeredResources(env,slot.id).write,slot.id);await delivery.deliverAccountInvitation(control,env);await (await import("@erpx/identity-access-server/public/account-recovery")).deliverAccountRecovery(control,env as JobEnv&{BETTER_AUTH_SECRET:string});}
  for(const slot of await activeSlots(env)) {
    const db=registeredResources(env,slot.id).write as PostgresDatabase;
    await withHolding(slot.id, async () => {
      if (!slot.beta) { await drainDirectoryOperations(db); await drainProvisioning(db); await runNativePositionGpoWorker(slot.id); }
      await (await import("@erpx/identity-access-server/public/user-management")).managementWork(db);
    });
  }
}
export async function posQueueWork(env: JobEnv) {
 for(const slot of await activeSlots(env))await withHolding(slot.id, async () => (await import("@erpx/pos-server/public/pos-stripe")).drainPos(registeredResources(env,slot.id).write as PostgresDatabase,slot.id));
}
export async function scheduledWork(env: JobEnv) {
  for (const slot of await activeSlots(env)) if (!slot.beta) await drainPeopleLifecycle(registeredResources(env,slot.id).write as PostgresDatabase);
  for (const slot of await activeSlots(env)) {
    const { write,files } = registeredResources(env,slot.id);
    await (await import("@erpx/finance-server/public/finance-due-drafts")).drainFinanceDueDrafts(write,slot.id);
    if (!slot.beta) {
      const payroll = await import("@erpx/people-payroll-server/public/payroll-readiness");
      await payroll.drainPayrollSchedules(write);
      await payroll.drainPayrollScheduledPreviews(write);
    }
    if (!slot.beta) await (await import("@erpx/people-payroll-server/public/people-roster")).escalateRosterCoverage(write);
    try { await withHolding(slot.id, async () => (await import("@erpx/people-payroll-server/public/leave-self-service")).runLeaveAccruals(write,new Date().toISOString().slice(0,10))); } catch (error) { console.warn(JSON.stringify({event:'leave_accruals_failed',holdingId:slot.id,code:error instanceof Error?error.message:String(error)})); }
    // Inventory Sales/Manufacturing reservation alert notices are owned by the
    // native inventory_reservation_alerts worker; no TS writer remains.
    // Quality due-soon/overdue escalation and notice routing are owned by the
    // native quality_alert_worker; no TS writer remains in this sweep.
    // Access-certification due-soon/overdue notices are owned by the native
    // identity_access_review_alerts worker; no TS writer remains.
    // Staff Work due-soon/overdue alerts are owned by the native
    // staff_task_alert_worker; no TS writer remains.
    await (await import("@erpx/collaboration-server/public/inbox")).drainInboxNotices(write);
    await (await import("@erpx/work-projection-server/public/calendar/reminders")).drainCalendarReminders(write as PostgresDatabase);
    if(env.CONTROL_DB)try{
      await (await import("@erpx/collaboration-server/public/web-push")).drainWebPush(write,env.CONTROL_DB);
    }catch(error){
      console.warn(JSON.stringify({event:"web_push_work_failed",holdingId:slot.id,
        code:error instanceof Error?error.name:"PushError"}));
    }
    const inbox=await import("@erpx/collaboration-server/public/inbox");
    const {inboxFailedScope}=await import("@erpx/workflow-server/public/collaboration-inbox-sources");
    const inboxMetrics=await inbox.inboxOperationalMetrics(write,inboxFailedScope(slot.id));
    if(inboxMetrics.oldestDeliveryLagMs>300000||inboxMetrics.unresolvedEventTypes>0||inboxMetrics.failedJobs>0)
      console.warn(JSON.stringify({event:"inbox_delivery_health",holdingId:slot.id,...inboxMetrics}));
    // Optional AI/agent drains must not abort the rest of a Holding sweep when
    // their native transport is not yet configured; each failure is named.
    for (const [event, task] of [
      ["request_ai_failed", () => withHolding(slot.id, () => drainRequestAi(write, env))],
      ...(!slot.beta ? [[
        "evaluation_ai_failed",
        async () => withHolding(slot.id, async () => (await import("@erpx/people-payroll-server/public/people-evaluation-ai")).drainEvaluationAi(write, env)),
      ]] : []),
      ...(!slot.beta ? [[
        "finance_ai_failed",
        async () => withHolding(slot.id, async () => { const ai = await import("@erpx/finance-server/public/finance-ai"); await ai.scanFinanceAi(write, env); await ai.drainFinanceAi(write, env); }),
      ]] : []),
      ["executive_generation_failed", async () => withHolding(slot.id, async () => (await import("@erpx/platform-runtime-server/public/executive/generation")).drainExecutiveGeneration(write, env))],
      ["social_agent_failed", () => withHolding(slot.id, () => drainSocialAgent(write, env, undefined, undefined, files, slot.id))],
    ] as [string, () => Promise<unknown>][]) {
      try { await task(); } catch (error) {
        console.warn(JSON.stringify({event, holdingId:slot.id, code:error instanceof Error?error.message:String(error)}));
      }
    }
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
  await (await import("@erpx/billing-server/public/billing-jobs")).drainBilling(env as import("@erpx/billing-server/public/billing-jobs").BillingJobEnv);
  await (await import("@erpx/billing-server/public/billing-attachments")).drainBillingEvidenceScans(env as import("@erpx/platform-runtime-server/adapters/runtime/env").RuntimeEnv);
  if(Date.now()>=usageProjectionNextAt) {
    try {
      const bindings=(await env.CONTROL_DB.prepare(holdingBindingsSql('holding_id id,slug,display_name "displayName"', " WHERE status='active' ORDER BY holding_id"))
        .all<{id:string;slug:string;displayName:string}>()).results
        .filter(binding=>resourceSlots(env).some(slot=>slot.id===binding.id));
      const projection=await import("@erpx/commercial-server/public/usage-projection");
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
  const heartbeatHolding = (await activeSlots(env))[0]?.id;
  if (heartbeatHolding)
    await withHolding(heartbeatHolding, () =>
      nativeRuntimeStatement('runtime_stmt_19', { companyId: '', now: Date.now(), actor: undefined })
        .bind(Date.now())
        .run(),
    );
}

let attachmentCleanupAt = 0;
export async function attachmentWork(env: JobEnv) {
  const attachments = await import("@erpx/files-server/public/attachments/service");
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
  await withHolding(slot.id, async () => {
   const db=registeredResources(env,slot.id).write as PostgresDatabase;
   await (await import("../../../../../../modules/pms/server/src/public/pms")).drainPms(db);
   await (await import("@erpx/pms-server/public/pms-booking")).drainBookingPayments(db,{...holdingOrigins,secret:String(env.BETTER_AUTH_SECRET),origin:applicationOrigin("booking",String(env.ERPX_PUBLIC_ORIGIN))!,apiKey:typeof env.RESEND_API_KEY==='string'?env.RESEND_API_KEY:undefined,from:typeof env.RESEND_FROM==='string'?env.RESEND_FROM:undefined});
   await (await import("@erpx/pms-server/public/pms-stripe")).drainPmsStripePayments(db);
   {
     const booking=await import("@erpx/pms-server/public/pms-booking");
     const {simulationBookingEmailProvider}=await import("@erpx/pms-server/public/simulation-email");
     await booking.deliverBookingEmails(db,{...holdingOrigins,secret: String(env.BETTER_AUTH_SECRET),origin: applicationOrigin("booking",String(env.ERPX_PUBLIC_ORIGIN))!,apiKey:typeof env.RESEND_API_KEY === "string" ? env.RESEND_API_KEY : undefined,from:typeof env.RESEND_FROM === "string" ? env.RESEND_FROM : undefined},simulationBookingEmailProvider(env));
   }
  });
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
      ['reminders', async () => (await import("@erpx/legal-server/public/legal/jobs")).legalReminders(write, slot.id)],
      ['invitations', async () => (await import("@erpx/legal-server/public/legal/invitations")).drainLegalInvitations(write,env)],
      ['documents', async () => runNativeLegalWorker(slot.id)],
      ['ai', async () => (await import("@erpx/legal-server/public/legal/jobs")).drainLegalAi(write,env)],
      ['sources', async () => {
        if (!env.CONTROL_DB) return;
        const binding = await env.CONTROL_DB.prepare(
          holdingBindingsSql('holding_id, slug, display_name', " WHERE holding_id=? AND status='active'"),
        ).bind(slot.id).first<{ holding_id: string; slug: string; display_name: string }>();
        if (!binding) return;
        return (await import("@erpx/legal-server/public/legal/law-corpus")).drainLegalSourceChecks(
          write, files,
          { holdingId: binding.holding_id, holdingSlug: binding.slug, holdingName: binding.display_name },
        );
      }],
    ];
    // Destruction is invoked explicitly by isolated rehearsals. The recurring
    // v1 worker retains evidence in every environment.
    await Promise.allSettled(tasks.map(async ([task, run]) => {
      const now = Date.now();
      const renewal = setInterval(() => {
        void runtimeLegalJobHealthEffects(write,slot.id).renew(Date.now(),task).catch(() => {});
      },15_000);
      try {
        await runtimeLegalJobHealthEffects(write,slot.id).started(task,now);
        await run();
        await runtimeLegalJobHealthEffects(write,slot.id).succeeded(Date.now(),Date.now(),task);
      } catch (error) {
        const code = String((error as {code?: string}).code ?? (error instanceof Error ? error.name : 'LegalJobError')).slice(0,64);
        console.error(JSON.stringify({event:'legal_work_failed',holdingId:slot.id,task,code}));
        await runtimeLegalJobHealthEffects(write,slot.id).failed(Date.now(),code,task).catch(() => {});
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
 * Runs one bounded native Position GPO delivery cycle for one exact Holding.
 * The native worker takes the catalog and provider advisory locks, reconciles
 * desired mappings, applies the bounded connector effects and commits the unit;
 * this transport only supplies the reviewed manifest path. A missing worker is
 * a hard failure, never a TypeScript delivery.
 */
async function runNativePositionGpoWorker(holdingId:string):Promise<void>{
 const binary=process.env.ERPX_NATIVE_SERVER_BIN,manifest=process.env.ERPX_DATABASE_MANIFEST;
 if(!binary||!manifest)throw Object.assign(new Error("The native Position GPO worker is not configured."),{code:"NATIVE_POSITION_GPO_WORKER_REQUIRED"});
 let stdout:string;
 try{({stdout}=await exec(binary,["position-gpo-worker","--once","--database-manifest",manifest,"--holding",holdingId],{timeout:120_000,maxBuffer:2*1024*1024,env:process.env,windowsHide:true}));}
 catch{throw Object.assign(new Error("The native Position GPO worker did not complete."),{code:"NATIVE_POSITION_GPO_WORKER_FAILED"});}
 let outcome:unknown;
 try{outcome=JSON.parse(stdout);}
 catch{throw Object.assign(new Error("The native Position GPO worker response is invalid."),{code:"NATIVE_POSITION_GPO_WORKER_RESPONSE_INVALID"});}
 const holdings=outcome&&typeof outcome==="object"&&!Array.isArray(outcome)?(outcome as {holdings?:unknown}).holdings:undefined;
 if(!Array.isArray(holdings)||holdings.length!==1||!holdings[0]||typeof holdings[0]!=="object"||(holdings[0] as {holdingId?:unknown}).holdingId!==holdingId)throw Object.assign(new Error("The native Position GPO worker target is invalid."),{code:"NATIVE_POSITION_GPO_WORKER_TARGET_MISMATCH"});
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
