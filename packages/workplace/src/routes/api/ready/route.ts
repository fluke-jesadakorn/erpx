import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {runtimeAppSources} from "@erpx/server-runtime/public/app-native-sources";
import { runtimeEnv } from "../../../../../../packages/server-runtime/src/public/runtime";
import { maintenance } from "@erpx/server-runtime/public/runtime/maintenance";
export async function GET() {
  try {
    const env=await runtimeEnv();
    if(await maintenance(env.ERPX_DATA_DIR)) throw new Error('MAINTENANCE');
    const heartbeat=await runtimeAppSources(env.CONTROL_DB).readyHeartbeat();
    if(!heartbeat || Date.now()-heartbeat.heartbeat_at>45_000)throw new Error('JOBS_UNAVAILABLE');
    return Response.json(canonicalDestinationLinks({status:'ready'}),{headers:{'Cache-Control':'no-store'}});
  } catch {return Response.json(canonicalDestinationLinks({status:'unavailable'}),{status:503,headers:{'Cache-Control':'no-store'}});}
}
