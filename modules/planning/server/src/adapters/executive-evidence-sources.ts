import "server-only";
// Strict private transport for the native Planning evidence SEARCH port. Rust
// owns source eligibility, cursor/filter policy, the bounded owner query and
// the audit; this adapter only forwards the registered Holding context and
// validates the returned full EvidenceRow. No SQL, retry or fallback remains.
import { headers } from "next/headers";
import { Agent, request } from "node:http";
import { Readable } from "node:stream";
import { z } from "zod";
import { nativeHoldingConfiguration } from "@erpx/platform-native-sdk/holding-target";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { inputSchema, type EvidenceRow } from "@erpx/platform-runtime-server/public/executive/planning-evidence-contract";

type Input = z.infer<typeof inputSchema>;

const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
const whole = z.string().regex(/^(0|[1-9][0-9]*)$/);
const revision = z.number().int().positive();
const nullableRevision = revision.nullable();

/** Exact full owner row. Extra fields, wrong primitive types and unbounded
 * text are rejected; the Runtime contract accepts the resulting narrow type. */
const evidenceRowSchema = z.object({
  id: z.string().min(1).max(200),
  scenario_id: z.string().min(1).max(200),
  recommendation_id: z.string().min(1).max(200).nullable(),
  branch_id: z.string().min(1).max(200),
  branch_name: z.string().max(200).nullable(),
  warehouse_id: z.string().min(1).max(200),
  warehouse_name: z.string().max(200).nullable(),
  specification_id: z.string().min(1).max(200),
  specification_name: z.string().max(200).nullable(),
  output_product_id: z.string().min(1).max(200).nullable(),
  output_product_name: z.string().max(200).nullable(),
  specification_version: revision,
  demand_quantity_milli: whole,
  due_date: date,
  capacity_minutes: whole,
  scenario_status: z.string().min(1).max(80),
  scenario_version: revision,
  scenario_created_at: z.number().int().positive(),
  scenario_updated_at: z.number().int().positive(),
  recommendation_status: z.string().min(1).max(80).nullable(),
  recommended_quantity_milli: whole.nullable(),
  required_minutes: whole.nullable(),
  feasible_date: date.nullable(),
  recommendation_version: nullableRevision,
  scenario_version_ref: nullableRevision,
  recommendation_created_at: z.number().int().positive().nullable(),
  recommendation_updated_at: z.number().int().positive().nullable(),
  shortage_count: whole,
  shortage_items_json: z.string().max(1_048_576),
}).strict();

const rowsSchema = z.object({ rows: z.array(evidenceRowSchema).max(51) }).strict();

function fail(code = "NATIVE_PLANNING_EVIDENCE_UNAVAILABLE", status = 503): never {
  throw new ApiError(status, code, "The native Planning evidence is unavailable.");
}

function profile() {
  return process.env.NODE_ENV === "development"
    && process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE === "container-localhost"
    ? "contributor" as const
    : "public" as const;
}

type Config = ReturnType<typeof nativeHoldingConfiguration>;

async function send(
  config: Config,
  slug: string,
  headersToSend: Headers,
  timeoutMs: number,
  body: string,
): Promise<{ status: number; body: unknown }> {
  let base: URL;
  try { base = new URL(config.privateBaseUrl); } catch { return fail("NATIVE_PLANNING_EVIDENCE_CONFIGURATION_INVALID"); }
  if (base.origin !== config.privateBaseUrl) return fail("NATIVE_PLANNING_EVIDENCE_CONFIGURATION_INVALID");
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(slug)}/planning/evidence/search`, base);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await new Promise<Response>((resolve, reject) => {
      const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import("node:http").ProxyEnv });
      const req = request(url, {
        method: "POST",
        headers: Object.fromEntries(headersToSend),
        agent,
        signal: controller.signal,
        maxHeaderSize: 8192,
      }, incoming => {
        incoming.once("close", () => agent.destroy());
        try {
          const h = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            h.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
            status: incoming.statusCode ?? 502, headers: h,
          }));
        } catch (error) { incoming.destroy(); agent.destroy(); reject(error); }
      });
      req.once("error", error => { agent.destroy(); reject(error); });
      req.end(body);
    });
    if (response.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json" || !response.body)
      return fail("NATIVE_PLANNING_EVIDENCE_RESPONSE_INVALID", 502);
    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    const cancel = () => { void reader.cancel().catch(() => undefined); };
    controller.signal.addEventListener("abort", cancel, { once: true });
    try {
      for (;;) {
        const part = await reader.read();
        if (controller.signal.aborted) return fail();
        if (part.done) break;
        size += part.value.length;
        if (size > 2 * 1024 * 1024) { await reader.cancel(); return fail("NATIVE_PLANNING_EVIDENCE_RESPONSE_LIMIT", 502); }
        chunks.push(part.value);
      }
    } finally { controller.signal.removeEventListener("abort", cancel); reader.releaseLock(); }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
    let parsed: unknown;
    try { parsed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
    catch { return fail("NATIVE_PLANNING_EVIDENCE_RESPONSE_INVALID", 502); }
    return { status: response.status, body: parsed };
  } finally { clearTimeout(timer); }
}

async function nativeSearch(actor: WorkContext, input: Input): Promise<EvidenceRow[]> {
  const raw = process.env.ERPX_NATIVE_PLANNING_EVIDENCE_CONFIG;
  if (!raw) return fail("NATIVE_PLANNING_EVIDENCE_CONFIGURATION_REQUIRED");
  let config: Config;
  try { config = nativeHoldingConfiguration(raw, profile()); }
  catch { return fail("NATIVE_PLANNING_EVIDENCE_CONFIGURATION_INVALID"); }

  const incoming = await headers();
  const target = config.holdings.find(target =>
    new URL(target.origin).host === incoming.get("host")
    && target.holdingId === actor.holdingId
    && target.holdingSlug === actor.holdingSlug);
  if (!target || !actor.authority || !actor.iam) return fail("WORK_CONTEXT_CHANGED", 409);
  const origin = incoming.get("origin");
  if (origin !== null && origin !== target.origin) return fail("ORIGIN_FORBIDDEN", 403);

  const h = new Headers({
    host: new URL(target.origin).host,
    accept: "application/json",
    "content-type": "application/json",
    "cache-control": "no-cache",
    "x-erpx-context-company": actor.companyId,
    "x-erpx-context-assignment": actor.assignmentId,
    "x-erpx-context-revision": String(actor.authority.revision),
    "x-erpx-context-assignment-version": String(actor.authority.assignmentVersion),
  });
  const cookie = incoming.get("cookie");
  if (cookie) { if (Buffer.byteLength(cookie) > 8192) return fail(); h.set("cookie", cookie); }
  if (origin) h.set("origin", origin);
  if (actor.scope.kind !== "holding" && actor.scope.branchSlug)
    h.set("x-erpx-workspace-branch", actor.scope.branchSlug);
  if (actor.scope.kind === "department" && actor.scope.departmentSlug)
    h.set("x-erpx-workspace-department", actor.scope.departmentSlug);

  const body = JSON.stringify({
    metric: input.metric,
    from: input.filters.from,
    to: input.filters.to,
    query: input.query,
    cursor: input.cursor ?? null,
    limit: input.limit,
  });
  if (new TextEncoder().encode(body).length > 16 * 1024) return fail("EVIDENCE_INPUT_INVALID", 422);

  let reply: Awaited<ReturnType<typeof send>>;
  try { reply = await send(config, target.holdingSlug, h, config.timeoutMs, body); }
  catch { return fail(); }
  if (reply.status !== 200) {
    const errorBody = reply.body as { error?: { code?: unknown } } | null;
    if (errorBody && typeof errorBody === "object" && typeof errorBody.error?.code === "string")
      return fail(errorBody.error.code, [401, 403, 404, 409, 422].includes(reply.status) ? reply.status : 503);
    return fail();
  }
  const parsed = rowsSchema.safeParse((reply.body as { data?: unknown } | null)?.data);
  if (!parsed.success) return fail("NATIVE_PLANNING_EVIDENCE_RESPONSE_INVALID", 502);
  const rows = parsed.data.rows as EvidenceRow[];
  if (new Set(rows.map(row => row.id)).size !== rows.length)
    return fail("NATIVE_PLANNING_EVIDENCE_RESPONSE_INVALID", 502);
  return rows;
}

/** Owner source projection used by the Runtime planning-evidence application.
 * The signature is retained so the consumer switch is a transport change only;
 * the implementation is now the strict native owner client above. */
export function nativePlanningEvidenceSources(_database?: unknown) {
  return {
    async searchRows0(
      actor: WorkContext,
      input: Input,
      _cursor: unknown,
      _isScenario: boolean,
    ): Promise<EvidenceRow[]> {
      return nativeSearch(actor, input);
    },
  };
}

export type NativePlanningEvidenceRow = EvidenceRow;
