import { requireWorkContext } from "@/server/authorization";
import { requestPublicOrigin } from "@/server/public-origin";
import { ApiError, errorResponse } from "@/server/http";
import {
  agentWorkspace,
  changeAgentRecord,
  changeAgentPool,
  changePrivateSession,
  createAgent,
  createAgentAssignment,
  createAgentPool,
  createAgentVersion,
} from "@/server/agent-governance";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(_: Request, c: C) {
  try {
    return Response.json(
      {
        data: await agentWorkspace(
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
      i = await r.json();
    let data;
    if (typeof i.id === "string" && i.entity === "pool") data = await changeAgentPool(a,i.id,i,key);
    else if (typeof i.id === "string" && i.entity === "session") data = await changePrivateSession(a,i.id,i,key);
    else if (typeof i.id === "string" && ["agent","version","assignment"].includes(i.entity))
      data = await changeAgentRecord(a, i.entity, i.id, i, key);
    else if (i.entity === "agent") data = await createAgent(a, i, key);
    else if (i.entity === "version") data = await createAgentVersion(a, i, key);
    else if (i.entity === "pool") data = await createAgentPool(a, i, key);
    else if (i.entity === "assignment") data = await createAgentAssignment(a, i, key);
    else throw new ApiError(422,"AGENT_ENTITY_REQUIRED","Choose an Agent governance command.");
    return Response.json({ data }, { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
