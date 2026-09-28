import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import { appointmentOverview, changeAppointment, publishDepartmentFlow } from "@/server/department-authority";
import { decideDepartmentOverride } from "@/server/department-decisions";

type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export const dynamic="force-dynamic";
export async function GET(_request:Request,context:Context){
  try{
    const {holdingSlug}=await context.params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
    return Response.json({data:await appointmentOverview(write,actor)},{headers:{"Cache-Control":"private, no-store"}});
  }catch(error){return errorResponse(error);}
}
export async function POST(request:Request,context:Context){
  try{
    const origin=request.headers.get("origin");
    if(origin && origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const key=request.headers.get("Idempotency-Key");
    if(!key)throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","Idempotency-Key is required.");
    const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId),input=await request.json();
    const data=path[0]==="appointments" && path.length===1?await changeAppointment(write,actor,input,key)
      :path[0]==="flows" && path.length===1?await publishDepartmentFlow(write,actor,input,key)
      :path[0]==="decisions" && path.length===1?await decideDepartmentOverride(actor,input,key)
      :null;
    if(!data)throw new ApiError(404,"NOT_FOUND","Authority operation not found.");
    return Response.json({data});
  }catch(error){return errorResponse(error);}
}
