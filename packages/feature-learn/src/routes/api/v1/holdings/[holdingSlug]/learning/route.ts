import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import {
  changeLearningAssignment,
  changeLearningCourse,
  createLearningAssignment,
  learningAssignmentDetail,
  learningWorkspace,
} from "@erpx/server-learning/public/learning";
import {nativeLearningCommandRoute} from "@erpx/feature-learn/lib/native-learning-command";
import {nativeLearningAvailable,nativeLearningDetailPage,nativeLearningWorkspacePage} from "@erpx/workplace/lib/native-learning";
import { certificateRevocationSchema,revokeLearningCertificate } from "@erpx/server-learning/public/learning-eligibility";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    const data = nativeLearningAvailable()
      ? id
        ? await nativeLearningDetailPage(a, id)
        : await nativeLearningWorkspacePage(a)
      : id
        ? await learningAssignmentDetail(a, id)
        : await learningWorkspace(a);
    return Response.json(
      canonicalDestinationLinks({ data }),
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
    const native = nativeLearningAvailable();
    const result = native
      ? await nativeLearningCommandRoute(a, input, key, r.headers)
      : undefined;
    if (native && !result)
      throw new ApiError(
        503,
        "NATIVE_LEARNING_UNAVAILABLE",
        "The native Learning command is unavailable.",
      );
    if (!native && input.entity === "course" && typeof input.id !== "string")
      throw new ApiError(
        503,
        "NATIVE_LEARNING_UNAVAILABLE",
        "The native Learning course command is unavailable.",
      );
    const data =
      result ??
      (input.entity==="certificate"?await revokeLearningCertificate(a,input.id,certificateRevocationSchema.parse({expectedAssignmentVersion:input.expectedAssignmentVersion,reason:input.reason}),key):input.entity === "course"
        ? await changeLearningCourse(a, String(input.id), input, key)
        : typeof input.id === "string"
          ? await changeLearningAssignment(a, input.id, input, key)
          : await createLearningAssignment(a, input, key));
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
