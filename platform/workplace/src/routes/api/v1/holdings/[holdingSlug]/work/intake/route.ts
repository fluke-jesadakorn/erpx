import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import {expenseOptions} from '@erpx/people-payroll-server/public/expenses';
import { leaveSummary } from "@erpx/people-payroll-server/public/leave-self-service";
import { workforcePresentationContext } from "@erpx/people-payroll-server/public/workforce-presentation";
import { requestPageOptions } from "@erpx/records-server/public/request-page";
import { holdingResources } from "../../../../../../../../../runtime/server/src/public/runtime";
import { getWorkCapabilities } from "@erpx/work-projection-server/public/work-capabilities";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
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
