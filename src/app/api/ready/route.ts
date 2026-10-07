import {runtimeAppSources} from "@/server/modules/runtime/public/app-native-sources";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { maintenance } from "@/server/modules/runtime/public/runtime/jobs";
export async function GET() {
  try {
    const env=await runtimeEnv();
    if(await maintenance(env.ERPX_DATA_DIR)) throw new Error('MAINTENANCE');
    const heartbeat=await runtimeAppSources(env.CONTROL_DB).readyHeartbeat();
    if(!heartbeat || Date.now()-heartbeat.heartbeat_at>45_000)throw new Error('JOBS_UNAVAILABLE');
    return Response.json({status:'ready'},{headers:{'Cache-Control':'no-store'}});
  } catch {return Response.json({status:'unavailable'},{status:503,headers:{'Cache-Control':'no-store'}});}
}
