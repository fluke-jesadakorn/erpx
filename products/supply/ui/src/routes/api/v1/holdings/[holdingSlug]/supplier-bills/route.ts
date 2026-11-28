import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {supplierBillWorkspace} from "@erpx/procurement-server/public/supplier-bill-matching";
import {ownedCaptureSupplierBill} from "@erpx/procurement-ui/public/native-supplier-bill-command-owned";
const nativeUnavailable=()=>{throw new ApiError(503,"NATIVE_PROCUREMENT_UNAVAILABLE","The native Procurement service is unavailable.");};
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(_r:Request,c:Context){try{return Response.json(canonicalDestinationLinks({data:await supplierBillWorkspace(await requireWorkContext((await c.params).holdingSlug))}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const holdingSlug=(await c.params).holdingSlug,actor=await requireWorkContext(holdingSlug),body=await r.json();return Response.json(canonicalDestinationLinks({data:await ownedCaptureSupplierBill(actor,holdingSlug,body,key)??nativeUnavailable()}),{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
