import type { RouteContext } from "@erpx/contracts/next-routes";
import { proxyNativeRecordRegistry } from "@erpx/server-records/public/native-record-registry";

export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/references">){
  const {holdingSlug,module,id}=await context.params;
  return proxyNativeRecordRegistry(request,holdingSlug,{method:"POST",parts:[module,id,"references"],body:true});
}
