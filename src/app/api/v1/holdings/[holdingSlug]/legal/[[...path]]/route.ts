import { legalReferences, addLegalReference } from "@/server/legal/references";
import { legalKnowledge } from "@/server/legal/knowledge";
import { preserveAttachment } from "@/server/legal/sources";
import { governanceView, governanceCommand } from "@/server/legal/governance";
import { legalReports } from "@/server/legal/reports";
import {
  legalApprovalView,
  legalApprovalCommand,
} from "@/server/legal/approval";
import { boundedBody } from "@/server/legal/body";
import { featureDetail, featureCommand } from "@/server/legal/features";
import { retentionView, retentionCommand } from "@/server/legal/retention";
import { queueInspection, processingStatus } from "@/server/legal/processing";
import { trustView, trustCommand } from "@/server/legal/trust";
import {
  startRevision,
  ruleImpact,
  prepareCounselPurchase,
  importPreview,
  importCommit,
  exportRegister,
  savedViews,
  saveView,
  applyTemplate,
  convertRequest,
  evidenceBundle,
  holdPreview,
  applyAiDraft,
} from "@/server/legal/operations";
import type { PostgresDatabase } from "@/server/runtime/postgres";
import { queueLegalAi, legalAiHistory } from "@/server/legal/jobs";
import {
  externalSubmission,
  documentExport,
  uploadDocument,
} from "@/server/legal/documents";
import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { legalData, detail, legalCommand } from "@/server/legal/service";
import { sendEnvelope, signingAction } from "@/server/legal/signing";
import { errorResponse, ApiError } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(request: Request, ctx: Context) {
  try {
    const { holdingSlug, path = [] } = await ctx.params,
      a = await requireWorkContext(holdingSlug),
      { write, files } = await holdingResources(a.holdingId);
    if (path[0] === "references" && path.length === 2)
      return Response.json(
        { data: await legalReferences(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "knowledge" && path.length === 1)
      return Response.json(
        {
          data: await legalKnowledge(
            write,
            a,
            new URL(request.url).searchParams.get("q"),
          ),
        },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "rule-impact" && path.length === 2)
      return Response.json(
        { data: await ruleImpact(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "governance" && path.length <= 2)
      return Response.json(
        { data: await governanceView(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 1 && path[0] === "reports")
      return Response.json(
        { data: await legalReports(write, a) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "approval")
      return Response.json(
        { data: await legalApprovalView(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "features")
      return Response.json(
        {
          data: await featureDetail(
            write,
            a,
            path[1],
            Object.fromEntries(new URL(request.url).searchParams),
          ),
        },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "retention" && path.length <= 2)
      return Response.json(
        { data: await retentionView(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "processing" && path.length === 2)
      return Response.json(
        { data: await processingStatus(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "trust" && path.length === 1)
      return Response.json(
        { data: await trustView(write, a) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "submissions") {
      const { row, bytes } = await externalSubmission(write, files, a, path[1]);
      return new Response(new Uint8Array(bytes), {
        headers: {
          "Content-Type": row.media_type,
          "Content-Disposition": `attachment; filename="submission"; filename*=UTF-8''${encodeURIComponent(row.name)}`,
          "Cache-Control": "private, no-store",
          "X-Content-Type-Options": "nosniff",
        },
      });
    }
    if (path.length === 1 && path[0] === "views")
      return Response.json(
        { data: await savedViews(write, a) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (
      path.length === 2 &&
      ["export", "evidence", "hold-preview"].includes(path[0])
    ) {
      const data =
        path[0] === "export"
          ? await exportRegister(write, a, path[1])
          : path[0] === "evidence"
            ? await evidenceBundle(write, a, path[1])
            : await holdPreview(write, a, path[1]);
      return Response.json(
        { data },
        {
          headers: {
            "Cache-Control": "private, no-store",
            ...(path[0] !== "hold-preview"
              ? {
                  "Content-Disposition":
                    'attachment; filename="legal-export.json"',
                }
              : {}),
          },
        },
      );
    }
    if (path.length === 2 && path[0] === "ai")
      return Response.json(
        { data: await legalAiHistory(write, a, path[1]) },
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "documents")
      return documentExport(
        write,
        files,
        a,
        path[1],
        new URL(request.url).searchParams.get("format"),
      );
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Resource not found.");
    return Response.json(
      {
        data: path.length
          ? await detail(write, a, path[0])
          : await legalData(
              write,
              a,
              Object.fromEntries(new URL(request.url).searchParams),
            ),
      },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, ctx: Context) {
  try {
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const { holdingSlug, path = [] } = await ctx.params;
    const a = await requireWorkContext(holdingSlug),
      { write, files } = await holdingResources(a.holdingId);
    if (path.length === 2 && path[0] === "upload") {
      const body = await boundedBody(request, 10 * 1024 * 1024);
      return Response.json({
        data: await uploadDocument(
          write,
          files,
          a,
          path[1],
          Number(request.headers.get("X-Record-Version")),
          decodeURIComponent(request.headers.get("X-File-Name") ?? "Document"),
          request.headers.get("Content-Type") ?? "",
          new Uint8Array(body),
          key,
        ),
      });
    }
    if (
      path.length === 3 &&
      path[0] === "documents" &&
      (path[2] === "ocr" || path[2] === "verify")
    )
      return Response.json({
        data: await queueInspection(write, a, path[1], path[2], key),
      });
    if (path.length)
      throw new ApiError(404, "NOT_FOUND", "Unknown Legal action.");
    const text = new TextDecoder().decode(await boundedBody(request, 500000));
    if (text.length > 500000)
      throw new ApiError(413, "BODY_TOO_LARGE", "Use a smaller request.");
    let raw;
    try {
      raw = JSON.parse(text);
    } catch {
      throw new ApiError(422, "INVALID_JSON", "Invalid JSON.");
    }
    const data =
      raw?.action === "reference.add"
        ? await addLegalReference(write, a, raw, key)
        : raw?.action === "revision.create"
          ? await startRevision(write as PostgresDatabase, a, raw, key)
          : raw?.action === "source.attach"
            ? await preserveAttachment(
                write as PostgresDatabase,
                files,
                a,
                raw,
                key,
              )
            : /^(form|intake|authority)\./.test(raw?.action ?? "")
              ? await governanceCommand(write, a, raw, key)
              : raw?.action?.startsWith("approval.")
                ? await legalApprovalCommand(write, a, raw, key)
                : raw?.action?.startsWith("trust.")
                  ? await trustCommand(write, a, raw, key)
                  : raw?.action?.startsWith("retention.")
                    ? await retentionCommand(write, a, raw, key)
                    : raw?.action?.startsWith("feature.")
                      ? await featureCommand(write, a, raw, key)
                      : raw?.action === "finance.prepare"
                        ? await prepareCounselPurchase(
                            write as PostgresDatabase,
                            a,
                            raw,
                            key,
                          )
                        : raw?.action === "import.preview"
                          ? await importPreview(a, raw.rows)
                          : raw?.action === "import.commit"
                            ? await importCommit(
                                write as PostgresDatabase,
                                a,
                                raw,
                                key,
                              )
                            : raw?.action === "view.save"
                              ? await saveView(write, a, raw, key)
                              : raw?.action === "template.apply"
                                ? await applyTemplate(
                                    write as PostgresDatabase,
                                    a,
                                    raw,
                                    key,
                                  )
                                : raw?.action === "request.convert"
                                  ? await convertRequest(
                                      write as PostgresDatabase,
                                      a,
                                      raw,
                                      key,
                                    )
                                  : raw?.action === "ai.apply"
                                    ? await applyAiDraft(write, a, raw, key)
                                    : raw?.action === "ai.generate"
                                      ? await queueLegalAi(write, a, raw, key)
                                      : raw?.action === "signing.send"
                                        ? await sendEnvelope(write, a, raw, key)
                                        : raw?.action?.startsWith("signing.")
                                          ? await signingAction(
                                              write,
                                              a,
                                              raw,
                                              key,
                                            )
                                          : await legalCommand(
                                              write,
                                              a,
                                              raw,
                                              key,
                                            );
    return Response.json(
      { data },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
