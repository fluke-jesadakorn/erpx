import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { requestPublicOrigin } from "@erpx/manage-erpx-server/public/public-origin";
import { ApiError, errorResponse } from "@erpx/platform-runtime-server/public/http";
import {nativeLearningCommandRoute} from "@erpx/product-learn-ui/lib/native-learning-command";
import {nativeLearningDetailPage,nativeLearningWorkspacePage} from "@erpx/platform-workplace/lib/native-learning";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    const data = id
      ? await nativeLearningDetailPage(a, id)
      : await nativeLearningWorkspacePage(a);
    return Response.json(
      canonicalDestinationLinks({ data }),
      { headers },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(r: Request, c: C) {
  try {
    if (
      r.headers.get("origin") &&
      r.headers.get("origin") !== requestPublicOrigin(r)
    )
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = r.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const a = await requireWorkContext((await c.params).holdingSlug),
      input = await r.json();
    const result = await nativeLearningCommandRoute(a, input, key, r.headers);
    if (!result)
      throw new ApiError(
        503,
        "NATIVE_LEARNING_UNAVAILABLE",
        "The native Learning command is unavailable.",
      );
    return Response.json(canonicalDestinationLinks({ data: result }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
