import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse,ApiError } from "@/server/modules/runtime/public/http";
import { requestSuggestion,suggestionSchema } from "@/server/modules/records/public/records";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/suggestions">){try{const {holdingSlug,module,id}=await context.params;const parsed=suggestionSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED","Choose a field.");return Response.json(canonicalDestinationLinks({data:await requestSuggestion(await requireWorkContext(holdingSlug),module,id,parsed.data,request.headers.get("Idempotency-Key")??undefined)}),{status:201});}catch(error){return errorResponse(error);}}
