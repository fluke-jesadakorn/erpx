import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { archiveRecord, getRecord, recordContext, updateRecord, updateRecordSchema } from "@/server/modules/records/public/records";
import { recordMentionDisplay } from "@/server/modules/records/public/mention-display";

export const dynamic = "force-dynamic";
export async function GET(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">) {
  try { const {holdingSlug,module,id}=await context.params; const actor=await requireWorkContext(holdingSlug); const data=new URL(request.url).searchParams.get("view")==="mention"?{record:await recordMentionDisplay(actor,module,id)}:await recordContext(actor,module,id); return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}}); }
  catch(error){return errorResponse(error);}
}
export async function DELETE(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">){try{const {holdingSlug,module,id}=await context.params;const expectedVersion=Number(new URL(request.url).searchParams.get("expectedVersion"));if(!Number.isInteger(expectedVersion)||expectedVersion<1)throw new ApiError(422,"VERSION_REQUIRED","Current record version is required.");return Response.json(canonicalDestinationLinks({data:await archiveRecord(await requireWorkContext(holdingSlug),module,id,expectedVersion,request.headers.get("Idempotency-Key")??undefined)}));}catch(error){return errorResponse(error);}}
export async function PATCH(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]">) {
  try { const {holdingSlug,module,id}=await context.params; const parsed=updateRecordSchema.safeParse(await request.json());
    if(!parsed.success) throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid fields.");
    const actor=await requireWorkContext(holdingSlug); await getRecord(actor,module,id);
    return Response.json(canonicalDestinationLinks({data:await updateRecord(actor,module,id,parsed.data,request.headers.get("Idempotency-Key")??undefined)}));
  } catch(error){return errorResponse(error);}
}
