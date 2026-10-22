import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse,ApiError } from "@erpx/server-runtime/public/http";
import { addPrincipalMention,addPrincipalMentionSchema } from "@erpx/server-records/public/records";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/mentions">){try{const {holdingSlug,module,id}=await context.params;const parsed=addPrincipalMentionSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED","Choose a valid mention target.");return Response.json(canonicalDestinationLinks({data:await addPrincipalMention(await requireWorkContext(holdingSlug),module,id,parsed.data,request.headers.get("Idempotency-Key")??undefined)}),{status:201});}catch(error){return errorResponse(error);}}
