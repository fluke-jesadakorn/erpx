import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {serviceCustomerPreparation,createServiceCustomer,serviceCustomerConfirmation,serviceCustomerSchema} from '@erpx/server-finance/public/service-customer-preparation';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Customer preparation route not found.');
function failure(error:unknown){const response=errorResponse(error instanceof SyntaxError?new ApiError(422,'INVALID_JSON','Use valid JSON.'):error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
export async function GET(request:Request,context:Context){try{const {holdingSlug,path=[]}=await context.params;if(path.length||new URL(request.url).search)throw missing();return Response.json({data:await serviceCustomerPreparation(await requireWorkContext(holdingSlug))},{headers});}catch(error){return failure(error);}}
export async function POST(request:Request,context:Context){try{
 const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const {holdingSlug,path=[]}=await context.params;if(new URL(request.url).search||!(path.length===0||path.length===1&&path[0]==='confirmations'))throw missing();
 const actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=serviceCustomerSchema.parse(await request.json());
 return Response.json({data:path.length?await serviceCustomerConfirmation(actor,input,key):await createServiceCustomer(actor,input,key)},{headers,status:path.length?200:201});
}catch(error){return failure(error);}}
