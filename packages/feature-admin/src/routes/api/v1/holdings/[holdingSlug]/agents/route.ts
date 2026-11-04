import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  nativeAgentEvidenceAvailable,
  nativeAgentEvidenceFailure,
  nativeAgentEvidenceFromHeaders,
} from "@erpx/server-agent/public/native-agent-evidence-page";
import {
  agentWorkspace,
  changeAgentRecord,
  changeAgentPool,
  changePrivateSession,
  createAgent,
  createAgentAssignment,
  createAgentPool,
  createAgentVersion,
} from "@erpx/server-agent/public/agent-governance";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(request: Request, c: C) {
  try {
    const holdingSlug = (await c.params).holdingSlug,
      actor = await requireWorkContext(holdingSlug),
      evidenceView = new URL(request.url).searchParams.get("view") === "evidence";
    let evidence;
    if (nativeAgentEvidenceAvailable()) {
      try {
        evidence = await nativeAgentEvidenceFromHeaders(actor, holdingSlug, request.headers);
      } catch (error) {
        const failure = nativeAgentEvidenceFailure(error);
        throw failure ? new ApiError(failure.status, failure.code, failure.message) : error;
      }
    }
    if (evidenceView) {
      if (!evidence)
        throw new ApiError(
          503,
          "NATIVE_AGENT_UNAVAILABLE",
          "The native Agent evidence is unavailable.",
        );
      return Response.json(
        canonicalDestinationLinks({
          data: {
            now: evidence.now,
            invocations: evidence.invocations,
            sessions: evidence.sessions,
          },
        }),
        { headers },
      );
    }
    const workspace = await agentWorkspace(actor, false),
      data = evidence
        ? { ...workspace, invocations: evidence.invocations, sessions: evidence.sessions }
        : { ...workspace, invocations: [], sessions: [] };
    return Response.json(canonicalDestinationLinks({ data }), { headers });
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
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
