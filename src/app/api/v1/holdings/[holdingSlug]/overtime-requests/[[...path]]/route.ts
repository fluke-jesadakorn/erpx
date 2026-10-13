import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { commandOvertime, createOvertime, getOvertime, listOvertime, previewOvertime, setOvertimeEligibility } from "@/server/modules/people_payroll/public/overtime";
import { overtimeCommandSchema, overtimeDetailsSchema } from "@/lib/people_payroll/overtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export const dynamic="force-dynamic";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export async function GET(_request:Request,context:Context) {
 try {const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug);
 return Response.json({data:path.length?await getOvertime(actor,path[0]):await listOvertime(actor)},{headers:{'Cache-Control':'private, no-store'}});
 }catch(e){return errorResponse(e);}
}
export async function POST(request:Request,context:Context) {
 try {const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),raw=await request.json();
 if(path[0]==='preview')return Response.json({data:await previewOvertime(actor,overtimeDetailsSchema.parse(raw),raw.actual===true,typeof raw.id==='string'?(await getOvertime(actor,raw.id)).id:null)});
 const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 return Response.json({data:path[0]==='eligibility'?await setOvertimeEligibility(actor,raw,key):path.length?await commandOvertime(actor,path[0],overtimeCommandSchema.parse(raw),key):await createOvertime(actor,overtimeDetailsSchema.parse(raw),key)});
 }catch(e){return errorResponse(e);}
}
export const PATCH=POST;
