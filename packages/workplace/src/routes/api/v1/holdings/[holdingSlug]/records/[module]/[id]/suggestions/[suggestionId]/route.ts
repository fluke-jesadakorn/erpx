import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse,ApiError } from "@erpx/server-runtime/public/http";
import { decideSuggestion,suggestionDecisionSchema } from "@erpx/server-records/public/records";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/suggestions/[suggestionId]">){try{const {holdingSlug,module,id,suggestionId}=await context.params;const parsed=suggestionDecisionSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED","Choose apply or reject.");return Response.json(canonicalDestinationLinks({data:await decideSuggestion(await requireWorkContext(holdingSlug),module,id,suggestionId,parsed.data,request.headers.get("Idempotency-Key")??undefined)}));}catch(error){return errorResponse(error);}}
