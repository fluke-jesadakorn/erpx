import {requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {requestPublicOrigin} from '@/server/modules/manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@/server/modules/runtime/public/http';
import {hireHandoffWorkspace} from '@/server/modules/people_payroll/public/hire-handoff-workspace';
import {createHireHandoffPayGroup,hireHandoffPayGroupSchema} from '@/server/modules/people_payroll/public/hire-handoff-pay-group';
type Context={params:Promise<{holdingSlug:string;id:string}>};
const headers={'Cache-Control':'private, no-store'};
const failure=(error:unknown)=>{const response=errorResponse(error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;};
export async function GET(_request:Request,context:Context){try{const {holdingSlug,id}=await context.params,actor=await requireWorkContext(holdingSlug);return Response.json({data:await hireHandoffWorkspace(actor,id,{strictProduct:true})},{headers});}catch(error){return failure(error);}}
export async function POST(request:Request,context:Context){try{const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');const {holdingSlug,id}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=hireHandoffPayGroupSchema.parse(await request.json());return Response.json({data:await createHireHandoffPayGroup(actor,id,input,key)},{status:201,headers});}catch(error){return failure(error);}}
