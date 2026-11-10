import type { BillingView } from "@erpx/feature-billing/lib/billing";

/**
 * Pure Billing owner transport. It carries the retained browser Host, cookie
 * and current context headers to the private native owner router and validates
 * the exact response before a page renders it. It never mints a session,
 * Holding, Platform role or command key.
 */
export type NativeBillingPageContext = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  platform: boolean;
  role: string;
  assignmentId: string;
  expiresAt?: number | null;
};

export type NativeBillingReadInput = Readonly<{
  holdingId: string;
  holdingSlug: string;
}>;

export type NativeBillingConfiguration = Readonly<{
  privateBaseUrl: string;
  holdings: readonly Readonly<{
    origin: string;
    holdingSlug: string;
    holdingId: string;
  }>[];
  timeoutMs: number;
}>;

export type NativeBillingTransport = (
  method: "GET" | "POST",
  url: URL,
  headers: Headers,
  body: string | undefined,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeBillingCommandInput = Readonly<{
  holdingId: string;
  holdingSlug: string;
  action: "cost" | "delete-cost" | "complete-costs" | "approve-target-rollout" | "verify-provider-cost" | "verify-statement" | "retry" | "record-operating-cost" | "review-target-capacity" | "approve-target-extra" | "rate-target";
  key: string;
  body: Readonly<Record<string, unknown>>;
}>;

type UnknownRow = Record<string, unknown>;
const failure = (code: string, status?: number, message?: string): never => {
  throw new NativeBillingError(code, status, message);
};
const record = (value: unknown): value is UnknownRow =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is UnknownRow =>
  record(value) &&
  Object.keys(value).length === keys.length &&
  keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  new TextEncoder().encode(value).length <= max &&
  !/[\u0000-\u001f\u007f]/.test(value);
const optionalText = (value: unknown, max = 4096) =>
  value === null || text(value, max);
const lineText = (value: unknown, max = 4096): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  new TextEncoder().encode(value).length <= max &&
  !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value);
const identifier = (value: unknown): value is string =>
  text(value, 200);
const optionalIdentifier = (value: unknown) =>
  value === null || identifier(value);
const integer = (value: unknown, min = 0): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= min;
const optionalInteger = (value: unknown, min = 0) =>
  value === null || integer(value, min);
const boolean = (value: unknown): value is boolean =>
  typeof value === "boolean";
const array = (value: unknown, max: number): value is unknown[] =>
  Array.isArray(value) && value.length <= max;
const rows = (value: unknown, max: number): value is UnknownRow[] =>
  array(value, max) && value.every(record);
const month = (value: unknown): value is string =>
  typeof value === "string" && /^\d{4}-(0[1-9]|1[0-2])$/.test(value);
const planId = (value: unknown): value is string =>
  typeof value === "string" &&
  ["infrastructure", "standard", "priority", "platform_usage"].includes(value);
const invoiceLines = (value: unknown): value is string => {
  if (typeof value !== "string" || new TextEncoder().encode(value).length > 4 * 1024 * 1024)
    return false;
  try {
    const parsed: unknown = JSON.parse(value);
    return Array.isArray(parsed) && parsed.length <= 4096;
  } catch {
    return false;
  }
};
const unique = <T>(values: readonly T[]) => new Set(values).size === values.length;

const ACCOUNT_KEYS = [
  "holding_id",
  "version",
  "customer_id",
  "payment_mode",
  "consent_at",
  "setup_state",
  "card_json",
] as const;
const ENROLLMENT_KEYS = [
  "id",
  "plan_id",
  "started_at",
  "ended_at",
  "pending_plan",
  "pending_end",
  "effective_at",
] as const;
const PLAN_KEYS = [
  "id",
  "revision",
  "name",
  "support_satang",
  "effective_month",
] as const;
const PERIOD_KEYS = [
  "id",
  "month",
  "start_at",
  "end_at",
  "plan_id",
  "plan_revision",
  "support_satang",
  "costs_complete",
  "closed_at",
] as const;
const COST_KEYS = [
  "id",
  "period_id",
  "category",
  "amount_satang",
  "service_start",
  "service_end",
  "note",
  "simulated",
  "evidence_id",
  "invoice_id",
  "original_invoice_id",
] as const;
const INVOICE_KEYS = [
  "id",
  "period_id",
  "kind",
  "status",
  "total_satang",
  "lines_json",
  "stripe_id",
  "hosted_url",
  "pdf_url",
  "due_at",
  "payment_state",
  "created_at",
  "original_invoice_id",
] as const;
const AUDIT_KEYS = ["id", "actor", "action", "reason", "created_at"] as const;
const JOB_KEYS = ["id", "state", "last_error"] as const;
const LEGACY_KEYS = ["status", "subscription_id"] as const;
const CONTEXT_KEYS = [
  "userId",
  "holdingId",
  "platform",
  "role",
  "assignmentId",
  "expiresAt",
] as const;
const WORKSPACE_KEYS = [
  "sandbox",
  "now",
  "account",
  "enrollment",
  "plans",
  "periods",
  "costs",
  "invoices",
  "audit",
  "jobs",
  "legacy",
] as const;

export class NativeBillingError extends Error {
  readonly code: string;
  readonly status?: number;
  constructor(code: string, status?: number, message?: string) {
    super(message ?? "The native Billing request could not be completed.");
    this.name = "NativeBillingError";
    this.code = code;
    this.status = status;
  }
}

function context(
  value: unknown,
  actor: NativeBillingPageContext,
  input: NativeBillingReadInput,
): UnknownRow {
  if (!exact(value, CONTEXT_KEYS)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (
    value.userId !== actor.userId ||
    value.holdingId !== input.holdingId ||
    value.holdingId !== actor.holdingId ||
    value.platform !== actor.platform ||
    value.role !== actor.role ||
    value.assignmentId !== actor.assignmentId ||
    (value.expiresAt !== null && !integer(value.expiresAt, 1)) ||
    (value.expiresAt ?? null) !== (actor.expiresAt ?? null)
  )
    return failure("NATIVE_BILLING_CONTEXT_CHANGED");
  return value;
}

function accountRow(value: unknown, holdingId: string): boolean {
  if (!exact(value, ACCOUNT_KEYS)) return false;
  return (
    value.holding_id === holdingId &&
    integer(value.version) &&
    optionalIdentifier(value.customer_id) &&
    (value.payment_mode === "manual" || value.payment_mode === "auto") &&
    optionalInteger(value.consent_at) &&
    optionalText(value.setup_state, 80) &&
    (value.card_json === null ||
      (typeof value.card_json === "string" &&
        new TextEncoder().encode(value.card_json).length <= 16 * 1024))
  );
}

function enrollmentRow(value: unknown): boolean {
  if (!exact(value, ENROLLMENT_KEYS)) return false;
  return (
    identifier(value.id) &&
    planId(value.plan_id) &&
    integer(value.started_at) &&
    optionalInteger(value.ended_at) &&
    (value.ended_at === null || value.ended_at >= value.started_at) &&
    (value.pending_plan === null || planId(value.pending_plan)) &&
    boolean(value.pending_end) &&
    optionalInteger(value.effective_at) &&
    !(value.pending_plan !== null && value.pending_end === true)
  );
}

function planRow(value: unknown): boolean {
  if (!exact(value, PLAN_KEYS)) return false;
  return (
    planId(value.id) &&
    integer(value.revision, 1) &&
    text(value.name, 160) &&
    integer(value.support_satang) &&
    value.support_satang <= 100_000_000 &&
    month(value.effective_month)
  );
}

function periodRow(value: unknown): boolean {
  if (!exact(value, PERIOD_KEYS)) return false;
  return (
    identifier(value.id) &&
    month(value.month) &&
    integer(value.start_at) &&
    integer(value.end_at) &&
    value.end_at > value.start_at &&
    planId(value.plan_id) &&
    integer(value.plan_revision, 1) &&
    integer(value.support_satang) &&
    boolean(value.costs_complete) &&
    optionalInteger(value.closed_at)
  );
}

function costRow(value: unknown): boolean {
  if (!exact(value, COST_KEYS)) return false;
  return (
    identifier(value.id) &&
    identifier(value.period_id) &&
    ["hosting", "storage", "provider_ai", "adjustment"].includes(
      String(value.category),
    ) &&
    typeof value.amount_satang === "number" &&
    Number.isSafeInteger(value.amount_satang) &&
    value.amount_satang >= -100_000_000 &&
    value.amount_satang <= 100_000_000 &&
    (value.category === "adjustment" || value.amount_satang >= 0) &&
    integer(value.service_start) &&
    integer(value.service_end) &&
    Number(value.service_end) > Number(value.service_start) &&
    lineText(value.note, 2000) &&
    boolean(value.simulated) &&
    optionalIdentifier(value.evidence_id) &&
    optionalIdentifier(value.invoice_id) &&
    optionalIdentifier(value.original_invoice_id)
  );
}

function invoiceRow(value: unknown): boolean {
  if (!exact(value, INVOICE_KEYS)) return false;
  return (
    identifier(value.id) &&
    identifier(value.period_id) &&
    (value.kind === "main" || value.kind === "adjustment") &&
    text(value.status, 40) &&
    integer(value.total_satang) &&
    invoiceLines(value.lines_json) &&
    optionalIdentifier(value.stripe_id) &&
    (value.hosted_url === null ||
      (typeof value.hosted_url === "string" &&
        new TextEncoder().encode(value.hosted_url).length <= 2048)) &&
    (value.pdf_url === null ||
      (typeof value.pdf_url === "string" &&
        new TextEncoder().encode(value.pdf_url).length <= 2048)) &&
    optionalInteger(value.due_at) &&
    optionalText(value.payment_state, 80) &&
    integer(value.created_at) &&
    optionalIdentifier(value.original_invoice_id)
  );
}

function auditRow(value: unknown): boolean {
  if (!exact(value, AUDIT_KEYS)) return false;
  return (
    identifier(value.id) &&
    text(value.actor, 200) &&
    text(value.action, 80) &&
    lineText(value.reason, 1000) &&
    integer(value.created_at)
  );
}

function jobRow(value: unknown): boolean {
  if (!exact(value, JOB_KEYS)) return false;
  return (
    identifier(value.id) &&
    text(value.state, 40) &&
    (value.last_error === null ||
      (typeof value.last_error === "string" &&
        new TextEncoder().encode(value.last_error).length <= 4000))
  );
}

function legacyRow(value: unknown): boolean {
  if (!exact(value, LEGACY_KEYS)) return false;
  return text(value.status, 80) && optionalIdentifier(value.subscription_id);
}

function descending(
  rows: readonly UnknownRow[],
  key: "month" | "created_at",
): boolean {
  for (let index = 1; index < rows.length; index += 1) {
    const previous = rows[index - 1]!;
    const current = rows[index]!;
    if (key === "month") {
      const left = String(previous.month);
      const right = String(current.month);
      if (left < right) return false;
      if (left === right && String(previous.id) < String(current.id)) return false;
      continue;
    }
    const left = Number(previous.created_at);
    const right = Number(current.created_at);
    if (left < right) return false;
    if (left === right && String(previous.id) < String(current.id)) return false;
  }
  return true;
}

function workspaceData(
  value: unknown,
  actor: NativeBillingPageContext,
  input: NativeBillingReadInput,
): BillingView {
  if (!exact(value, WORKSPACE_KEYS)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  const account = value.account;
  const enrollment = value.enrollment;
  const legacy = value.legacy;
  const plans = value.plans;
  const periods = value.periods;
  const costs = value.costs;
  const invoices = value.invoices;
  const audit = value.audit;
  const jobs = value.jobs;
  if (
    value.sandbox !== true ||
    !integer(value.now) ||
    (account !== null && !accountRow(account, input.holdingId)) ||
    (enrollment !== null && !enrollmentRow(enrollment)) ||
    !rows(plans, 500) ||
    !rows(periods, 500) ||
    !rows(costs, 500) ||
    !rows(invoices, 500) ||
    !rows(audit, 100) ||
    !rows(jobs, 500) ||
    (legacy !== null && !legacyRow(legacy))
  )
    return failure("NATIVE_BILLING_RESPONSE_INVALID");

  if (!plans.every(planRow)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  const planKeys = new Set<string>();
  for (const plan of plans) {
    const key = `${String(plan.id)}\u0000${String(plan.revision)}`;
    if (planKeys.has(key)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
    planKeys.add(key);
  }
  const planRank = (id: unknown) =>
    id === "infrastructure" ? 0 : id === "standard" ? 1 : id === "priority" ? 2 : 3;
  for (let index = 1; index < plans.length; index += 1) {
    const previous = plans[index - 1]!;
    const current = plans[index]!;
    const left = planRank(previous.id);
    const right = planRank(current.id);
    if (left > right) return failure("NATIVE_BILLING_RESPONSE_INVALID");
    if (
      left === right &&
      Number(previous.revision) > 0 &&
      Number(previous.revision) < Number(current.revision)
    )
      return failure("NATIVE_BILLING_RESPONSE_INVALID");
  }

  if (!periods.every(periodRow)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!unique(periods.map((row) => String(row.id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!descending(periods, "month")) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (
    !periods.every(
      (period) =>
        period.plan_id === "platform_usage" ||
        plans.some(
          (plan) =>
            plan.id === period.plan_id &&
            plan.revision === period.plan_revision,
        ),
    )
  )
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  const periodIds = new Set(periods.map((row) => String(row.id)));

  if (!costs.every(costRow)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!unique(costs.map((row) => String(row.id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!costs.every((row) => periodIds.has(String(row.period_id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");

  if (!invoices.every(invoiceRow)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!unique(invoices.map((row) => String(row.id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!invoices.every((row) => periodIds.has(String(row.period_id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!descending(invoices, "created_at"))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");

  if (!audit.every(auditRow)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!unique(audit.map((row) => String(row.id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!descending(audit, "created_at"))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");

  if (!jobs.every(jobRow)) return failure("NATIVE_BILLING_RESPONSE_INVALID");
  if (!unique(jobs.map((row) => String(row.id))))
    return failure("NATIVE_BILLING_RESPONSE_INVALID");

  if (
    actor.holdingId !== input.holdingId ||
    actor.holdingSlug !== input.holdingSlug ||
    (account !== null && accountRow(account, actor.holdingId) === false)
  )
    return failure("NATIVE_BILLING_CONTEXT_CHANGED");
  return value as BillingView;
}

function configuration(raw: NativeBillingConfiguration) {
  let base: URL;
  try {
    base = new URL(raw.privateBaseUrl);
  } catch {
    return failure("NATIVE_BILLING_CONFIGURATION_INVALID");
  }
  const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    raw.privateBaseUrl !== base.origin ||
    !integer(raw.timeoutMs, 250) ||
    raw.timeoutMs > 30000 ||
    !array(raw.holdings, 64) ||
    raw.holdings.length < 1
  )
    return failure("NATIVE_BILLING_CONFIGURATION_INVALID");
  const holdings = new Map<
    string,
    { origin: string; holdingSlug: string; holdingId: string }
  >();
  for (const holding of raw.holdings) {
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return failure("NATIVE_BILLING_CONFIGURATION_INVALID");
    }
    if (
      origin.protocol !== "https:" ||
      holding.origin !== origin.origin ||
      origin.port ||
      origin.username ||
      origin.password ||
      !identifier(holding.holdingId) ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      holdings.has(origin.host)
    )
      return failure("NATIVE_BILLING_CONFIGURATION_INVALID");
    holdings.set(origin.host, {
      origin: holding.origin,
      holdingSlug: holding.holdingSlug,
      holdingId: holding.holdingId,
    });
  }
  return { base, holdings, timeout: raw.timeoutMs };
}

function boundedHeader(
  incoming: Pick<Headers, "get">,
  name: string,
): string | undefined {
  const value = incoming.get(name);
  if (value === null) return undefined;
  if (
    value.length === 0 ||
    new TextEncoder().encode(value).length > 512 ||
    /[\u0000-\u001f\u007f]/.test(value)
  )
    return failure("NATIVE_BILLING_REQUEST_INVALID");
  return value;
}

function ownerCall(
  transport: NativeBillingTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, "get">,
  actor: NativeBillingPageContext,
  input: Pick<NativeBillingReadInput, "holdingId" | "holdingSlug">,
  method: "GET" | "POST",
  route: string,
  body: string | undefined,
  key: string | undefined,
  signal?: AbortSignal,
): Promise<Response> {
  const host = incoming.get("host");
  const holding = host ? config.holdings.get(host) : undefined;
  if (
    !host ||
    !holding ||
    holding.holdingSlug !== input.holdingSlug ||
    holding.holdingId !== input.holdingId ||
    actor.holdingSlug !== input.holdingSlug ||
    actor.holdingId !== input.holdingId
  )
    return Promise.reject(
      new NativeBillingError("NATIVE_BILLING_AUTHORITY_INVALID"),
    );
  const headers = new Headers({
    host,
    accept: "application/json",
    "cache-control": "no-cache",
  });
  const cookie = incoming.get("cookie");
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return Promise.reject(new NativeBillingError("NATIVE_BILLING_COOKIE_LIMIT"));
    headers.set("cookie", cookie);
  }
  for (const name of [
    "x-erpx-platform-assignment",
    "x-erpx-context-revision",
    "x-erpx-context-company",
    "x-erpx-context-assignment",
    "x-erpx-context-assignment-version",
  ]) {
    const value = boundedHeader(incoming, name);
    if (value !== undefined) headers.set(name, value);
  }
  if (method === "POST") {
    if (!key || !/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
      return Promise.reject(new NativeBillingError("NATIVE_BILLING_REQUEST_INVALID"));
    const origin = incoming.get("origin");
    if (origin && origin !== holding.origin)
      return Promise.reject(
        new NativeBillingError("ORIGIN_FORBIDDEN", 403, "Use the application origin."),
      );
    headers.set("content-type", "application/json");
    headers.set("idempotency-key", key);
    headers.set("origin", holding.origin);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(input.holdingSlug)}/billing${route}`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener("abort", cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  return transport(method, url, headers, body, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener("abort", cancel);
  });
}

function transportCall(
  transport: NativeBillingTransport,
  config: ReturnType<typeof configuration>,
  incoming: Pick<Headers, "get">,
  actor: NativeBillingPageContext,
  input: NativeBillingReadInput,
  signal?: AbortSignal,
): Promise<Response> {
  return ownerCall(transport, config, incoming, actor, input, "GET", "", undefined, undefined, signal);
}

async function boundedBytes(response: Response, limit: number): Promise<Uint8Array> {
  if (!response.body) return new Uint8Array();
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > limit) {
        await reader.cancel();
        return failure("NATIVE_BILLING_RESPONSE_LIMIT");
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.length;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return failure("NATIVE_BILLING_UNAVAILABLE", response.status);
  }
  const contentType = response.headers.get("content-type")?.split(";")[0]?.trim();
  if (response.status !== 200) {
    const body = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === "application/json") {
      try {
        const value: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(body));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    return failure(code ?? "NATIVE_BILLING_UNAVAILABLE", response.status, message);
  }
  if (contentType !== "application/json") {
    void response.body?.cancel().catch(() => undefined);
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  }
  const data = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(data));
  } catch {
    return failure("NATIVE_BILLING_RESPONSE_INVALID");
  }
}

export function nativeBillingAvailable(): boolean {
  return process.env.ERPX_NATIVE_BILLING_CONFIG !== undefined;
}

export function nativeBillingConfiguration(raw: string): NativeBillingConfiguration {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw new NativeBillingError("NATIVE_BILLING_CONFIGURATION_INVALID");
  }
  if (
    !record(value) ||
    !text(value.privateBaseUrl) ||
    !integer(value.timeoutMs) ||
    !Array.isArray(value.holdings) ||
    !value.holdings.every(
      (holding: unknown) =>
        record(holding) &&
        text(holding.origin) &&
        text(holding.holdingSlug) &&
        text(holding.holdingId),
    )
  )
    return failure("NATIVE_BILLING_CONFIGURATION_INVALID");
  return value as unknown as NativeBillingConfiguration;
}

const directTransport: NativeBillingTransport = (
  method,
  url,
  headers,
  body,
  signal,
) => fetch(url, { method, headers, body, signal, redirect: "manual", cache: "no-store" });

export function createNativeBillingClient(
  raw: NativeBillingConfiguration,
  transport: NativeBillingTransport = directTransport,
) {
  const config = configuration(raw);
  const billing = async (
    incoming: Pick<Headers, "get">,
    actor: NativeBillingPageContext,
    input: NativeBillingReadInput,
    signal?: AbortSignal,
  ): Promise<BillingView> => {
    if (!identifier(input.holdingId) || !/^[a-z0-9_-]{1,128}$/.test(input.holdingSlug))
      return failure("NATIVE_BILLING_REQUEST_INVALID");
    const response = await transportCall(
      transport,
      config,
      incoming,
      actor,
      input,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeBillingError) throw error;
      return failure("NATIVE_BILLING_UNAVAILABLE");
    });
    const value = await boundedJson(response);
    if (!exact(value, ["data", "context"]))
      return failure("NATIVE_BILLING_RESPONSE_INVALID");
    context(value.context, actor, input);
    return workspaceData(value.data, actor, input);
  };
  const command = async (
    incoming: Pick<Headers, "get">,
    actor: NativeBillingPageContext,
    input: NativeBillingCommandInput,
    signal?: AbortSignal,
  ): Promise<{ ok: true }> => {
    const route =
      input.action === "cost"
        ? "/costs/record"
        : input.action === "delete-cost"
          ? "/costs/delete"
          : input.action === "complete-costs"
            ? "/costs/complete"
            : input.action === "approve-target-rollout"
              ? "/target/rollout/approve"
              : input.action === "verify-provider-cost"
                ? "/target/provider/cost"
                : input.action === "verify-statement"
                  ? "/target/provider/statement"
                  : input.action === "retry"
                    ? "/jobs/retry"
                    : input.action === "record-operating-cost"
                      ? "/target/evidence/operating-cost"
                      : input.action === "review-target-capacity"
                        ? "/target/evidence/capacity"
                        : input.action === "approve-target-extra"
                          ? "/target/extra"
                          : "/target/rate";
    const response = await ownerCall(
      transport,
      config,
      incoming,
      actor,
      input,
      "POST",
      route,
      JSON.stringify(input.body),
      input.key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeBillingError) throw error;
      return failure("NATIVE_BILLING_UNAVAILABLE");
    });
    const value = await boundedJson(response);
    if (!exact(value, ["data", "context"]))
      return failure("NATIVE_BILLING_RESPONSE_INVALID");
    context(value.context, actor, input);
    if (!exact(value.data, ["ok"]) || value.data.ok !== true)
      return failure("NATIVE_BILLING_RESPONSE_INVALID");
    return { ok: true };
  };

  const invoice = async (
    incoming: Pick<Headers, "get">,
    actor: NativeBillingPageContext,
    input: NativeBillingReadInput & { route: string; key: string; body: unknown },
    signal?: AbortSignal,
  ) => {
    const response = await ownerCall(
      transport,
      config,
      incoming,
      actor,
      input,
      "POST",
      `/invoices${input.route}`,
      JSON.stringify(input.body),
      input.key,
      signal,
    ).catch((error: unknown) => {
      if (error instanceof NativeBillingError) throw error;
      return failure("NATIVE_BILLING_UNAVAILABLE");
    });
    const value = await boundedJson(response);
    if (!exact(value, ["data", "context"]))
      return failure("NATIVE_BILLING_RESPONSE_INVALID");
    context(value.context, actor, input);
    if (!exact(value.data, ["ok"]) || value.data.ok !== true)
      return failure("NATIVE_BILLING_RESPONSE_INVALID");
    return { ok: true };
  };

  const holding = (holdingId: string) => {
    return [...config.holdings.values()].find(
      (candidate) => candidate.holdingId === holdingId,
    );
  };
  return { billing, command, invoice, holding };
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeBillingFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeBillingError)) return undefined;
  if (error.code === "NATIVE_BILLING_CONTEXT_CHANGED")
    return {
      status: 409,
      code: "WORK_CONTEXT_CHANGED",
      message: "Your work role changed. Reload and review before retrying.",
    };
  if (error.status === 401)
    return { status: 401, code: "UNAUTHENTICATED", message: "Sign in to continue." };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === "NATIVE_BILLING_UNAVAILABLE" ? "FORBIDDEN" : error.code,
      message: error.message,
    };
  if (error.status === 404) return { status: 404, code: "NOT_FOUND", message: "Not found." };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  return {
    status: 503,
    code: "NATIVE_BILLING_UNAVAILABLE",
    message: "The native Billing projection is temporarily unavailable.",
  };
}
