import { iamApi } from "@erpx/server-identity_access/public/iam-api";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import {
  nativePlatformIamAvailable,
  nativePlatformIamCommand,
  nativePlatformIamFailure,
  nativePlatformAdminFailure,
  type NativePlatformIamCommand,
} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import { productHostForHost, requestProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export const dynamic = "force-dynamic";

const responseHeaders = {
  "Cache-Control": "private, no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
};

/** Only the five reviewed command paths move. Every other Platform IAM path,
 * including every GET and the response reads, stays on the retained handler. */
function commandFor(
  request: Request,
  path: string[],
): NativePlatformIamCommand | undefined {
  if (request.method !== "POST") return undefined;
  if (path.length === 1 && path[0] === "roles") return { kind: "role-save" };
  if (path.length === 2 && path[0] === "reviews")
    return { kind: "review-decide", reviewId: path[1]! };
  if (path.length === 1 && path[0] === "access-reviews")
    return { kind: "access-review-start" };
  if (path.length === 3 && path[0] === "access-review-items" && path[2] === "decide")
    return { kind: "access-review-item-decide", itemId: path[1]! };
  if (path.length === 3 && path[0] === "access-reviews" && path[2] === "cancel")
    return { kind: "access-review-cancel", campaignId: path[1]! };
  return undefined;
}

async function handle(
  request: Request,
  { params }: { params: Promise<{ path?: string[] }> },
) {
  const path = (await params).path ?? [];
  const command = commandFor(request, path);
  if (!command) return iamApi(request, null, path);
  if (!nativePlatformIamAvailable()) {
    const response = errorResponse(
      new ApiError(
        503,
        "NATIVE_PLATFORM_IAM_REQUIRED",
        "This Platform IAM command requires the native owner service.",
      ),
    );
    for (const [key, value] of Object.entries(responseHeaders))
      response.headers.set(key, value);
    return response;
  }
  try {
    const origin = requestProductOrigin("admin", request.headers);
    if (!origin || productHostForHost(request.headers.get("host")) !== "admin")
      throw new ApiError(404, "NOT_FOUND", "Not found.");
    if (request.headers.get("origin") !== origin)
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use ERPX Administration to submit this request.",
      );
    if (
      request.headers.get("content-type")?.split(";", 1)[0].trim().toLowerCase() !==
      "application/json"
    )
      throw new ApiError(415, "JSON_REQUIRED", "Submit a JSON request.");
    const body = await request.text();
    if (new TextEncoder().encode(body).length > 256 * 1024)
      throw new ApiError(413, "REQUEST_TOO_LARGE", "This request is too large.");
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "A valid Idempotency-Key is required.",
      );
    const { work, platform } = await requireManageErpxContext();
    const data = await nativePlatformIamCommand(
      work,
      platform,
      request.headers,
      command,
      body,
      key,
    );
    if (!data)
      throw new ApiError(
        503,
        "NATIVE_PLATFORM_IAM_UNAVAILABLE",
        "The native Platform IAM service is unavailable.",
      );
    return Response.json({ data }, { headers: responseHeaders });
  } catch (error) {
    const mapped =
      nativePlatformIamFailure(error) ?? nativePlatformAdminFailure(error);
    const response = errorResponse(
      mapped ? new ApiError(mapped.status, mapped.code, mapped.message) : error,
    );
    for (const [key, value] of Object.entries(responseHeaders))
      response.headers.set(key, value);
    return response;
  }
}

export { handle as GET, handle as POST };
