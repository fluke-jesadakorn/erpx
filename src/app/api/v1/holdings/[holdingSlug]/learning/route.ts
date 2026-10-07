import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import {
  changeLearningAssignment,
  changeLearningCourse,
  createLearningAssignment,
  createLearningCourse,
  learningAssignmentDetail,
  learningWorkspace,
} from "@/server/modules/learning/public/learning";
import { certificateRevocationSchema,revokeLearningCertificate } from "@/server/modules/learning/public/learning-eligibility";
type C = { params: Promise<{ holdingSlug: string }> };
const headers = { "Cache-Control": "private, no-store" };
export async function GET(r: Request, c: C) {
  try {
    const a = await requireWorkContext((await c.params).holdingSlug),
      id = new URL(r.url).searchParams.get("id");
    return Response.json(
      {
        data: id
          ? await learningAssignmentDetail(a, id)
          : await learningWorkspace(a),
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
      input = await r.json(),
      data =
        input.entity==="certificate"?await revokeLearningCertificate(a,input.id,certificateRevocationSchema.parse({expectedAssignmentVersion:input.expectedAssignmentVersion,reason:input.reason}),key):input.entity === "course"
          ? typeof input.id === "string"
            ? await changeLearningCourse(a, input.id, input, key)
            : await createLearningCourse(a, input, key)
          : typeof input.id === "string"
            ? await changeLearningAssignment(a, input.id, input, key)
            : await createLearningAssignment(a, input, key);
    return Response.json({ data }, { headers });
  } catch (e) {
    return errorResponse(e);
  }
}
