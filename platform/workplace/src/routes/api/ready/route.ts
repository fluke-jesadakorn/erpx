import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {runtimeAppSources} from "@erpx/platform-runtime-server/public/app-native-sources";
import { runtimeEnv } from "../../../../../runtime/server/src/public/runtime";
import { maintenance } from "@erpx/platform-runtime-server/public/runtime/maintenance";
export async function GET() {
  try {
    const env=await runtimeEnv();
    if(await maintenance(env.ERPX_DATA_DIR)) throw new Error('MAINTENANCE');
    const heartbeat=await runtimeAppSources(env.CONTROL_DB).readyHeartbeat();
    if(!heartbeat || Date.now()-heartbeat.heartbeat_at>45_000)throw new Error('JOBS_UNAVAILABLE');
    return Response.json(canonicalDestinationLinks({status:'ready'}),{headers:{'Cache-Control':'no-store'}});
  } catch {return Response.json(canonicalDestinationLinks({status:'unavailable'}),{status:503,headers:{'Cache-Control':'no-store'}});}
}
