import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { adminCompanyHandoff } from "@erpx/feature-runtime/lib/application-origins";
import { roomChargeReversalQueue } from "@erpx/server-pos/public/room-charge-reversal-queue";
import { z } from 'zod';
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import {posCommand} from '@erpx/server-pos/public/pos-core';
import { finishPosOAuth, oauthUrl } from "@erpx/server-pos/public/pos-stripe";
import { posCustomers,posDayReport } from "@erpx/server-pos/public/pos-daily";
import { posProductImage,uploadPosProductImage } from "@erpx/server-pos/public/pos-images";
type Context={params:Promise<{holdingSlug:string;path:string[]}>};
export async function POST(request:Request,{params}:Context){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
  const {holdingSlug,path}=await params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
  if(path.length!==2)throw new ApiError(404,'NOT_FOUND','Unknown POS action.');
  if(path.join('/')==='images/upload')return Response.json(canonicalDestinationLinks({data:await uploadPosProductImage(request,actor)}),{headers:{'Cache-Control':'private, no-store'}});
  const input=await request.json(),operation=path.join("."),db=(await holdingResources(actor.holdingId)).write as PostgresDatabase;
  const data=await posCommand(db,actor,operation,input,key);
  if(operation==='connection.oauth')data.url=oauthUrl(String(data.id),holdingSlug,Boolean(data.livemode));
  return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
 }catch(error){if(error instanceof z.ZodError)return errorResponse(new ApiError(422,'VALIDATION_FAILED',error.issues[0]?.message??'Invalid input.'));if(error instanceof SyntaxError)return errorResponse(new ApiError(422,'INVALID_JSON','Invalid JSON.'));return errorResponse(error);}
}
export async function GET(request:Request,{params}:Context){
 try{
  const {holdingSlug,path}=await params;
  if(path.join('/')==='room-charge-reversals') return Response.json(canonicalDestinationLinks({data:await roomChargeReversalQueue(await requireWorkContext(holdingSlug),new URL(request.url).searchParams.get('property')??undefined)}),{headers:{'Cache-Control':'private, no-store'}});
  if(path[0]==='images'&&path.length===2)return posProductImage(await requireWorkContext(holdingSlug),path[1],new URL(request.url).searchParams.get('original')==='1');
  if(path.join('/')==='customers/search'||path.join('/')==='day/report'){
   const actor=await requireWorkContext(holdingSlug),db=(await holdingResources(actor.holdingId)).write,url=new URL(request.url);
   const data=path[0]==='customers'?await posCustomers(db,actor,url.searchParams.get('q')??''):await posDayReport(db,actor,z.string().min(1).parse(actor.branchId??url.searchParams.get('branch')),z.iso.date().parse(url.searchParams.get('date')));
   return Response.json(canonicalDestinationLinks({data}),{headers:{'Cache-Control':'private, no-store'}});
  }
  if(path.join('/')!=='connect/callback')throw new ApiError(404,'NOT_FOUND','Not found.');
  const actor=await requireWorkContext(holdingSlug),url=new URL(request.url);if(url.searchParams.has('error'))throw new ApiError(422,'STRIPE_CONNECT_CANCELLED','Connection was cancelled. Start again from POS settings.');
  await finishPosOAuth(actor,z.string().uuid().parse(url.searchParams.get('state')),z.string().min(1).parse(url.searchParams.get('code')));
  return Response.redirect(adminCompanyHandoff(actor.companyId, `/holdings/${holdingSlug}/settings/stripe`,requestPublicOrigin(request)),303);
 }catch(error){return errorResponse(error);}
}
