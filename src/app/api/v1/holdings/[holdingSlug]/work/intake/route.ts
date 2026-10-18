import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { expenseOptions } from "@/server/modules/people_payroll/public/expenses";
import { leaveSummary } from "@/server/modules/people_payroll/public/leave-self-service";
import { workforcePresentationContext } from "@/server/modules/people_payroll/public/workforce-presentation";
import { requestPageOptions } from "@/server/modules/records/public/request-page";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { getWorkCapabilities } from "@/server/modules/work_projection/public/work-capabilities";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
export async function GET(request:Request,context:{params:Promise<{holdingSlug:string}>}) {
 try {
  const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),kind=new URL(request.url).searchParams.get("kind");
  const capabilities=await getWorkCapabilities(write,actor);
  if(!capabilities.creation.some(c=>c.kind===kind))throw new ApiError(403,"INTAKE_FORBIDDEN","This action is unavailable in your current role.");
  const data=kind==="expense"?{kind,options:await expenseOptions(actor)}:kind==="leave"?{kind,summary:await leaveSummary(actor),workContext:workforcePresentationContext(actor),companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId}:kind==="purchase_request"||kind==="service_request"?{kind,options:await requestPageOptions(write,actor)}:null;
  if(!data)throw new ApiError(422,"INTAKE_KIND","Choose a supported request type.");
  return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});
 }catch(error){return errorResponse(error);}
}
