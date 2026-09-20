import {requireWorkContext} from '@/server/authorization';
import {requestPublicOrigin} from '@/server/public-origin';
import {ApiError,errorResponse} from '@/server/http';
import {captureSupplierBill,supplierBillWorkspace} from '@/server/supplier-bill-matching';
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(_r:Request,c:Context){try{return Response.json({data:await supplierBillWorkspace(await requireWorkContext((await c.params).holdingSlug))},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(r:Request,c:Context){try{if(r.headers.get('origin')&&r.headers.get('origin')!==requestPublicOrigin(r))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');const key=r.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');return Response.json({data:await captureSupplierBill(await requireWorkContext((await c.params).holdingSlug),await r.json(),key)},{headers:{'Cache-Control':'private, no-store'}});}catch(e){return errorResponse(e);}}
