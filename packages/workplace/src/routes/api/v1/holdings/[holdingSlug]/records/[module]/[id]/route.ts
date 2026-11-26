import type { RouteContext } from "@erpx/contracts/next-routes";
import { proxyNativeRecordRegistry } from "@erpx/server-records/public/native-record-registry";

export const dynamic = "force-dynamic";

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">){
  const {holdingSlug,module,id}=await context.params;
  return proxyNativeRecordRegistry(request,holdingSlug,{method:"GET",parts:[module,id]});
}
export async function PATCH(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">){
  const {holdingSlug,module,id}=await context.params;
  return proxyNativeRecordRegistry(request,holdingSlug,{method:"PATCH",parts:[module,id],body:true});
}
export async function DELETE(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">){
  const {holdingSlug,module,id}=await context.params;
  return proxyNativeRecordRegistry(request,holdingSlug,{method:"DELETE",parts:[module,id],query:true});
}
