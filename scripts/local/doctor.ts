import { getRuntime,closeRuntime } from '../../src/server/runtime/env';
import { resourceSlots,registeredResources } from '../../src/lib/holding-registry';
import { maintenance } from '../../src/server/runtime/jobs';
try{
  const env=getRuntime(),holdings=[];
  for(const slot of resourceSlots(env)){
    const {write,read}=registeredResources(env,slot.id);
    await read.prepare('SELECT 1').first();
    const backlog=await write.prepare(`SELECT count(*) FILTER(WHERE processed_at IS NULL AND dead_letter_at IS NULL) pending,
      count(*) FILTER(WHERE dead_letter_at IS NOT NULL) failed FROM outbox_events`).first();
    holdings.push({id:slot.id,...backlog});
  }
  console.log(JSON.stringify({origin:env.ERPX_PUBLIC_ORIGIN,registration:env.ERPX_PUBLIC_BETA==='true',emailConfigured:!!env.RESEND_API_KEY,
    maintenance:await maintenance(env.ERPX_DATA_DIR),services:(await env.CONTROL_DB.prepare('SELECT service,heartbeat_at FROM runtime_health').all()).results,holdings},null,2));
}finally{await closeRuntime();}
