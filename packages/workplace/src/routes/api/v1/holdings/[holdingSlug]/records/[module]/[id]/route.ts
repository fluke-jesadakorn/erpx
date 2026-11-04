import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse, ApiError } from "@erpx/server-runtime/public/http";
import { archiveRecord, getRecord, recordContext, updateRecord, updateRecordSchema } from "@erpx/server-records/public/records";
import { recordMentionDisplay } from "@erpx/server-records/public/mention-display";
import { createNativeRecordContextClient, nativeRecordContextAvailable, nativeRecordContextConfiguration, nativeRecordContextFailure } from "@erpx/server-records/public/native-record-context";

export const dynamic = "force-dynamic";
export async function GET(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">) {
  try {
    const {holdingSlug,module,id}=await context.params;
    const actor=await requireWorkContext(holdingSlug);
    const data=new URL(request.url).searchParams.get("view")==="mention"
      ? {record:await recordMentionDisplay(actor,module,id)}
      : await readRecordContext(request,actor,holdingSlug,module,id);
    return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});
  }
  catch(error){return errorResponse(error);}
}
async function readRecordContext(request:Request,actor:Awaited<ReturnType<typeof requireWorkContext>>,holdingSlug:string,moduleId:string,recordId:string) {
  if(!nativeRecordContextAvailable())return recordContext(actor,moduleId,recordId);
  if(!actor.authority||!actor.iam)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before retrying.");
  try {
    const raw=process.env.ERPX_NATIVE_RECORDS_CONFIG;
    if(raw===undefined)throw new ApiError(503,"NATIVE_RECORD_CONTEXT_UNAVAILABLE","The native Records reader is unavailable.");
    return await createNativeRecordContextClient(nativeRecordContextConfiguration(raw)).context(request.headers,{
      userId:actor.userId,holdingId:actor.holdingId,holdingSlug,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,
      assignmentId:actor.assignmentId,role:actor.role,assignmentScope:actor.assignmentScope,modules:actor.modules,
      authority:actor.authority,iam:{permissions:actor.iam.permissions},
    },{holdingSlug,moduleId,recordId});
  } catch(error) {
    const mapped=nativeRecordContextFailure(error);
    if(!mapped)throw error;
    throw new ApiError(mapped.status,mapped.code,mapped.message);
  }
}
export async function DELETE(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">){try{const {holdingSlug,module,id}=await context.params;const expectedVersion=Number(new URL(request.url).searchParams.get("expectedVersion"));if(!Number.isInteger(expectedVersion)||expectedVersion<1)throw new ApiError(422,"VERSION_REQUIRED","Current record version is required.");return Response.json(canonicalDestinationLinks({data:await archiveRecord(await requireWorkContext(holdingSlug),module,id,expectedVersion,request.headers.get("Idempotency-Key")??undefined)}));}catch(error){return errorResponse(error);}}
export async function PATCH(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">) {
  try { const {holdingSlug,module,id}=await context.params; const parsed=updateRecordSchema.safeParse(await request.json());
    if(!parsed.success) throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid fields.");
    const actor=await requireWorkContext(holdingSlug); await getRecord(actor,module,id);
    return Response.json(canonicalDestinationLinks({data:await updateRecord(actor,module,id,parsed.data,request.headers.get("Idempotency-Key")??undefined)}));
  } catch(error){return errorResponse(error);}
}
