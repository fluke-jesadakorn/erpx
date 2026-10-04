import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { expenseDraftSchema } from "@/server/expense-drafts";
import { expenseOptions,getExpense,canEditExpense } from "@/server/expenses";
import { expenseRoute } from "@/server/expense-routing";
import { activeStaff } from "@/server/staff-policy";
import { namedRoute } from "@/server/request-preview";
import { ApiError,errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const actor=await requireWorkContext((await context.params).holdingSlug),{write}=await holdingResources(actor.holdingId),raw=await request.json(),input=expenseDraftSchema.parse(raw),options=await expenseOptions(actor);
 const current=typeof raw.id==="string"?await getExpense(actor,raw.id):null;
 if(current&&!canEditExpense(actor,current))throw new ApiError(403,"EXPENSE_LOCKED","Only an unsubmitted claim can be previewed.");
 const person=input.personId?options.people.find(p=>p.id===input.personId):null;
 if(input.personId&&!person)throw new ApiError(404,"PERSON_NOT_FOUND","Claimant not found in this workspace.");
 const claimant=String(current?.claimant_user_id??person?.user_id??actor.userId);
 if(current&&person&&person.user_id!==claimant)throw new ApiError(422,"CLAIMANT_IMMUTABLE","Create a separate claim for another person.");
 try {const staff=await activeStaff(write,actor.companyId,claimant);if((actor.branchId&&staff.branch_id!==actor.branchId)||(actor.departmentId&&staff.department_id!==actor.departmentId))throw new ApiError(404,"PERSON_NOT_FOUND","Claimant not found in this workspace.");
 const route=await expenseRoute(write,actor.companyId,claimant,[String(current?.prepared_by??current?.created_by??actor.userId)],false,input.requestKind??"expense");
 return Response.json({data:await namedRoute(write,actor,route)},{headers:{"Cache-Control":"private, no-store"}});
 }catch(error){if(error instanceof ApiError&&[409,422].includes(error.status))return Response.json({data:{steps:[],blockers:[error.message]}},{headers:{"Cache-Control":"private, no-store"}});throw error;}
}catch(error){return errorResponse(error);}}
