import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { nativeCreateComment, nativePostComments, nativeSocialAvailable } from "@erpx/server-collaboration/public/native-social-page";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { createCommentSchema } from "@erpx/server-collaboration/public/social";

export async function POST(request: Request, context: RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/comments">) {
  try {
    const { holdingSlug, id } = await context.params;
    const commandId = request.headers.get("Idempotency-Key");
    if (!commandId) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.");
    const parsed = createCommentSchema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid comment.");
    await requireWorkContext(holdingSlug);
    if (!nativeSocialAvailable())
      throw new ApiError(
        503,
        "NATIVE_SOCIAL_UNAVAILABLE",
        "The native Social service is unavailable.",
      );
    return Response.json(canonicalDestinationLinks({ data: await nativeCreateComment(request, holdingSlug, id, parsed.data, commandId) }), { status: 201 });
  } catch (error) { return errorResponse(error); }
}

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/posts/[id]/comments">){
  try {
    const {holdingSlug,id}=await context.params;
    await requireWorkContext(holdingSlug);
    if(!nativeSocialAvailable())
      throw new ApiError(503,"NATIVE_SOCIAL_UNAVAILABLE","The native Social service is unavailable.");
    const cursor=new URL(request.url).searchParams.get("cursor")??undefined;
    const page=(await nativePostComments(request,holdingSlug,id,cursor)) as {
      items:Array<Record<string,unknown>&{rich_content?:unknown;rich_content_json?:string|null}>;
      nextCursor:string|null;
      hasMore:boolean;
    };
    // The retained conversation client reads the parsed `richContent` projection.
    const items=page.items.map(({rich_content_json,...item})=>({
      ...item,
      richContent:item.rich_content??parseRichContent(rich_content_json),
    }));
    return Response.json(canonicalDestinationLinks({data:{...page,items}}),{headers:{"Cache-Control":"private, no-store"}});
  } catch(error){return errorResponse(error);}
}

function parseRichContent(value:string|null|undefined){
  if(!value)return null;
  try{return JSON.parse(value) as unknown;}catch{return null;}
}
