import { betaHandler } from "@erpx/server-manage_erpx/public/beta-api";
import {
  nativeCreateComment,
  nativePostAction,
  nativeReportPost,
  nativeSocialAvailable,
} from "@erpx/server-collaboration/public/native-social-page";

const native = betaHandler("post");
type Context = { params: Promise<{ holdingSlug: string; id: string }> };
const headers = { "Cache-Control": "private, no-store" };

async function workSocialPost(request: Request, context: Context) {
  const { holdingSlug, id } = await context.params;
  if (nativeSocialAvailable() && request.method === "POST") {
    const key = request.headers.get("Idempotency-Key") ?? "";
    const body = (await request.clone().json().catch(() => null)) as Record<
      string,
      unknown
    > | null;
    if (key && body && typeof body.action === "string") {
      if (body.action === "comment")
        return Response.json(
          {
            data: await nativeCreateComment(
              request,
              holdingSlug,
              id,
              {
                body: body.body,
                parentCommentId: body.parentCommentId,
                mentions: body.mentions,
              },
              key,
            ),
          },
          { headers },
        );
      if (["edit", "retract", "appreciate", "follow"].includes(body.action))
        return Response.json(
          { data: await nativePostAction(request, holdingSlug, id, body, key) },
          { headers },
        );
      if (body.action === "report")
        return Response.json(
          {
            data: await nativeReportPost(
              request,
              holdingSlug,
              id,
              String(body.body ?? ""),
              key,
            ),
          },
          { headers },
        );
    }
  }
  return native(request, context);
}
export const GET = workSocialPost;
export const POST = workSocialPost;
