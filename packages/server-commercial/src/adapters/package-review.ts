import "server-only";
import { createHash, createHmac, timingSafeEqual } from "node:crypto";
import { z } from "zod";
import { ApiError } from "@erpx/server-runtime/public/http";
import { packageCatalogueSnapshot, packageReviewRequest, samePackageReviewActor, type PackageReviewActor, type PackageReviewClaims } from "@erpx/server-commercial/domain/package-review";
import type { PackageGrantBatch } from "@erpx/server-commercial/application/package-grant-batches";

const claimsSchema = z.object({ version: z.literal(1), holdingId: z.string().min(1), companyId: z.string().min(1),
  actor: z.object({ userId: z.string(), sessionId: z.string(), assignmentId: z.string(), role: z.string(), roleRevision: z.number().int().positive() }),
  requestHash: z.string().regex(/^[a-f0-9]{64}$/), authorityRevision: z.string().regex(/^[a-f0-9]{64}$/),
  catalogueHash: z.string().regex(/^[a-f0-9]{64}$/), issuedAt: z.number().int(), expiresAt: z.number().int() }).strict();
export const packageReviewHash = (value: string) => createHash("sha256").update(value).digest("hex");
export function packageReviewSigner(secret: string, now = Date.now, purpose: "legacy-migration" | "package-lifecycle" = "legacy-migration") {
  if (secret.length < 32) throw new Error("PACKAGE_REVIEW_SECRET_REQUIRED");
  const domain = purpose === "legacy-migration" ? "erpx-development-commercial-review-v1" : "erpx-development-commercial-lifecycle-review-v1";
  const signature = (value: string) => createHmac("sha256", secret).update(`${domain}:${value}`).digest();
  return {
    issue(actor: PackageReviewActor, holdingId: string, input: PackageGrantBatch, authorityRevision: string) {
      const instant = now();
      const claims: PackageReviewClaims = { version: 1, holdingId, companyId: input.companyId, actor,
        requestHash: packageReviewHash(JSON.stringify(packageReviewRequest(input))), authorityRevision,
        catalogueHash: packageReviewHash(packageCatalogueSnapshot()), issuedAt: instant, expiresAt: instant + 300_000 };
      const body = Buffer.from(JSON.stringify(claims)).toString("base64url");
      return { token: `${body}.${signature(body).toString("base64url")}`, expiresAt: claims.expiresAt };
    },
    read(token: string, actor: PackageReviewActor, holdingId: string, input: PackageGrantBatch) {
      const invalid = () => new ApiError(409, "PACKAGE_REVIEW_INVALID", purpose === "legacy-migration" ? "Preview this Company migration again." : "Preview these Company packages again.");
      if (token.length > 8192 || !/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]{43}$/.test(token)) throw invalid();
      const [body, mac] = token.split("."), expected = signature(body), received = Buffer.from(mac, "base64url");
      if (received.length !== expected.length || !timingSafeEqual(expected, received)) throw invalid();
      let parsed: unknown;
      try { parsed = JSON.parse(Buffer.from(body, "base64url").toString("utf8")); } catch { throw invalid(); }
      const result = claimsSchema.safeParse(parsed);
      if (!result.success) throw invalid();
      const claims = result.data;
      if (!samePackageReviewActor(claims.actor, actor) || claims.holdingId !== holdingId || claims.companyId !== input.companyId ||
        claims.requestHash !== packageReviewHash(JSON.stringify(packageReviewRequest(input))) || claims.catalogueHash !== packageReviewHash(packageCatalogueSnapshot()) ||
        claims.issuedAt > now() || claims.expiresAt !== claims.issuedAt + 300_000) throw invalid();
      // Current authority is checked even on replay. Expiry is enforced inside
      // the new-write callback, so a committed receipt remains recoverable.
      return claims;
    },
    assertFresh(claims: PackageReviewClaims) {
      if (claims.expiresAt <= now()) throw new ApiError(409, "PACKAGE_REVIEW_EXPIRED", "This preview expired. Preview the current Company again.");
    },
  };
}
