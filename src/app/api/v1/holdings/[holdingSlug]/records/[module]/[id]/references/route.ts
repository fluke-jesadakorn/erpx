import { requireWorkContext } from "@/server/authorization";
import { errorResponse,ApiError } from "@/server/http";
import { addReference,addReferenceSchema } from "@/server/records";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/references">){try{const {holdingSlug,module,id}=await context.params;const parsed=addReferenceSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid reference.");return Response.json({data:await addReference(await requireWorkContext(holdingSlug),module,id,parsed.data)},{status:201});}catch(error){return errorResponse(error);}}
