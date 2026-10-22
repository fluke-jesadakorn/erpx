import { getRuntime,closeRuntime } from "@erpx/server-runtime/adapters/runtime/env";
import { resourceSlots,registeredResources } from "@erpx/feature-runtime/lib/holding-registry";
import { maintenance } from "@erpx/server-runtime/adapters/runtime/jobs";
import { attachmentsV2Enabled } from "@erpx/server-files/public/attachments/feature";
import { attachmentRuntimeReadiness } from "@erpx/server-files/public/attachments/readiness";
import { billingEvidenceReadiness } from "@erpx/server-billing/public/billing-attachments";
try{
  const env=getRuntime(),holdings=[],attachmentsEnabled=attachmentsV2Enabled(),attachments=await attachmentRuntimeReadiness(),billing=await billingEvidenceReadiness(env);
  for(const slot of resourceSlots(env)){
    const {write,read}=registeredResources(env,slot.id);
    await read.prepare('SELECT 1').first();
    const backlog=await write.prepare(`SELECT count(*) FILTER(WHERE processed_at IS NULL AND dead_letter_at IS NULL) pending,
      count(*) FILTER(WHERE dead_letter_at IS NOT NULL) failed FROM outbox_events`).first();
    holdings.push({id:slot.id,...backlog});
  }
  console.log(JSON.stringify({origin:env.ERPX_PUBLIC_ORIGIN,registration:env.ERPX_PUBLIC_BETA==='true',emailConfigured:!!env.RESEND_API_KEY,
    maintenance:await maintenance(env.ERPX_DATA_DIR),attachments:{enabled:attachmentsEnabled,...attachments,billing},services:(await env.CONTROL_DB.prepare('SELECT service,heartbeat_at,details_json FROM runtime_health').all()).results,holdings},null,2));
}finally{await closeRuntime();}
