import type { RouteContext } from "@erpx/contracts/next-routes";
import { proxyNativeRecordRegistry } from "@erpx/server-records/public/native-record-registry";

export const dynamic = "force-dynamic";

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]">){
  const {holdingSlug,module}=await context.params;
  return proxyNativeRecordRegistry(request,holdingSlug,{method:"GET",parts:[module],query:true});
}
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]">){
  const {holdingSlug,module}=await context.params;
  return proxyNativeRecordRegistry(request,holdingSlug,{method:"POST",parts:[module],body:true});
}
