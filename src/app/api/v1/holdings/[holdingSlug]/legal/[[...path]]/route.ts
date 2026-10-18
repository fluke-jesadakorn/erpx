import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { legalReferences, addLegalReference } from "@/server/modules/legal/public/legal/references";
import { legalKnowledge, lawCitation } from "@/server/modules/legal/public/legal/knowledge";
import { legalPeople } from "@/server/modules/legal/public/legal/assignees";
import { executedCopies, reviewExecutedCopy } from "@/server/modules/legal/public/legal/executed-copies";
import { lawCorpusView, lawCorpusCommand } from "@/server/modules/legal/public/legal/law-corpus";
import { legalWorkspaceView } from "@/server/modules/legal/public/legal/workspace";
import { legalRuntimeHealth } from "@/server/modules/legal/public/legal/runtime-gate";
import { releaseAcceptance, submitReleaseAcceptance, reviewReleaseAcceptance, revokeReleaseAcceptance } from "@/server/modules/platform_administration/public/release-acceptance";
import { legalProductionScope } from "@/data/production-scope";
import { preserveAttachment } from "@/server/modules/legal/public/legal/sources";
import { governanceView, governanceCommand } from "@/server/modules/legal/public/legal/governance";
import { legalReports } from "@/server/modules/legal/public/legal/reports";
import {
  legalApprovalView,
  legalApprovalCommand,
} from "@/server/modules/legal/public/legal/approval";
import { boundedBody } from "@/server/modules/legal/public/legal/body";
import { featureDetail, featureCommand } from "@/server/modules/legal/public/legal/features";
import { retentionView, retentionCommand } from "@/server/modules/legal/public/legal/retention";
import { queueInspection, processingStatus } from "@/server/modules/legal/public/legal/processing";
import { trustView, trustCommand } from "@/server/modules/legal/public/legal/trust";
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
} from "@/server/modules/legal/public/legal/operations";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { queueLegalAi, legalAiHistory } from "@/server/modules/legal/public/legal/jobs";
import {
  externalSubmission,
  documentExport,
  uploadDocument,
  importExecutedCopy,
} from "@/server/modules/legal/public/legal/documents";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { legalData, legalWorkspaceDetail as detail, legalCommand } from "@/server/modules/legal/public/legal/service";
import { sendEnvelope, signingAction } from "@/server/modules/legal/public/legal/signing";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export async function GET(request: Request, ctx: Context) {
  try {
    const { holdingSlug, path = [] } = await ctx.params,
      a = await requireWorkContext(holdingSlug),
      { write, files } = await holdingResources(a.holdingId);
    const query = new URL(request.url).searchParams;
    if (path.length === 1 && path[0] === "people")
      return Response.json(canonicalDestinationLinks({ data: await legalPeople(write, a, Object.fromEntries(query)) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path.length === 1 && path[0] === "workspace")
      return Response.json(canonicalDestinationLinks({ data: await legalWorkspaceView(write, a) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path.length === 1 && path[0] === "runtime-health")
      return Response.json(canonicalDestinationLinks({ data: await legalRuntimeHealth(write, a) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path.length === 1 && path[0] === "release-acceptance")
      return Response.json(canonicalDestinationLinks({ data: await releaseAcceptance(write, a, undefined, legalProductionScope.version) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path.length === 1 && path[0] === "laws")
      return Response.json(canonicalDestinationLinks({ data: await lawCorpusView(write, a, query.get("ruleId") ?? undefined) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path.length === 2 && path[0] === "law-citations")
      return Response.json(canonicalDestinationLinks({ data: await lawCitation(write, a, path[1], query.get("asOf") ?? undefined) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path.length === 3 && path[0] === "items" && path[2] === "executed-copies")
      return Response.json(canonicalDestinationLinks({ data: await executedCopies(write, a, path[1]) }), { headers: { "Cache-Control": "private, no-store" } });
    if (path[0] === "references" && path.length === 2)
      return Response.json(
        canonicalDestinationLinks({ data: await legalReferences(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "knowledge" && path.length === 1)
      return Response.json(
        canonicalDestinationLinks({
          data: await legalKnowledge(
            write,
            a,
            new URL(request.url).searchParams.get("q"),
            query.get("asOf") ?? undefined,
          ),
        }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "rule-impact" && path.length === 2)
      return Response.json(
        canonicalDestinationLinks({ data: await ruleImpact(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "governance" && path.length <= 2)
      return Response.json(
        canonicalDestinationLinks({ data: await governanceView(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 1 && path[0] === "reports")
      return Response.json(
        canonicalDestinationLinks({ data: await legalReports(write, a) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "approval")
      return Response.json(
        canonicalDestinationLinks({ data: await legalApprovalView(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "features")
      return Response.json(
        canonicalDestinationLinks({
          data: await featureDetail(
            write,
            a,
            path[1],
            Object.fromEntries(new URL(request.url).searchParams),
          ),
        }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "retention" && path.length <= 2)
      return Response.json(
        canonicalDestinationLinks({ data: await retentionView(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "processing" && path.length === 2)
      return Response.json(
        canonicalDestinationLinks({ data: await processingStatus(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path[0] === "trust" && path.length === 1)
      return Response.json(
        canonicalDestinationLinks({ data: await trustView(write, a) }),
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
        canonicalDestinationLinks({ data: await savedViews(write, a) }),
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
        canonicalDestinationLinks({ data }),
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
        canonicalDestinationLinks({ data: await legalAiHistory(write, a, path[1]) }),
        { headers: { "Cache-Control": "private, no-store" } },
      );
    if (path.length === 2 && path[0] === "documents")
      return await documentExport(
        write,
        files,
        a,
        path[1],
        new URL(request.url).searchParams.get("format"),
      );
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Resource not found.");
    return Response.json(
      canonicalDestinationLinks({
        data: path.length
          ? await detail(write, a, path[0])
          : await legalData(
              write,
              a,
              Object.fromEntries(new URL(request.url).searchParams),
            ),
      }),
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
    if (path.length === 3 && path[0] === "items" && path[2] === "executed-copies") {
      if (!request.headers.get("Content-Type")?.startsWith("multipart/form-data;"))
        throw new ApiError(422, "MULTIPART_REQUIRED", "Choose a signed PDF or scanned image.");
      const bytes = await boundedBody(request, 10 * 1024 * 1024 + 65536);
      const form = await new Response(bytes, { headers: { "Content-Type": request.headers.get("Content-Type")! } }).formData();
      const file = form.get("file"), metadata = form.get("metadata");
      if (!(file instanceof File) || typeof metadata !== "string" || metadata.length > 30000)
        throw new ApiError(422, "EXECUTED_METADATA_REQUIRED", "Supply the signed file and its signing details.");
      let parsed: unknown;
      try { parsed = JSON.parse(metadata); } catch { throw new ApiError(422, "INVALID_JSON", "Signing details are invalid."); }
      return Response.json(canonicalDestinationLinks({ data: await importExecutedCopy(write, files, a, path[1], Number(form.get("expectedVersion")), parsed, file.name, file.type, new Uint8Array(await file.arrayBuffer()), key) }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if (path.length === 2 && path[0] === "upload") {
      const body = await boundedBody(request, 10 * 1024 * 1024);
      return Response.json(canonicalDestinationLinks({
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
      }));
    }
    if (
      path.length === 3 &&
      path[0] === "documents" &&
      (path[2] === "ocr" || path[2] === "verify")
    )
      return Response.json(canonicalDestinationLinks({
        data: await queueInspection(write, a, path[1], path[2], key),
      }));
    if (path.length && path[0] !== "release-acceptance")
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
    if (path[0] === "release-acceptance") {
      if (path.length === 3 && ["review", "revoke"].includes(path[2])) {
        const target = await releaseAcceptance(write, a, path[1], legalProductionScope.version);
        if (target.release?.scopeVersion !== legalProductionScope.version)
          throw new ApiError(404, "RELEASE_NOT_FOUND", "Legal release not found.");
      }
      const data = path.length === 1
        ? await submitReleaseAcceptance(write, a, { ...raw, scopeVersion: legalProductionScope.version }, key)
        : path.length === 3 && path[2] === "review"
          ? await reviewReleaseAcceptance(write, a, path[1], raw, key)
          : path.length === 3 && path[2] === "revoke"
            ? await revokeReleaseAcceptance(write, a, path[1], raw, key)
            : (() => { throw new ApiError(404, "NOT_FOUND", "Unknown release action."); })();
      return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
    }
    const data =
      raw?.action === "executed.review"
        ? await reviewExecutedCopy(write, a, raw, key)
        : /^(law\.|knowledge\.document\.)/.test(raw?.action ?? "")
          ? await lawCorpusCommand(write, a, raw, key)
      : raw?.action === "reference.add"
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
      canonicalDestinationLinks({ data }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
