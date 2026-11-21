import { providerCommercialApi } from "@erpx/server-commercial/public/provider-commercial";
import {
  createNativePackageGrantsClient,
  nativePackageGrantsConfiguration,
  nativePackageGrantsConfigured,
  nativePackageGrantsFailure,
  type NativePackageGrantActor,
} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import { requirePlatformContext } from "@erpx/server-identity_access/public/authorization";
import { productHostForHost, requestProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export const dynamic = "force-dynamic";

const responseHeaders = {
  "Cache-Control": "private, no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
};

/** Only the two reviewed batch grant paths move. Every other Platform
 * Commercial path, including `legacy-grants/*`, stays on the retained handler. */
const operations: ReadonlyMap<string, "preview" | "apply"> = new Map([
  ["packages/preview", "preview"],
  ["packages/apply", "apply"],
]);

function invalid(code: string, message: string): never {
  throw new ApiError(422, code, message);
}

async function handle(
  request: Request,
  { params }: { params: Promise<{ path?: string[] }> },
) {
  const path = (await params).path ?? [];
  const operation = request.method === "POST" ? operations.get(path.join("/")) : undefined;
  if (!operation || !nativePackageGrantsConfigured())
    return providerCommercialApi(request, path);
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
    if (body.length > 200_000)
      throw new ApiError(413, "REQUEST_TOO_LARGE", "This request is too large.");
    let input: unknown;
    try {
      input = JSON.parse(body);
    } catch {
      throw new ApiError(422, "INVALID_JSON", "Submit a valid JSON request.");
    }
    if (!input || typeof input !== "object" || Array.isArray(input))
      invalid("VALIDATION_FAILED", "Use the reviewed Company package request.");
    const value = input as Record<string, unknown>;
    if (!value.request || typeof value.request !== "object" || Array.isArray(value.request))
      invalid("VALIDATION_FAILED", "Use the reviewed Company package request.");
    const reviewed = value.request as Record<string, unknown>;
    const idempotencyKey = reviewed.idempotencyKey;
    if (typeof idempotencyKey !== "string" || !/^[A-Za-z0-9_-]{16,128}$/.test(idempotencyKey))
      invalid("VALIDATION_FAILED", "Use the reviewed Company package request.");
    if (operation === "apply") {
      if (typeof value.token !== "string" || value.token.length === 0 || value.token.length > 8192)
        invalid("VALIDATION_FAILED", "Use the reviewed Company package request.");
      if (request.headers.get("Idempotency-Key") !== idempotencyKey)
        throw new ApiError(
          422,
          "IDEMPOTENCY_KEY_REQUIRED",
          "Use the reviewed Idempotency-Key.",
        );
    }
    const platform = await requirePlatformContext();
    const iam = platform.iam;
    if (!iam?.roleRevision)
      throw new ApiError(
        403,
        "PLATFORM_COMMERCIAL_REQUIRED",
        "Current Platform Commercial authority is required.",
      );
    const actor: NativePackageGrantActor = {
      userId: platform.userId,
      platformAssignmentId: platform.assignmentId,
      platformRole: platform.role,
      roleRevision: iam.roleRevision,
      modules: platform.modules ?? [],
      permissions: iam.permissions ?? [],
    };
    const client = createNativePackageGrantsClient(
      nativePackageGrantsConfiguration(process.env.ERPX_NATIVE_COMMERCIAL_CONFIG!),
    );
    const data =
      operation === "preview"
        ? await client.preview(request.headers, actor, body)
        : await client.apply(request.headers, actor, body, idempotencyKey);
    return Response.json({ data }, { headers: responseHeaders });
  } catch (error) {
    const mapped = nativePackageGrantsFailure(error);
    const response = errorResponse(
      mapped ? new ApiError(mapped.status, mapped.code, mapped.message) : error,
    );
    for (const [key, value] of Object.entries(responseHeaders)) response.headers.set(key, value);
    return response;
  }
}

export { handle as GET, handle as POST };
