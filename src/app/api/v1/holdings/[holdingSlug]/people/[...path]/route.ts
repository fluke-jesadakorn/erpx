import { requestPublicOrigin } from "@/server/public-origin";
import { grantSchema,grantPersonAccess,revokePersonAccess,invitePerson } from "@/server/account-access";
import { directorySchema, directoryUpdateSchema, directoryStatusSchema, saveDirectoryPerson, setDirectoryStatus } from "@/server/people-directory";
import { z } from "zod";
import { requireWorkContext } from "@/server/authorization";
import { ApiError, errorResponse } from "@/server/http";
import {
  payrollOptions,assignPayGroupSchema,assignPayGroup,
  compensationSchema,
  compensationTransitionSchema,
  confirmPaymentBatch,
  countryPackSchema,
  createCompensation,
  createCountryPack,
  createEngagement,
  createPayrollRun,
  createPayGroup,
  createPerson,
  createPaymentBatch,
  createPersonSchema,
  createEngagementSchema,
  exportPaymentBatch,
  getPayrollRun,
  listPayrollRuns,
  listPeople,
  getPerson,
  peopleSummary,
  payrollRunSchema,
  payGroupSchema,
  paymentBatchSchema,
  paymentConfirmationSchema,
  calculatePayrollRun,
  approvePayrollRun,
  payrollPostSchema,
  postPayroll,
  approvePaymentBatch,
  getPaymentFile,
  listCountryPacks,
  transitionCountryPack,
  countryPackTransitionSchema,
  timeEntrySchema,
  timeEntryUpdateSchema, timeEntryReviewSchema, updateTimeEntry, rejectTimeEntry,
  createTimeEntry,
  approveTimeEntry,
  transitionCompensation,
  engagementTransitionSchema,
  transitionEngagement,
} from "@/server/people-payroll";

type Context = { params: Promise<{ holdingSlug: string; path: string[] }> };
function parts(path: string[]) { return path.filter(Boolean).map((part) => decodeURIComponent(part)); }
function command(request: Request) { const value = request.headers.get("Idempotency-Key"); if (!value) throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","Idempotency-Key is required."); return value; }
async function json(request: Request) { try { return await request.json(); } catch { throw new ApiError(422,"INVALID_JSON","Request body must be valid JSON."); } }
function parsed<T>(result: z.ZodSafeParseResult<T>): T {
  if (!result.success) throw new ApiError(422,"VALIDATION_FAILED",result.error.issues[0]?.message ?? "Invalid request.");
  return result.data;
}

export const dynamic = "force-dynamic";

export async function GET(request: Request, context: Context) {
  try {
    const { holdingSlug, path } = await context.params; const actor = await requireWorkContext(holdingSlug); const p = parts(path);
    if (p.length === 0 || (p[0] === "people" && !p[1])) return Response.json({ data: await listPeople(actor) });
    if (p[0] === "people" && p[1]) return Response.json({ data: await getPerson(actor, p[1]) });
    if (p[0] === "pay-groups") return Response.json({data:await payrollOptions(actor)});
    if (p[0] === "summary") return Response.json({ data: await peopleSummary(actor) });
    if (p[0] === "country-packs") return Response.json({ data: await listCountryPacks(actor) });
    if (p[0] === "payroll" && p[1] === "runs" && p.length === 2) return Response.json({ data: await listPayrollRuns(actor) });
    if (p[0] === "payroll" && p[1] === "runs" && p[2]) return Response.json({ data: await getPayrollRun(actor,p[2]) });
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "file") { const result=await getPaymentFile(actor,p[2]); const headers=new Headers({"Content-Type":"text/csv","Content-Disposition":`attachment; filename="${p[2]}.csv"`,"Cache-Control":"private, no-store"}); if(result.checksum)headers.set("ETag",result.checksum); return new Response(result.object.body,{status:200,headers}); }
    throw new ApiError(404,"NOT_FOUND","People resource not found.");
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  try {
    const origin=request.headers.get("origin"); if(origin && origin!==requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin for this action.");
    const { holdingSlug, path } = await context.params; const actor = await requireWorkContext(holdingSlug); const p = parts(path); const input = await json(request); const id = command(request);
    if(p[0]==="directory"&&p[2]==="invite")return Response.json({data:await invitePerson(actor,p[1],parsed(grantSchema.safeParse(input)))});
    if(p[0]==="directory"&&p[2]==="grant")return Response.json({data:await grantPersonAccess(actor,p[1],parsed(grantSchema.safeParse(input)),id)});
    if(p[0]==="directory"&&p[2]==="revoke"){const data=parsed(z.object({assignmentId:z.string().min(1)}).safeParse(input));return Response.json({data:await revokePersonAccess(actor,p[1],data.assignmentId,id)});}
    if (p[0] === "directory" && p.length === 1) return Response.json({data:await saveDirectoryPerson(actor,null,{...parsed(directorySchema.safeParse(input)),expectedVersion:1},id)},{status:201});
    if (p[0] === "directory" && p.length === 2) return Response.json({data:await saveDirectoryPerson(actor,p[1],parsed(directoryUpdateSchema.safeParse(input)),id)});
    if (p[0] === "directory" && p[2] === "status") return Response.json({data:await setDirectoryStatus(actor,p[1],parsed(directoryStatusSchema.safeParse(input)),id)});
    if (p[0] === "people" && p.length === 1) { const data=parsed(createPersonSchema.safeParse(input)); return Response.json({data:await createPerson(actor,data,id)},{status:201}); }
    if (p[0] === "engagements" && p.length === 1) { const data=parsed(createEngagementSchema.safeParse(input)); return Response.json({data:await createEngagement(actor,data,id)},{status:201}); }
    if(p[0]==="engagements"&&p[2]==="pay-group")return Response.json({data:await assignPayGroup(actor,p[1],parsed(assignPayGroupSchema.safeParse(input)),id)});
    if (p[0] === "engagements" && p[2] === "transition") { const data=parsed(engagementTransitionSchema.safeParse(input)); return Response.json({data:await transitionEngagement(actor,p[1],data,id)}); }
    if (p[0] === "compensation" && p.length === 1) { const data=parsed(compensationSchema.safeParse(input)); return Response.json({data:await createCompensation(actor,data,id)},{status:201}); }
    if (p[0] === "compensation" && p[2] === "transition") { const data=parsed(compensationTransitionSchema.safeParse(input)); return Response.json({data:await transitionCompensation(actor,p[1],data,id)}); }
    if (p[0] === "pay-groups" && p.length === 1) { const data=parsed(payGroupSchema.safeParse(input)); return Response.json({data:await createPayGroup(actor,data,id)},{status:201}); }
    if (p[0] === "country-packs" && p.length === 1) { const data=parsed(countryPackSchema.safeParse(input)); return Response.json({data:await createCountryPack(actor,data,id)},{status:201}); }
    if (p[0] === "country-packs" && p[2] === "transition") { const data=parsed(countryPackTransitionSchema.safeParse(input)); return Response.json({data:await transitionCountryPack(actor,p[1],data,id)}); }
    if (p[0] === "expenses") throw new ApiError(410,"EXPENSE_API_MOVED","Use the /expenses workflow for claims, receipts, approval and reimbursement.");
    if (p[0] === "time-entries" && p.length === 1) { const data=parsed(timeEntrySchema.safeParse(input)); return Response.json({data:await createTimeEntry(actor,data,id)},{status:201}); }
    if (p[0] === "time-entries" && p.length === 2) return Response.json({data:await updateTimeEntry(actor,p[1],parsed(timeEntryUpdateSchema.safeParse(input)),id)});
    if (p[0] === "time-entries" && p[2] === "reject") return Response.json({data:await rejectTimeEntry(actor,p[1],parsed(timeEntryReviewSchema.safeParse(input)),id)});
    if (p[0] === "time-entries" && p[2] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json({data:await approveTimeEntry(actor,p[1],data.expectedVersion,id)}); }
    if (p[0] === "payroll" && p[1] === "runs" && p.length === 2) { const data=parsed(payrollRunSchema.safeParse(input)); return Response.json({data:await createPayrollRun(actor,data,id)},{status:201}); }
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "calculate") return Response.json({data:await calculatePayrollRun(actor,p[2],id)});
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json({data:await approvePayrollRun(actor,p[2],data.expectedVersion,id)}); }
    if (p[0] === "payroll" && p[1] === "runs" && p[3] === "post") { parsed(payrollPostSchema.safeParse(input)); return Response.json({data:await postPayroll(actor)}); }
    if (p[0] === "payments" && p[1] === "batches" && p.length === 2) { const data=parsed(paymentBatchSchema.safeParse(input)); return Response.json({data:await createPaymentBatch(actor,data,id)},{status:201}); }
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "approve") { const data=parsed(z.object({expectedVersion:z.number().int().positive()}).safeParse(input)); return Response.json({data:await approvePaymentBatch(actor,p[2],data.expectedVersion,id)}); }
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "export") return Response.json({data:await exportPaymentBatch(actor,p[2],id)});
    if (p[0] === "payments" && p[1] === "batches" && p[3] === "confirm") { const data=parsed(paymentConfirmationSchema.safeParse(input)); return Response.json({data:await confirmPaymentBatch(actor,p[2],data,id)}); }
    throw new ApiError(404,"NOT_FOUND","People resource not found.");
  } catch (error) { return errorResponse(error); }
}
