import "server-only";
import { z } from "zod";
import { currentSession, requirePlatformContext } from "../../identity_access/public/authorization";
import { withPlatformCommercialAuthority, type PlatformCommercialLease } from "../../identity_access/public/platform-commercial-authority";
import { activeHoldingBindings, holdingResources, runtimeEnv } from "../../runtime/public/runtime";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import { ApiError, errorResponse } from "../../runtime/public/http";
import { commercialCompanies } from "../../organization/public/commercial-companies";
import { configuredProductOrigin, productHostForHost } from "@/lib/runtime/product-hosts";
import { legacyGrantMigration } from "./legacy-grant-migration";
import { companyPackageLifecycle } from "./company-package-lifecycle";

const text = z.string().trim().min(3).max(1000);
const previewSchema = z.object({ holdingId: z.string().min(1).max(100), companyId: z.string().min(1).max(100), reason: text, evidence: text }).strict();
const applySchema = z.object({ holdingId: z.string().min(1).max(100), token: z.string().min(1).max(8192), request: z.object({
  companyId: z.string().min(1).max(100), expectedRevision: z.string().regex(/^[a-f0-9]{64}$/),
  idempotencyKey: z.string().regex(/^[A-Za-z0-9_-]{16,128}$/), changes: z.array(z.object({ expectedVersion: z.number().int().nonnegative(), grant: z.object({
    companyId: z.string().min(1).max(100), packageId: z.string().min(1).max(100), packageVersion: z.number().int().positive(), state: z.enum(["active", "draining", "inactive"]),
    startsAt: z.number().int().nonnegative(), endsAt: z.number().int().nonnegative().nullable(), version: z.number().int().positive(), reason: text, evidence: text,
  }).strict() }).strict()).min(1).max(100),
}).strict() }).strict();
const lifecyclePreviewSchema = applySchema.omit({ token: true });
const companySchema = z.object({ holdingId: z.string().min(1).max(100), companyId: z.string().min(1).max(100) }).strict();

async function provider<T>(manage: boolean, run: (lease: PlatformCommercialLease, env: Awaited<ReturnType<typeof runtimeEnv>>) => Promise<T>) {
  const [{ session }, context, env] = await Promise.all([currentSession(), requirePlatformContext(), runtimeEnv()]);
  if (env.ERPX_ENVIRONMENT !== "local") throw new ApiError(404, "NOT_FOUND", "Not found.");
  return withPlatformCommercialAuthority(env.CONTROL_DB as PostgresDatabase,
    { userId: context.userId, sessionId: session.session.id, assignmentId: context.assignmentId, role: context.role, roleRevision: context.iam!.roleRevision },
    manage ? "platform.commercial.manage" : "platform.commercial.read", manage, lease => run(lease, env));
}

export async function providerCommercialOverview() {
  return provider(false, async lease => {
    const holdings = await activeHoldingBindings();
    const companies = [];
    for (const holding of holdings) for (const company of await commercialCompanies((await holdingResources(holding.id)).write))
      companies.push({ ...company, holdingId: holding.id, holding: holding.displayName });
    return { companies, canApply: lease.canManage };
  });
}

export async function providerCompanyPackageOverview(holdingId: string, companyId: string) {
  return provider(false, async (lease, env) => {
    if (!(await activeHoldingBindings()).some(holding => holding.id === holdingId)) throw new ApiError(404, "HOLDING_NOT_FOUND", "Select an active Holding.");
    return companyPackageLifecycle((await holdingResources(holdingId)).write as PostgresDatabase, holdingId, lease, env.BETTER_AUTH_SECRET).overview(companyId);
  });
}

export async function providerCommercialApi(request: Request, path: string[]) {
  const headers = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff" };
  try {
    const origin = configuredProductOrigin("admin");
    if (!origin || productHostForHost(request.headers.get("host")) !== "admin") throw new ApiError(404, "NOT_FOUND", "Not found.");
    if (request.method === "GET" && path.length === 0) return Response.json({ data: await providerCommercialOverview() }, { headers });
    if (request.method === "GET" && path.join("/") === "packages") {
      const query = new URL(request.url).searchParams;
      if (query.getAll("holdingId").length !== 1 || query.getAll("companyId").length !== 1) throw new ApiError(422, "COMPANY_REQUIRED", "Select one Company and Holding.");
      const input = companySchema.parse(Object.fromEntries(query));
      return Response.json({ data: await providerCompanyPackageOverview(input.holdingId, input.companyId) }, { headers });
    }
    if (request.method !== "POST") return new Response("Method not allowed", { status: 405, headers: { ...headers, Allow: "GET, POST" } });
    if (request.headers.get("origin") !== origin) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use ERPX Administration to submit this request.");
    if (request.headers.get("content-type")?.split(";", 1)[0].trim().toLowerCase() !== "application/json") throw new ApiError(415, "JSON_REQUIRED", "Submit a JSON request.");
    const body = await request.text();
    if (body.length > 200_000) throw new ApiError(413, "REQUEST_TOO_LARGE", "This request is too large.");
    let input: unknown;
    try { input = JSON.parse(body); } catch { throw new ApiError(422, "INVALID_JSON", "Submit a valid JSON request."); }
    if (!["legacy-grants/preview", "legacy-grants/apply", "packages/preview", "packages/apply"].includes(path.join("/"))) throw new ApiError(404, "NOT_FOUND", "Not found.");
    const apply = path.at(-1) === "apply";
    const result = await provider(apply, async (lease, env) => {
      const action = apply ? { kind: "apply" as const, value: applySchema.parse(input) }
        : path[0] === "packages" ? { kind: "packages-preview" as const, value: lifecyclePreviewSchema.parse(input) }
          : { kind: "legacy-preview" as const, value: previewSchema.parse(input) };
      const holdingId = action.value.holdingId;
      if (!(await activeHoldingBindings()).some(holding => holding.id === holdingId)) throw new ApiError(404, "HOLDING_NOT_FOUND", "Select an active Holding.");
      const { write } = await holdingResources(holdingId);
      const lifecycle = path[0] === "packages" ? companyPackageLifecycle(write as PostgresDatabase, holdingId, lease, env.BETTER_AUTH_SECRET) : null;
      const service = legacyGrantMigration(write as PostgresDatabase, holdingId, lease, env.BETTER_AUTH_SECRET);
      if (action.kind === "apply") {
        const value = action.value;
        if (request.headers.get("Idempotency-Key") !== value.request.idempotencyKey) throw new ApiError(422, "IDEMPOTENCY_KEY_REQUIRED", "Use the reviewed Idempotency-Key.");
        return { receipt: lifecycle ? await lifecycle.apply(value.request, value.token) : await service.apply(value.request, value.token) };
      }
      if (action.kind === "packages-preview") return lifecycle!.preview(action.value.request);
      return service.preview(action.value.companyId, action.value.reason, action.value.evidence);
    });
    return Response.json({ data: result }, { headers });
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("UNMAPPED_LEGACY_MODULE:"))
      error = new ApiError(409, "LEGACY_MODULE_REVIEW_REQUIRED", "Legacy module labels do not match the registry. Review the Company baseline before migration.");
    if (error instanceof Error && ["STALE_PACKAGE_BATCH", "STALE_PACKAGE_GRANT", "IDEMPOTENCY_CONFLICT", "PACKAGE_DEPENDENCY_REQUIRED", "PACKAGE_DEPENDENCY_PERIOD_REQUIRED", "PACKAGE_DEPENDENCY_IN_USE", "PACKAGE_DEPENDENCY_PERIOD_CONFLICT", "DRAIN_PACKAGE_FIRST", "PACKAGE_OBLIGATIONS_REMAIN", "PACKAGE_OBLIGATION_SOURCES_REQUIRED"].includes(error.message))
      error = new ApiError(409, error.message, "Company packages changed or dependencies require review. Preview again.");
    if (error instanceof Error && ["UNKNOWN_PACKAGE_VERSION", "INVALID_PACKAGE_STATE", "GRANT_EVIDENCE_REQUIRED", "INVALID_GRANT_PERIOD", "PACKAGE_GRANT_IDENTITY_MISMATCH"].includes(error.message))
      error = new ApiError(422, error.message, "Review the package version, state, period and evidence.");
    const response = errorResponse(error);
    if (response.status >= 500) {
      const code = (error as { code?: unknown })?.code;
      const message = error instanceof Error ? error.message : "";
      // Diagnostics identify a server/SQL failure without logging request
      // bodies, review tokens, database URLs or authentication material.
      console.error(JSON.stringify({ event: "provider_commercial_failed", code:
        typeof code === "string" && /^[A-Z0-9_]{2,80}$/.test(code) ? code : /^[A-Z][A-Z0-9_]{2,100}$/.test(message) ? message : "UNCLASSIFIED" }));
    }
    for (const [key, value] of Object.entries(headers)) response.headers.set(key, value);
    return response;
  }
}
