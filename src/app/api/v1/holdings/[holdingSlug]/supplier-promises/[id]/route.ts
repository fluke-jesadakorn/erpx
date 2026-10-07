import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {supplierPromiseDetail,recordSupplierPromise} from "@/server/modules/procurement/public/supplier-promises";
type Context={params:Promise<{holdingSlug:string;id:string}>};
export async function GET(_request:Request,context:Context){try{const {holdingSlug,id}=await context.params,actor=await requireWorkContext(holdingSlug);return Response.json({data:await supplierPromiseDetail(actor,id)},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}}
export async function POST(request:Request,context:Context){try{const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const {holdingSlug,id}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');return Response.json({data:await recordSupplierPromise(actor,id,await request.json(),key)},{headers:{'Cache-Control':'private, no-store'}});}catch(error){return errorResponse(error);}}
