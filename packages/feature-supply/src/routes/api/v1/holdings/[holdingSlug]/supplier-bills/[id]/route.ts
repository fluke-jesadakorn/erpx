import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {requestPublicOrigin} from "@erpx/server-manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@erpx/server-runtime/public/http";
import {supplierBillDetail} from "@erpx/server-procurement/public/supplier-bill-matching";
import {ownedDecideSupplierBill} from "@erpx/feature-procurement/public/native-supplier-bill-command-owned";
const nativeUnavailable=()=>{throw new ApiError(503,"NATIVE_PROCUREMENT_UNAVAILABLE","The native Procurement service is unavailable.");};
type Context={params:Promise<{holdingSlug:string;id:string}>};
export async function GET(_r:Request,c:Context){try{return Response.json(canonicalDestinationLinks({data:await supplierBillDetail(await requireWorkContext((await c.params).holdingSlug),(await c.params).id)}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const {holdingSlug,id}=await c.params,actor=await requireWorkContext(holdingSlug),body=await r.json();return Response.json(canonicalDestinationLinks({data:await ownedDecideSupplierBill(actor,holdingSlug,id,body,key)??nativeUnavailable()}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
