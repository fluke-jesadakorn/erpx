import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {manufacturingPostingSetup,initializeManufacturingPostingSetup,manufacturingPostingSetupSchema} from '@erpx/server-manufacturing/public/posting-setup';
type Context={params:Promise<{holdingSlug:string}>};
const headers={'Cache-Control':'private, no-store'};
const failure=(error:unknown)=>{const response=errorResponse(error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;};
export async function GET(_request:Request,context:Context){
 try{const actor=await requireWorkContext((await context.params).holdingSlug);return Response.json({data:await manufacturingPostingSetup(actor)},{headers});}catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{
  const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');
  const actor=await requireWorkContext((await context.params).holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
  const input=manufacturingPostingSetupSchema.parse(await request.json());return Response.json({data:await initializeManufacturingPostingSetup(actor,input,key)},{status:201,headers});
 }catch(error){return failure(error);}
}
