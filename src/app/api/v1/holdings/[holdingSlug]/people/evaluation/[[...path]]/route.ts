import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { addEvaluationEvidence, approveEvaluationCompensation, cancelEvaluationCompensation, confirmEvaluationPayBasis, createEvaluationAssessment, createEvaluationCompensationDraft, evaluationDetail, evaluationWorkspace, importEvaluationMarket, readEvaluationEvidence, saveEvaluationAssessment, saveEvaluationDefinition, transitionEvaluationAssessment } from "@/server/modules/people_payroll/public/people-evaluation";
import { evaluationAiStatus, queueEvaluationAi } from "@/server/modules/people_payroll/public/people-evaluation-ai";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(request: Request, { params }: Context) {
  try {
    const { holdingSlug, path = [] } = await params, actor = await requireWorkContext(holdingSlug);
    let data;
    if (!path.length || (path[0] === "compare" && path.length === 1)) data = await evaluationWorkspace(actor, Object.fromEntries(new URL(request.url).searchParams));
    else if (path[0] === "assessments" && path.length === 2) data = await evaluationDetail(actor, path[1]);
    else if (path[0] === "evidence" && path.length === 2) data = await readEvaluationEvidence(actor, path[1]);
    else if (path[0] === "ai" && path.length === 2) data = await evaluationAiStatus(actor, path[1]);
    else throw new ApiError(404, "NOT_FOUND", "Evaluation endpoint not found.");
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
export async function POST(request: Request, { params }: Context) {
  try {
    if (request.headers.get("origin") && request.headers.get("origin") !== requestPublicOrigin(request)) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key) throw new ApiError(422, "COMMAND_KEY_REQUIRED", "Idempotency-Key is required.");
    const { holdingSlug, path = [] } = await params, actor = await requireWorkContext(holdingSlug), input = await request.json();
    let data;
    if (path[0] === "criteria" && path.length === 1) data = await saveEvaluationDefinition(actor, input, key);
    else if (path[0] === "market-import" && path.length === 1) data = await importEvaluationMarket(actor, input, key);
    else if (path[0] === "assessments" && path.length === 1) data = await createEvaluationAssessment(actor, input, key);
    else if (path[0] === "assessments" && path.length === 3 && path[2] === "save") data = await saveEvaluationAssessment(actor, path[1], input, key);
    else if (path[0] === "assessments" && path.length === 3 && path[2] === "transition") data = await transitionEvaluationAssessment(actor, path[1], input, key);
    else if (path[0] === "assessments" && path.length === 3 && path[2] === "evidence") data = await addEvaluationEvidence(actor, path[1], input, key);
    else if (path[0] === "assessments" && path.length === 3 && path[2] === "compensation") data = await createEvaluationCompensationDraft(actor, path[1], input, key);
    else if (path[0] === "basis" && path.length === 2) data = await confirmEvaluationPayBasis(actor, path[1], input, key);
    else if (path[0] === "compensation" && path.length === 3 && path[2] === "approve") data = await approveEvaluationCompensation(actor, path[1], input, key);
    else if (path[0] === "compensation" && path.length === 3 && path[2] === "cancel") data = await cancelEvaluationCompensation(actor, path[1], input, key);
    else if (path[0] === "ai" && path.length === 1) data = await queueEvaluationAi(actor, await runtimeEnv(), input, key);
    else throw new ApiError(404, "NOT_FOUND", "Evaluation command not found.");
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
