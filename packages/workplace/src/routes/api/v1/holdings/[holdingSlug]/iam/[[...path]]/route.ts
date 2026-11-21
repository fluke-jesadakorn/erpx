import { iamApi } from "@erpx/server-identity_access/public/iam-api";
import { nativeIamAvailable, nativeIamCommand } from "@erpx/server-identity_access/public/native-iam";
import { nativeDirectoryConnectionAvailable, nativeDirectoryConnectionCommand } from "@erpx/server-identity_access/public/native-directory-connection";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export const dynamic = "force-dynamic";

function nativePath(path: string[]): string | null {
  if (path[0] === "roles" && path.length === 1) return "roles";
  if (path[0] === "reviews" && path[2] === "decide" && typeof path[1] === "string")
    return `reviews/${encodeURIComponent(path[1])}/decide`;
  if (path[0] === "access-reviews") {
    if (path.length === 1) return "access-reviews";
    if (path[2] === "cancel" && typeof path[1] === "string")
      return `access-reviews/${encodeURIComponent(path[1])}/cancel`;
  }
  if (path[0] === "access-review-items" && path[2] === "decide" && typeof path[1] === "string")
    return `access-review-items/${encodeURIComponent(path[1])}/decide`;
  return null;
}

function nativeConnection(path: string[]): { operation: string; id: string | null } | null {
  if (path[0] !== "connections") return null;
  if (path.length === 1) return { operation: "save", id: null };
  if (path.length === 3 && ["test", "activate", "disable"].includes(path[2]!))
    return { operation: path[2]!, id: path[1]! };
  return null;
}

async function handle(
  request: Request,
  { params }: { params: Promise<{ holdingSlug: string; path?: string[] }> },
) {
  const p = await params;
  const connection = p.path ? nativeConnection(p.path) : null;
  if (request.method === "POST" && connection) {
    if (!nativeDirectoryConnectionAvailable())
      return errorResponse(
        new ApiError(
          503,
          "NATIVE_DIRECTORY_CONNECTION_REQUIRED",
          "This Directory connection command requires the native owner service.",
        ),
      );
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      return errorResponse(
        new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required."),
      );
    const body = await request.json().catch(() => null);
    const data = await nativeDirectoryConnectionCommand(
      request,
      p.holdingSlug,
      connection.operation,
      connection.id,
      body,
      key,
    );
    return Response.json({ data }, { headers: { "Cache-Control": "no-store" } });
  }
  const native = p.path ? nativePath(p.path) : null;
  if (request.method === "POST" && native) {
    if (!nativeIamAvailable())
      return errorResponse(
        new ApiError(
          503,
          "NATIVE_IAM_REQUIRED",
          "This IAM command requires the native owner service.",
        ),
      );
    const body = await request.json().catch(() => null);
    const data = await nativeIamCommand(request, p.holdingSlug, native, body);
    return Response.json({ data }, { headers: { "Cache-Control": "no-store" } });
  }
  return iamApi(request, p.holdingSlug, p.path ?? []);
}
export { handle as GET, handle as POST };
