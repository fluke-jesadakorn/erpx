import { requireWorkContext } from "@/server/authorization";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import {
  changeActionCard,
  collaborationWorkspace,
  createActionCard,
  createRecordRelation,
  removeRecordRelation,
} from "@/server/collaboration-work";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(_r: Request, c: C) {
  try {
    return Response.json(
      {
        data: await collaborationWorkspace(
          await requireWorkContext((await c.params).holdingSlug),
        ),
      },
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
    return Response.json({ data }, { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
