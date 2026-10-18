import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import {requireManageErpxContext} from "@/server/modules/identity_access/public/authorization";

import {holdingResources,runtimeEnv,activeHoldingBindings} from "@/server/modules/runtime/public/runtime";
import {summarizePlatformOperations} from "@/server/modules/platform_administration/public/platform-operations";
import {errorResponse} from "@/server/modules/runtime/public/http";
export const dynamic='force-dynamic';
export async function GET(){try{
 const {platform:actor}=await requireManageErpxContext(),env=await runtimeEnv();
 const bindings=(await activeHoldingBindings()).filter(row=>row.id!=='h003');
 const data=await summarizePlatformOperations(actor,env.CONTROL_DB,await Promise.all(bindings.map(async row=>({id:row.id,slug:row.slug,displayName:row.displayName,db:(await holdingResources(row.id)).write}))));
 return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
