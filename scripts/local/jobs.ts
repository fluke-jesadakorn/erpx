import { getRuntime, closeRuntime } from '../../src/server/runtime/env';
import { activeSlots, maintenance, runOne, scheduledWork, expenseAiWork, directoryWork } from '../../src/server/runtime/jobs';

let stopping=false;
for(const signal of ['SIGINT','SIGTERM'] as const) process.on(signal,()=>{stopping=true;});
const env=getRuntime();
async function heartbeat() {
  await env.CONTROL_DB.prepare(`INSERT INTO runtime_health(service,heartbeat_at,details_json) VALUES('jobs',?,?)
    ON CONFLICT(service) DO UPDATE SET heartbeat_at=excluded.heartbeat_at,details_json=excluded.details_json`)
    .bind(Date.now(),JSON.stringify({maintenance:await maintenance(env.ERPX_DATA_DIR)})).run();
}
let directoryRunning: Promise<void> | null = null;
const directoryPulse = setInterval(() => {
 if(stopping || directoryRunning)return;
 directoryRunning=(async()=>{if(!await maintenance(env.ERPX_DATA_DIR))await directoryWork(env);})()
  .catch(()=>console.error('{"event":"directory_iteration_failed"}')).finally(()=>{directoryRunning=null;});
},10000);
let aiRunning: Promise<void> | null = null;
const aiPulse = setInterval(() => {
  if (stopping || aiRunning) return;
  aiRunning = (async () => { if (!await maintenance(env.ERPX_DATA_DIR)) await expenseAiWork(env); })()
    .catch(() => console.error('{"event":"expense_ai_iteration_failed"}'))
    .finally(() => { aiRunning = null; });
}, 3000);
const pulse=setInterval(()=>void heartbeat().catch(()=>console.error('{"event":"jobs_heartbeat_failed"}')),10_000);
try {
  await heartbeat();let scheduledAt=0;
  while(!stopping) {
    try {
      if(!await maintenance(env.ERPX_DATA_DIR)) {
        if(Date.now()-scheduledAt>=120_000){await scheduledWork(env);scheduledAt=Date.now();}
        for(const slot of await activeSlots(env)){if(stopping)break;await runOne(env,slot.id);}
      }
    } catch(error) {console.error(JSON.stringify({event:'jobs_iteration_failed',code:error instanceof Error?error.name:'UnknownError'}));}
    if(!stopping)await new Promise(resolve=>setTimeout(resolve,1000));
  }
} finally {clearInterval(directoryPulse);clearInterval(aiPulse);clearInterval(pulse);await directoryRunning;await aiRunning;await closeRuntime();}
