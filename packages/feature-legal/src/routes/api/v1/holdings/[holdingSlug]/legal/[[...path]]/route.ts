import {createHash} from "node:crypto";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { legalReferences, addLegalReference } from "@erpx/server-legal/public/legal/references";
import { legalKnowledge, lawCitation } from "@erpx/server-legal/public/legal/knowledge";
import { legalPeople } from "@erpx/server-legal/public/legal/assignees";
import { executedCopies, reviewExecutedCopy } from "@erpx/server-legal/public/legal/executed-copies";
import { lawCorpusView, lawCorpusCommand } from "@erpx/server-legal/public/legal/law-corpus";
import { legalWorkspaceView } from "@erpx/server-legal/public/legal/workspace";
import { legalRuntimeHealth } from "@erpx/server-legal/public/legal/runtime-gate";
import { releaseAcceptance, submitReleaseAcceptance, reviewReleaseAcceptance, revokeReleaseAcceptance } from "@erpx/server-platform_administration/public/release-acceptance";
import { legalProductionScope } from "@erpx/catalogue/production-scope";
import { preserveAttachment } from "@erpx/server-legal/public/legal/sources";
import { governanceView, governanceCommand } from "@erpx/server-legal/public/legal/governance";
import { legalReports } from "@erpx/server-legal/public/legal/reports";
import {
  legalApprovalView,
  legalApprovalCommand,
} from "@erpx/server-legal/public/legal/approval";
import { boundedBody } from "@erpx/server-legal/public/legal/body";
import { featureDetail, featureCommand } from "@erpx/server-legal/public/legal/features";
import { retentionView, retentionCommand } from "@erpx/server-legal/public/legal/retention";
import { queueInspection, processingStatus } from "@erpx/server-legal/public/legal/processing";
import { trustView, trustCommand } from "@erpx/server-legal/public/legal/trust";
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
} from "@erpx/server-legal/public/legal/operations";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { queueLegalAi, legalAiHistory } from "@erpx/server-legal/public/legal/jobs";
import {
  externalSubmission,
  documentExport,
  uploadDocument,
  importExecutedCopy,
} from "@erpx/server-legal/public/legal/documents";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { legalData, legalRecordData, legalWorkspaceDetail as detail, legalCommand } from "@erpx/server-legal/public/legal/service";
import { sendEnvelope, signingAction } from "@erpx/server-legal/public/legal/signing";
import { errorResponse, ApiError } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { legalReadScope } from "@erpx/server-legal/public/purchases";
import { nativeLegalAvailable, nativeLegalCommand, nativeLegalCommandSelected, nativeLegalDetail, nativeLegalExport, nativeLegalFailure, nativeLegalRegister } from "@erpx/feature-legal/lib/native-legal";
import { createAttachmentSet, reserveAttachmentFile, completeAttachmentUpload, scanAttachmentNow } from "@erpx/server-files/public/attachments/service";
export const dynamic = "force-dynamic";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
function nativeFailure(error: unknown): never { const failure = nativeLegalFailure(error); if (failure) throw new ApiError(failure.status, failure.code, failure.message); throw error; }
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
    if (path.length === 2 && path[0] === "documents") {
      const format = new URL(request.url).searchParams.get("format");
      if (nativeLegalAvailable()) {
        const native = await nativeLegalExport(a, holdingSlug, path[1], format, request.headers).catch(nativeFailure);
        if (native && !native.requires_provider) {
          const headers = {
            "Content-Type": native.media_type,
            "Content-Disposition": `attachment; filename="document"; filename*=UTF-8''${encodeURIComponent(native.name)}`,
            "Cache-Control": "private, no-store",
            "X-Content-Type-Options": "nosniff",
          };
          if (native.object_key) {
            const object = await files.get(native.object_key);
            if (!object)
              throw new ApiError(404, "LEGAL_DOCUMENT_UNAVAILABLE", "The original document is unavailable.");
            const bytes = await object.arrayBuffer();
            if (createHash("sha256").update(new Uint8Array(bytes)).digest("hex") !== native.checksum)
              throw new ApiError(502, "LEGAL_DOCUMENT_INTEGRITY", "The original document failed its integrity check.");
            return new Response(bytes, { headers });
          }
          return new Response(native.body, { headers });
        }
      }
      return await documentExport(
        write,
        files,
        a,
        path[1],
        format,
      );
    }
    if (path.length > 1)
      throw new ApiError(404, "NOT_FOUND", "Resource not found.");
    if (nativeLegalAvailable() && path.length === 0) {
      const native = await nativeLegalRegister(a, holdingSlug, Object.fromEntries(query), request.headers).catch(nativeFailure);
      if (native) {
        const shell = await legalRecordData(write, a);
        const purchases = (await legalReadScope(write, a)).purchases;
        return Response.json(canonicalDestinationLinks({ data: { ...shell, items: native.records, total: native.total, page: native.page, counts: native.counts, enabled: native.enabled, purchases } }), { headers: { "Cache-Control": "private, no-store" } });
      }
    }
    if (nativeLegalAvailable() && path.length === 1) {
      const native = await nativeLegalDetail(a, holdingSlug, path[0], request.headers).catch(nativeFailure);
      if (native) {
        const purchases = (await legalReadScope(write, a)).purchases;
        return Response.json(canonicalDestinationLinks({ data: { ...native, purchases } }), { headers: { "Cache-Control": "private, no-store" } });
      }
    }
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
      const expectedVersion = Number(form.get("expectedVersion"));
      const fileBytes = new Uint8Array(await file.arrayBuffer());
      if (nativeLegalAvailable()) {
        const set = await createAttachmentSet(write, a, {
          targetType: "legal_item",
          targetId: path[1],
          componentKind: "record",
          componentRef: `executed:${key}`,
          purposeKey: "legal_document",
          expectedTargetVersion: expectedVersion,
        }, `${key}:set`, { compatibilityAdapter: true });
        const reserved = await reserveAttachmentFile(write, a, set.id, {
          filename: file.name,
          mediaType: file.type,
          bytes: fileBytes.byteLength,
        }, `${key}:file`, { compatibilityAdapter: true });
        const uploaded = await completeAttachmentUpload(write, files, a, reserved.uploadIntentId, fileBytes.buffer as ArrayBuffer, { compatibilityAdapter: true });
        const sourceState = await scanAttachmentNow(write, files, uploaded.sourceId);
        if (sourceState !== "available") {
          throw new ApiError(
            sourceState === "malware_rejected" ? 422 : 503,
            sourceState === "malware_rejected" ? "LEGAL_UPLOAD_REJECTED" : "LEGAL_SCAN_UNAVAILABLE",
            sourceState === "malware_rejected" ? "The upload was rejected by malware scanning." : "The upload could not pass malware scanning.",
          );
        }
        const native = await nativeLegalCommand(a, holdingSlug, {
          action: "executed.import",
          id: path[1],
          expectedVersion,
          attachmentLinkId: uploaded.attachmentLinkId,
          metadata: parsed,
        }, key, request.headers).catch(nativeFailure);
        if (native) return Response.json(canonicalDestinationLinks({ data: native }), { headers: { "Cache-Control": "private, no-store" } });
      }
      return Response.json(canonicalDestinationLinks({ data: await importExecutedCopy(write, files, a, path[1], expectedVersion, parsed, file.name, file.type, fileBytes, key) }), { headers: { "Cache-Control": "private, no-store" } });
    }
    if (path.length === 2 && path[0] === "upload") {
      const body = await boundedBody(request, 10 * 1024 * 1024);
      const bytes = new Uint8Array(body);
      const expectedVersion = Number(request.headers.get("X-Record-Version"));
      const name = decodeURIComponent(request.headers.get("X-File-Name") ?? "Document");
      if (nativeLegalAvailable()) {
        const set = await createAttachmentSet(write, a, {
          targetType: "legal_item",
          targetId: path[1],
          componentKind: "record",
          componentRef: `document:${key}`,
          purposeKey: "legal_document",
          expectedTargetVersion: expectedVersion,
        }, `${key}:set`, { compatibilityAdapter: true });
        const reserved = await reserveAttachmentFile(write, a, set.id, {
          filename: name,
          mediaType: request.headers.get("Content-Type") ?? "",
          bytes: bytes.byteLength,
        }, `${key}:file`, { compatibilityAdapter: true });
        const uploaded = await completeAttachmentUpload(write, files, a, reserved.uploadIntentId, bytes.buffer as ArrayBuffer, { compatibilityAdapter: true });
        const sourceState = await scanAttachmentNow(write, files, uploaded.sourceId);
        if (sourceState !== "available") {
          throw new ApiError(
            sourceState === "malware_rejected" ? 422 : 503,
            sourceState === "malware_rejected" ? "LEGAL_UPLOAD_REJECTED" : "LEGAL_SCAN_UNAVAILABLE",
            sourceState === "malware_rejected" ? "The upload was rejected by malware scanning." : "The upload could not pass malware scanning.",
          );
        }
        const native = await nativeLegalCommand(a, holdingSlug, {
          action: "document.import",
          id: path[1],
          expectedVersion,
          attachmentLinkId: uploaded.attachmentLinkId,
          name,
        }, key, request.headers).catch(nativeFailure);
        if (native) return Response.json(canonicalDestinationLinks({ data: native }), { headers: { "Cache-Control": "private, no-store" } });
      }
      return Response.json(canonicalDestinationLinks({
        data: await uploadDocument(
          write,
          files,
          a,
          path[1],
          expectedVersion,
          name,
          request.headers.get("Content-Type") ?? "",
          bytes,
          key,
        ),
      }));
    }
    if (
      path.length === 3 &&
      path[0] === "documents" &&
      (path[2] === "ocr" || path[2] === "verify")
    ) {
      if (nativeLegalAvailable()) {
        const source = await write.prepare("SELECT i.id,i.version::int version FROM legal_items i JOIN legal_documents d ON d.item_id=i.id WHERE d.id=? AND i.company_id=?").bind(path[1], a.companyId).first<{ id: string; version: number }>();
        if (source) {
          const native = await nativeLegalCommand(a, holdingSlug, {
            action: "document.inspect",
            id: source.id,
            expectedVersion: Number(source.version),
            documentId: path[1],
            kind: path[2],
          }, key, request.headers).catch(nativeFailure);
          if (native) return Response.json(canonicalDestinationLinks({ data: { id: path[1], jobId: native.jobId, state: "queued" } }), { headers: { "Cache-Control": "private, no-store" } });
        }
      }
      return Response.json(canonicalDestinationLinks({
        data: await queueInspection(write, a, path[1], path[2], key),
      }));
    }
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
    if (path.length === 0 && nativeLegalAvailable() && nativeLegalCommandSelected(raw)) {
      const data = await nativeLegalCommand(a, holdingSlug, raw, key, request.headers).catch(nativeFailure);
      if (data) return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
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
