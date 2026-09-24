import "server-only";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { requireManageErpxContext, requireWorkContext } from "./authorization";
import { runtimeEnv, holdingResources } from "./runtime";
import type { PostgresDatabase } from "./runtime/postgres";
import { LocalFileStore } from "./runtime/filesystem";
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
import { matchesUploadSignature } from "./upload-validation";
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
        const store = new LocalFileStore(
          join(env.ERPX_DATA_DIR, "files", "platform-billing"),
        );
        if (request.method === "GET" && path[1]) {
          const row = (
            await db.pool.query(
              "SELECT * FROM billing_evidence WHERE id=$1 AND holding_id=$2",
              [path[1], holdingId],
            )
          ).rows[0];
          if (!row) throw new ApiError(404, "NOT_FOUND", "Evidence not found.");
          const file = await store.get(`${holdingId}/${row.id}`);
          if (!file)
            throw new ApiError(404, "NOT_FOUND", "Evidence file missing.");
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
              "Use a PDF, PNG or JPEG up to 5 MB.",
            );
          const chunks: Uint8Array[] = [];
          let size = 0;
          const reader = request.body?.getReader();
          if (reader)
            while (true) {
              const r = await reader.read();
              if (r.done) break;
              size += r.value.length;
              if (size > 5 * 1024 * 1024) {
                await reader.cancel();
                throw new ApiError(413, "FILE_TOO_LARGE", "Maximum 5 MB.");
              }
              chunks.push(r.value);
            }
          const bytes = Buffer.concat(chunks);
          if (!matchesUploadSignature(mime, bytes))
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
          const saved = await store.put(`${holdingId}/${id}`, bytes, {
            onlyIf: { etagDoesNotMatch: "*" },
            httpMetadata: { contentType: mime },
          });
          if (!saved) {
            const existing = await store.head(`${holdingId}/${id}`);
            if (existing?.etag !== sha)
              throw new ApiError(
                409,
                "KEY_CONFLICT",
                "Upload key already used.",
              );
          }
          await db.pool.query(
            "INSERT INTO billing_evidence VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            [
              id,
              holdingId,
              filename,
              mime,
              size,
              sha,
              actor.userId,
              Date.now(),
            ],
          );
          return Response.json({ data: { id } });
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
