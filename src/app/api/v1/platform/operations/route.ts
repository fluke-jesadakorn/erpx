import {requireManageErpxContext} from '@/server/authorization';

import {holdingResources,runtimeEnv,activeHoldingBindings} from '@/server/runtime';
import {summarizePlatformOperations} from '@/server/platform-operations';
import {errorResponse} from '@/server/http';
export const dynamic='force-dynamic';
export async function GET(){try{
 const {platform:actor}=await requireManageErpxContext(),env=await runtimeEnv();
 const bindings=(await activeHoldingBindings()).filter(row=>row.id!=='h003');
 const data=await summarizePlatformOperations(actor,env.CONTROL_DB,await Promise.all(bindings.map(async row=>({id:row.id,slug:row.slug,displayName:row.displayName,db:(await holdingResources(row.id)).write}))));
 return Response.json({data},{headers:{'Cache-Control':'private, no-store'}});
}catch(error){return errorResponse(error);}}
