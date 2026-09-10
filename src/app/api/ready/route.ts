import { runtimeEnv } from '@/server/runtime';
import { maintenance } from '@/server/runtime/jobs';
export async function GET() {
  try {
    const env=await runtimeEnv();
    if(await maintenance(env.ERPX_DATA_DIR)) throw new Error('MAINTENANCE');
    const heartbeat=await env.CONTROL_DB.prepare("SELECT heartbeat_at FROM runtime_health WHERE service='jobs'").first<{heartbeat_at:number}>();
    if(!heartbeat || Date.now()-heartbeat.heartbeat_at>45_000)throw new Error('JOBS_UNAVAILABLE');
    return Response.json({status:'ready'},{headers:{'Cache-Control':'no-store'}});
  } catch {return Response.json({status:'unavailable'},{status:503,headers:{'Cache-Control':'no-store'}});}
}
