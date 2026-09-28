import { attachmentsV2Enabled } from "@/server/attachments/feature";
import { attachmentRuntimeReadiness } from "@/server/attachments/readiness";
import { runtimeEnv } from "@/server/runtime";
import { registeredResources,resourceSlots } from "@/lib/holding-registry";
import { billingEvidenceReadiness } from "@/server/billing-attachments";

export const dynamic = "force-dynamic";

export async function GET() {
  const enabled=attachmentsV2Enabled();
  const app=await attachmentRuntimeReadiness();
  let migration={ready:!enabled,pendingHoldings:0};
  let billing={ready:false,scanReady:false,reconciliationReady:false,pending:-1,sealedMalware:-1,reconciledAt:null as number|null};
  let jobs={ready:false,heartbeatAt:null as number|null,attachmentReady:false,uploadFrozen:null as boolean|null};
  try{
    const env=await runtimeEnv();
    billing=await billingEvidenceReadiness(env);
    if(enabled){let pending=0;for(const slot of resourceSlots(env)){const row=await registeredResources(env,slot.id).write.prepare("SELECT ready FROM attachment_reconciliation_runs WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC LIMIT 1").first<{ready:boolean}>();if(!row?.ready)pending++;}migration={ready:pending===0,pendingHoldings:pending};}
    const heartbeat=await env.CONTROL_DB.prepare("SELECT heartbeat_at,details_json FROM runtime_health WHERE service='jobs'").first<{heartbeat_at:number;details_json:string}>();
    if(heartbeat){
      const details=JSON.parse(heartbeat.details_json) as {attachments?:{ready?:boolean;uploadFrozen?:boolean}};
      const heartbeatAt=Number(heartbeat.heartbeat_at),attachmentReady=details.attachments?.ready===true,freeze=details.attachments?.uploadFrozen,uploadFrozen=typeof freeze==="boolean"?freeze:null;
      jobs={ready:Date.now()-heartbeatAt<=45_000&&attachmentReady&&uploadFrozen!==null&&uploadFrozen===app.uploadFrozen,heartbeatAt,attachmentReady,uploadFrozen};
    }
  }catch{migration={ready:false,pendingHoldings:-1};}
  const billingReady=billing.scanReady&&(!enabled||billing.reconciliationReady);
  const ready=app.ready&&jobs.ready&&migration.ready&&billingReady;
  return Response.json(
    { status: ready ? "ok" : "degraded", attachments: { enabled,uploadFrozen:app.uploadFrozen,scanner:app.scanner,preview:app.preview,jobs,migration,billing:{...billing,compatibilityReady:billing.scanReady} } },
    { status: ready ? 200 : 503, headers: { "Cache-Control": "no-store" } },
  );
}
