import { requestPublicOrigin } from "@/server/public-origin";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { saveDocumentPage } from "@/server/document-derivatives";
import { ApiError, errorResponse } from "@/server/http";
export async function POST(
  request: Request,
  context: { params: Promise<{ holdingSlug: string; id: string }> },
) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const length = Number(request.headers.get("content-length"));
    if (!length || length > 2 * 1024 * 1024)
      throw new ApiError(
        422,
        "PAGE_SIZE",
        "A rendered page must be at most 2 MB.",
      );
    const { holdingSlug, id } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write, files } = await holdingResources(actor.holdingId),
      url = new URL(request.url),
      key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const bytes = await request.arrayBuffer();
    if (bytes.byteLength !== length)
      throw new ApiError(422, "PAGE_SIZE", "The uploaded page size changed.");
    return Response.json({
      data: await saveDocumentPage(
        write,
        files,
        actor,
        id,
        {
          page: Number(url.searchParams.get("page")),
          pageCount: Number(url.searchParams.get("total")),
          originalChecksum: url.searchParams.get("original") ?? "",
          bytes,
        },
        key,
      ),
    });
  } catch (error) {
    return errorResponse(error);
  }
}
