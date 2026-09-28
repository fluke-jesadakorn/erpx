import "server-only";
import { createHash } from "node:crypto";
import { requireManageErpxContext, requireWorkContext } from "./authorization";
import { runtimeEnv, holdingResources } from "./runtime";
import type { PostgresDatabase } from "./runtime/postgres";
import { requireCompanyAuthority } from "./staff-policy";
import { ApiError, errorResponse } from "./http";
import { betaBody } from "./beta-http";
import { requestPublicOrigin } from "./public-origin";
import {
  billingCommand,
  billingInput,
  billingView,
  type BillingActor,
} from "./billing";
import { billingStripe } from "./billing-stripe";
import { attachmentUploadsFrozen, attachmentsV2ReadOnly } from "./attachments/feature";
import { detectedAttachmentMediaType } from "./attachments/preview";
import { attachmentScannerReadiness } from "./attachments/scanner";
import { billingEvidenceStore } from "./billing-attachments";
export function billingHandler(platform: boolean) {
  return async (
    request: Request,
    context: { params: Promise<{ holdingSlug?: string; path?: string[] }> },
  ) => {
    try {
      const params = await context.params,
        env = await runtimeEnv(),
        db = env.CONTROL_DB as PostgresDatabase,
        url = new URL(request.url),
        path = params.path ?? [];
      let holdingId: string | undefined,
        slug = params.holdingSlug,
        actor: BillingActor;
      if (platform) {
        const { platform: a } = await requireManageErpxContext();
        if (
          !["platform_admin", "platform_auditor"].includes(a.role) ||
          (request.method !== "GET" && a.role !== "platform_admin")
        )
          throw new ApiError(
            403,
            "FORBIDDEN",
            "Platform billing permission required.",
          );
        actor = { userId: a.userId, email: a.email, platform: true };
        holdingId = url.searchParams.get("holdingId") ?? undefined;
      } else {
        const a = await requireWorkContext(slug!);
        if (a.role !== "holding_owner")
          throw new ApiError(
            403,
            "FORBIDDEN",
            "Only the Holding owner can access billing.",
          );
        await requireCompanyAuthority(
          (await holdingResources(a.holdingId)).write,
          a,
          ["holding_owner"],
        );
        holdingId = a.holdingId;
        actor = {
          userId: a.userId,
          email: a.email,
          platform: false,
          holdingId,
        };
      }
      if (platform && !holdingId && request.method === "GET" && !path.length)
        return Response.json({
          data: {
            accounts: (
              await db.pool.query(
                "SELECT h.holding_id,h.slug,h.display_name,a.version,a.payment_mode FROM holding_bindings h LEFT JOIN billing_accounts a ON a.holding_id=h.holding_id WHERE h.holding_id <> 'h003' ORDER BY h.display_name",
              )
            ).rows,
          },
        });
      if (platform && holdingId === "h003")
        throw new ApiError(403, "ERPX_NOT_CUSTOMER", "ERPX is not a customer billing account.");
      if (!holdingId)
        throw new ApiError(422, "HOLDING_REQUIRED", "Select a Holding.");
      const holding = (
        await db.pool.query(
          "SELECT slug FROM holding_bindings WHERE holding_id=$1",
          [holdingId],
        )
      ).rows[0];
      if (!holding) throw new ApiError(404, "NOT_FOUND", "Holding not found.");
      slug = holding.slug;
      if (path[0] === "evidence") {
        if (!platform)
          throw new ApiError(
            403,
            "FORBIDDEN",
            "Evidence is private to Platform billing.",
          );
        const store = billingEvidenceStore(env);
        if (request.method === "GET" && path[1]) {
          const row = (
            await db.pool.query(
              "SELECT * FROM billing_evidence WHERE id=$1 AND holding_id=$2",
              [path[1], holdingId],
            )
          ).rows[0];
          if (!row) throw new ApiError(404, "NOT_FOUND", "Evidence not found.");
          if(row.source_state!=="available"){
            await db.pool.query("INSERT INTO billing_evidence_access_events(id,evidence_id,holding_id,actor,outcome,reason,created_at) VALUES($1,$2,$3,$4,'denied',$5,$6)",[`billing-access-${crypto.randomUUID()}`,row.id,holdingId,actor.userId,"EVIDENCE_NOT_AVAILABLE",Date.now()]);
            throw new ApiError(423,"EVIDENCE_NOT_AVAILABLE","Evidence is quarantined, rejected, or awaiting a clean scan.");
          }
          const file = await store.get(row.object_key??`${holdingId}/${row.id}`);
          if (!file){
            await db.pool.query("INSERT INTO billing_evidence_access_events(id,evidence_id,holding_id,actor,outcome,reason,created_at) VALUES($1,$2,$3,$4,'denied',$5,$6)",[`billing-access-${crypto.randomUUID()}`,row.id,holdingId,actor.userId,"FILE_UNAVAILABLE",Date.now()]);
            throw new ApiError(404, "NOT_FOUND", "Evidence file missing.");
          }
          await db.pool.query("INSERT INTO billing_evidence_access_events(id,evidence_id,holding_id,actor,outcome,created_at) VALUES($1,$2,$3,$4,'allowed',$5)",[`billing-access-${crypto.randomUUID()}`,row.id,holdingId,actor.userId,Date.now()]);
          return new Response(file.body, {
            headers: {
              "Content-Type": row.mime,
              "Content-Disposition": `attachment; filename*=UTF-8''${encodeURIComponent(row.filename)}`,
              "Cache-Control": "private, no-store",
              "X-Content-Type-Options": "nosniff",
            },
          });
        }
        if (request.method === "POST" && path.length === 1) {
          if(attachmentsV2ReadOnly())throw new ApiError(503,"ATTACHMENTS_READ_ONLY","Billing evidence intake is disabled during attachment rollback. Existing clean evidence remains readable.");
          if(attachmentUploadsFrozen())throw new ApiError(503,"ATTACHMENT_UPLOAD_FREEZE","Billing evidence intake is briefly paused for final storage reconciliation.");
          if (
            request.headers.get("origin") &&
            request.headers.get("origin") !== requestPublicOrigin(request)
          )
            throw new ApiError(
              403,
              "ORIGIN_FORBIDDEN",
              "Use the application origin.",
            );
          const key = request.headers.get("Idempotency-Key") ?? "";
          if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
            throw new ApiError(
              422,
              "KEY_REQUIRED",
              "Upload command key required.",
            );
          const mime = request.headers.get("content-type") ?? "",
            filename = decodeURIComponent(
              request.headers.get("x-filename") ?? "evidence",
            );
          if (
            !["application/pdf", "image/png", "image/jpeg"].includes(mime) ||
            filename.length > 200
          )
            throw new ApiError(
              422,
              "INVALID_FILE",
              "Use a PDF, PNG or JPEG up to 10 MiB.",
            );
          const chunks: Uint8Array[] = [];
          let size = 0;
          const reader = request.body?.getReader();
          if (reader)
            while (true) {
              const r = await reader.read();
              if (r.done) break;
              size += r.value.length;
              if (size > 10 * 1024 * 1024) {
                await reader.cancel();
                throw new ApiError(413, "FILE_TOO_LARGE", "Maximum 10 MiB.");
              }
              chunks.push(r.value);
            }
          const bytes = Buffer.concat(chunks);
          const detected=detectedAttachmentMediaType(bytes,mime);
          if (detected!==mime)
            throw new ApiError(
              422,
              "INVALID_FILE",
              "The file contents do not match its type.",
            );
          const id = createHash("sha256")
              .update(`${holdingId}:${actor.userId}:${key}`)
              .digest("hex"),
            sha = createHash("sha256").update(bytes).digest("hex");
          const old = (
            await db.pool.query("SELECT * FROM billing_evidence WHERE id=$1", [
              id,
            ])
          ).rows[0];
          if (
            old &&
            (old.sha256 !== sha ||
              old.filename !== filename ||
              old.mime !== mime)
          )
            throw new ApiError(409, "KEY_CONFLICT", "Upload key already used.");
          if(old){
            const state=String(old.source_state),pending=["legacy_unscanned","quarantined","scanning","scan_error"].includes(state);
            return Response.json({data:{id:old.id,state}},{status:pending?202:200});
          }
          if (
            !(
              await db.pool.query(
                "SELECT 1 FROM billing_accounts WHERE holding_id=$1",
                [holdingId],
              )
            ).rowCount
          )
            throw new ApiError(
              409,
              "ACCOUNT_REQUIRED",
              "Enroll this Holding before adding evidence.",
            );
          const readiness=await attachmentScannerReadiness({...process.env,ERPX_ATTACHMENTS_V2:"true"});
          if(!readiness.ready||!readiness.enabled)throw new ApiError(503,"ATTACHMENT_SCANNER_UNAVAILABLE","Billing evidence intake requires a healthy malware scanner and current signatures.");
          const objectKey=`${holdingId}/quarantine/${id}/original`;
          const saved = await store.put(objectKey, bytes, {
            onlyIf: { etagDoesNotMatch: "*" },
            httpMetadata: { contentType: mime,cacheControl:"private, no-store" },customMetadata:{sha256:sha,state:"quarantined"},
          });
          if (!saved) {
            const existing = await store.head(objectKey);
            if (existing?.customMetadata.sha256 !== sha)
              throw new ApiError(
                409,
                "KEY_CONFLICT",
                "Upload key already used.",
              );
          }
          const now=Date.now(),client=await db.pool.connect();try{await client.query("BEGIN");await client.query(
            `INSERT INTO billing_evidence(id,holding_id,filename,mime,size,sha256,actor,created_at,object_key,detected_mime,source_state)
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING`,
            [id,holdingId,filename,mime,size,sha,actor.userId,now,objectKey,detected,"quarantined"],
          );await client.query(`INSERT INTO billing_evidence_storage_ledger(id,evidence_id,holding_id,object_key,bytes,state,created_at,updated_at)
            SELECT $1,id,holding_id,object_key,size,'stored',$2,$2 FROM billing_evidence WHERE id=$3 ON CONFLICT(object_key) DO NOTHING`,[`billing-storage-${id}`,now,id]);await client.query("INSERT INTO billing_evidence_scan_attempts(id,evidence_id,attempt,state,created_at) VALUES($1,$2,1,'pending',$3) ON CONFLICT(evidence_id,attempt) DO NOTHING",[`billing-scan-${id}`,id,now]);await client.query("INSERT INTO billing_evidence_events(id,evidence_id,holding_id,actor,action,details_json,created_at) VALUES($1,$2,$3,$4,'file.quarantined',$5,$6) ON CONFLICT(id) DO NOTHING",[`billing-event-${id}`,id,holdingId,actor.userId,JSON.stringify({sha256:sha,bytes:size,detectedMime:detected}),now]);await client.query("COMMIT");}catch(error){await client.query("ROLLBACK");throw error;}finally{client.release();}
          return Response.json({ data: { id,state:"quarantined" } },{status:202});
        }
      }
      if (path.length)
        throw new ApiError(404, "NOT_FOUND", "Billing route not found.");
      if (request.method === "GET")
        return Response.json(
          { data: await billingView(db, holdingId) },
          { headers: { "Cache-Control": "private, no-store" } },
        );
      const input = billingInput.parse(await betaBody(request));
      const data = await billingCommand(
        db,
        actor,
        holdingId,
        input,
        request.headers.get("Idempotency-Key") ?? "",
        billingStripe(env),
        new URL(`/holdings/${slug}/settings?section=usage`, env.BETTER_AUTH_URL)
          .href,
      );
      return Response.json({ data });
    } catch (error) {
      return errorResponse(error);
    }
  };
}
