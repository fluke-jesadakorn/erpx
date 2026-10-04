import { requireWorkContext } from "@/server/authorization";
import { expenseOptions } from "@/server/expenses";
import { leaveSummary } from "@/server/leave-self-service";
import { requestPageOptions } from "@/server/request-page";
import { holdingResources } from "@/server/runtime";
import { getWorkCapabilities } from "@/server/work-capabilities";
import { ApiError, errorResponse } from "@/server/http";
export async function GET(request:Request,context:{params:Promise<{holdingSlug:string}>}) {
 try {
  const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),kind=new URL(request.url).searchParams.get("kind");
  const capabilities=await getWorkCapabilities(write,actor);
  if(!capabilities.creation.some(c=>c.kind===kind))throw new ApiError(403,"INTAKE_FORBIDDEN","This action is unavailable in your current role.");
  const data=kind==="expense"?{kind,options:await expenseOptions(actor)}:kind==="leave"?{kind,summary:await leaveSummary(actor),companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId}:kind==="purchase_request"||kind==="service_request"?{kind,options:await requestPageOptions(write,actor)}:null;
  if(!data)throw new ApiError(422,"INTAKE_KIND","Choose a supported request type.");
  return Response.json({data},{headers:{"Cache-Control":"private, no-store"}});
 }catch(error){return errorResponse(error);}
}
