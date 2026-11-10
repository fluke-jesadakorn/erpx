import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  readNativeWorkCardPreferences,
  saveNativeWorkCardPreferences,
} from "@erpx/server-work_projection/public/work-card-preferences";
type Context = { params: Promise<{ holdingSlug: string }> };
const BODY_LIMIT = 8 * 1024;
export async function GET(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(
      canonicalDestinationLinks({ data: await readNativeWorkCardPreferences(request.headers, actor) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const key = request.headers.get("Idempotency-Key");
    if (!key || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key))
      throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.");
    const body = await request.text();
    if (new TextEncoder().encode(body).length > BODY_LIMIT)
      throw new ApiError(422, "WORK_CARD_PREFERENCES_INVALID", "Choose each shortcut at most once.");
    return Response.json(
      canonicalDestinationLinks({ data: await saveNativeWorkCardPreferences(request.headers, actor, body, key) }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
