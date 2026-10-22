import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";

import {holdingResources,runtimeEnv,activeHoldingBindings} from "../../../../../../../../packages/server-runtime/src/public/runtime";
import {summarizePlatformOperations} from "@erpx/server-platform_administration/public/platform-operations";
import {errorResponse} from "@erpx/server-runtime/public/http";
export const dynamic='force-dynamic';
export async function GET(){try{
 const {platform:actor}=await requireManageErpxContext(),env=await runtimeEnv();
 const bindings=(await activeHoldingBindings()).filter(row=>row.id!=='h003');
 const data=await summarizePlatformOperations(actor,env.CONTROL_DB,await Promise.all(bindings.map(async row=>({id:row.id,slug:row.slug,displayName:row.displayName,db:(await holdingResources(row.id)).write}))));
 return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
