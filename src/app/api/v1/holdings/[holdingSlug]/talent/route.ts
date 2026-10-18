import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  changeTalentRecord,
  createTalentApplication,
  createTalentCandidate,
  createTalentOffer,
  createTalentProposal,
  createTalentRequisition,
  talentDetail,
  talentWorkspace,
} from "@/server/modules/talent/public/talent";
import { candidatePrivacySchema,changeCandidatePrivacy } from "@/server/modules/people_payroll/public/people-handoffs";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    return Response.json(
      canonicalDestinationLinks({ data: id ? await talentDetail(a, id) : await talentWorkspace(a) }),
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
    if(i.entity==="candidate-privacy"&&typeof i.id==="string")data=await changeCandidatePrivacy(a,i.id,candidatePrivacySchema.parse({action:i.action,expectedVersion:i.expectedVersion,reason:i.reason}),key);
    else if (typeof i.id === "string"){
      if(!["requisition","application","proposal","offer"].includes(i.entity))throw new ApiError(422,"TALENT_ENTITY_REQUIRED","Choose a registered Recruitment record type.");
      data = await changeTalentRecord(a, i.entity, i.id, i, key);
    }
    else if (i.entity === "requisition")
      data = await createTalentRequisition(a, i, key);
    else if (i.entity === "candidate")
      data = await createTalentCandidate(a, i, key);
    else if (i.entity === "application")
      data = await createTalentApplication(a, i, key);
    else if (i.entity === "proposal")
      data = await createTalentProposal(a, i, key);
    else if(i.entity==="offer")data = await createTalentOffer(a, i, key);
    else throw new ApiError(422,"TALENT_ENTITY_REQUIRED","Choose a registered Recruitment record type.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
