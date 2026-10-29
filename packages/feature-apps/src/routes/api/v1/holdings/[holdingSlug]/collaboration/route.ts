import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  changeActionCard,
  collaborationExport,
  collaborationWorkspace,
  createActionCard,
  createRecordRelation,
  removeRecordRelation,
} from "@erpx/server-collaboration/public/collaboration-work";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug);
    if (new URL(r.url).searchParams.get("format") === "csv") {
      const result = await collaborationExport(a);
      return new Response(result.content, {
        headers: {
          "Content-Type": "text/csv; charset=utf-8",
          "Content-Disposition": 'attachment; filename="erpx-collaboration.csv"',
          "Cache-Control": "private, no-store",
          ETag: result.sha256,
        },
      });
    }
    return Response.json(
      canonicalDestinationLinks({
        data: await collaborationWorkspace(a),
      }),
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
    const data =
      input.entity === "relation"
        ? input.action === "remove"
          ? await removeRecordRelation(
              a,
              input.id,
              Number(input.expectedVersion),
              String(input.reason),
              key,
            )
          : await createRecordRelation(a, input, key)
        : typeof input.id === "string"
          ? await changeActionCard(a, input.id, input, key)
          : await createActionCard(a, input, key);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
