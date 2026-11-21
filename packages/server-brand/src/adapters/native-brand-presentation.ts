import "server-only";
import { Agent, request as nodeRequest } from "node:http";
import { Readable } from "node:stream";
import { headers } from "next/headers";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/index";
import type {
  DocumentBrand,
  DocumentBrandMode,
  StorefrontAppBrand,
  WorkspaceBrand,
} from "@erpx/server-brand/domain/branding";

/**
 * Strict private-loopback client for the native Brand presentation reads.
 *
 * The native route re-verifies the session, the selected work context and the
 * Brand `records.read`/module authority; this client grants nothing. It forwards
 * only the captured cookie and the selected-work context headers. Every
 * transport, configuration and response failure is a named fail-closed error, so
 * a switched consumer never falls back to the replaced TypeScript SQL read.
 */

export type NativeBrandProfileEvidence = Readonly<{
  id: string;
  branch_name: string;
  document_mode: string;
  updated_at: number;
  primary_attachment_id: string | null;
  inverse_attachment_id: string | null;
}>;

export type NativeBrandProfiles = Readonly<{
  holding: WorkspaceBrand;
  branches: WorkspaceBrand[];
  storefront: StorefrontAppBrand;
}>;

export type NativeBrandDocumentScope = Readonly<{
  branchId: string | null;
  requestedMode?: string | null;
}>;

export type NativeBrandPresentationTransport = (
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

type Row = Record<string, unknown>;
type NativeBrandPresentationConfiguration = Readonly<{
  base: URL;
  timeout: number;
  holdings: readonly Readonly<{ host: string; holdingId: string; holdingSlug: string }>[];
}>;

const CONFIG_ENV = "ERPX_NATIVE_BRANDING_CONFIG";
const RESPONSE_LIMIT = 1024 * 1024;
const MAX_BRANCHES = 5_000;
const DOCUMENT_MODES: readonly DocumentBrandMode[] = ["holding", "branch", "both"];

const object = (value: unknown): value is Row =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === "string" && new TextEncoder().encode(value).length <= max;
const label = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && new TextEncoder().encode(value).length <= 2048;
const identifier = (value: unknown): value is string =>
  typeof value === "string" &&
  value.length > 0 &&
  value.length <= 200 &&
  !/[\u0000-\u001f\u007f/\\]/.test(value);
const nullableIdentifier = (value: unknown) => value === null || identifier(value);
const timestamp = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
const mediaUrl = (value: unknown): value is string | null =>
  value === null || (typeof value === "string" && value.length > 0 && value.length <= 4096);

function failure(code: string, status = 503): never {
  throw new BusinessRuleError(
    status,
    code,
    status === 503
      ? "The Brand presentation is unavailable."
      : "The Brand presentation request could not be completed.",
  );
}

function toWorkspaceBrand(value: unknown): WorkspaceBrand {
  if (
    !object(value) ||
    !["holding", "branch"].includes(String(value.scopeKind)) ||
    !identifier(value.scopeId) ||
    !label(value.label) ||
    !text(value.initials, 16) ||
    !mediaUrl(value.primaryUrl) ||
    !mediaUrl(value.inverseUrl) ||
    !DOCUMENT_MODES.includes(value.documentMode as DocumentBrandMode)
  )
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  return {
    scopeKind: value.scopeKind as WorkspaceBrand["scopeKind"],
    scopeId: value.scopeId,
    label: value.label,
    initials: value.initials,
    primaryUrl: value.primaryUrl,
    inverseUrl: value.inverseUrl,
    documentMode: value.documentMode as DocumentBrandMode,
  };
}

function toStorefrontBrand(value: unknown): StorefrontAppBrand {
  if (
    !object(value) ||
    !identifier(value.companyId) ||
    !identifier(value.holdingSlug) ||
    !identifier(value.companySlug) ||
    !label(value.companyName) ||
    !label(value.shortName) ||
    typeof value.accentColor !== "string" ||
    !/^#[0-9a-fA-F]{6}$/.test(value.accentColor) ||
    !mediaUrl(value.logoUrl) ||
    !(value.identityRevision === undefined || timestamp(value.identityRevision))
  )
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  return {
    companyId: value.companyId,
    holdingSlug: value.holdingSlug,
    companySlug: value.companySlug,
    companyName: value.companyName,
    shortName: value.shortName,
    accentColor: value.accentColor,
    logoUrl: value.logoUrl,
    ...(value.identityRevision === undefined ? {} : { identityRevision: value.identityRevision }),
  };
}

function toDocumentBrand(value: unknown): DocumentBrand {
  if (!object(value) || !object(value.holding) || !DOCUMENT_MODES.includes(value.mode as DocumentBrandMode))
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  const branch = value.branch;
  if (
    !label(value.companyName) ||
    !(value.branchName === null || label(value.branchName)) ||
    !(branch === null || object(branch))
  )
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  return {
    mode: value.mode as DocumentBrandMode,
    companyName: value.companyName,
    branchName: value.branchName as string | null,
    holding: toWorkspaceBrand(value.holding),
    branch: branch === null ? null : toWorkspaceBrand(branch),
  };
}

function toProfiles(value: unknown): NativeBrandProfiles {
  if (
    !object(value) ||
    !object(value.holding) ||
    !object(value.storefront) ||
    !Array.isArray(value.branches) ||
    value.branches.length > MAX_BRANCHES
  )
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  return {
    holding: toWorkspaceBrand(value.holding),
    branches: value.branches.map(toWorkspaceBrand),
    storefront: toStorefrontBrand(value.storefront),
  };
}

function toEvidence(value: unknown): NativeBrandProfileEvidence | null {
  if (value === null) return null;
  if (
    !object(value) ||
    !identifier(value.id) ||
    !label(value.branch_name) ||
    !text(value.document_mode, 32) ||
    !timestamp(value.updated_at) ||
    !nullableIdentifier(value.primary_attachment_id) ||
    !nullableIdentifier(value.inverse_attachment_id)
  )
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  return {
    id: value.id,
    branch_name: value.branch_name,
    document_mode: value.document_mode,
    updated_at: value.updated_at,
    primary_attachment_id: value.primary_attachment_id,
    inverse_attachment_id: value.inverse_attachment_id,
  };
}

function configuration(raw: string): NativeBrandPresentationConfiguration {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return failure("NATIVE_BRAND_PRESENTATION_CONFIGURATION_INVALID");
  }
  const invalid = () => failure("NATIVE_BRAND_PRESENTATION_CONFIGURATION_INVALID");
  if (!object(parsed)) return invalid();
  const privateBaseUrl = parsed.privateBaseUrl;
  const timeout = parsed.timeoutMs;
  const configured = parsed.holdings;
  if (
    !text(privateBaseUrl) ||
    !timestamp(timeout) ||
    timeout < 250 ||
    timeout > 30_000 ||
    !Array.isArray(configured) ||
    configured.length < 1 ||
    configured.length > 64
  )
    return invalid();
  const holdings: { host: string; holdingId: string; holdingSlug: string }[] = [];
  for (const holding of configured) {
    if (
      !object(holding) ||
      !identifier(holding.holdingId) ||
      !identifier(holding.holdingSlug) ||
      !text(holding.origin, 512)
    )
      return invalid();
    let origin: URL;
    try {
      origin = new URL(holding.origin);
    } catch {
      return invalid();
    }
    if (origin.protocol !== "https:" || origin.origin !== holding.origin) return invalid();
    holdings.push({
      host: origin.host,
      holdingId: holding.holdingId,
      holdingSlug: holding.holdingSlug,
    });
  }
  let base: URL;
  try {
    base = new URL(privateBaseUrl);
  } catch {
    return invalid();
  }
  const port = Number(base.port);
  if (
    base.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(base.hostname) ||
    !Number.isSafeInteger(port) ||
    port < 1 ||
    port > 65_535 ||
    base.username ||
    base.password ||
    base.origin !== privateBaseUrl.replace(/\/+$/, "")
  )
    return invalid();
  return { base, timeout, holdings };
}

function selectedHolding(
  config: NativeBrandPresentationConfiguration,
  actor: Pick<WorkContext, "holdingId" | "holdingSlug">,
  incoming: Pick<Headers, "get">,
) {
  const host = incoming.get("host");
  const byHost = host === null ? undefined : config.holdings.find((holding) => holding.host === host);
  if (byHost)
    return byHost.holdingId === actor.holdingId && byHost.holdingSlug === actor.holdingSlug
      ? byHost
      : undefined;
  return config.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
}

function requestHeaders(
  incoming: Pick<Headers, "get">,
  target: Readonly<{ host: string }>,
  actor: WorkContext,
): Headers {
  const authority = actor.authority;
  if (!authority) return failure("WORK_CONTEXT_CHANGED", 409);
  if (!label(actor.holdingName))
    return failure("NATIVE_BRAND_PRESENTATION_REQUEST_INVALID", 422);
  const values = new Headers({
    host: target.host,
    accept: "application/json",
    "cache-control": "no-cache",
    "x-erpx-holding-name": actor.holdingName,
    "x-erpx-context-company": actor.companyId,
    "x-erpx-context-assignment": actor.assignmentId,
    "x-erpx-context-revision": String(authority.revision),
    "x-erpx-context-assignment-version": String(authority.assignmentVersion),
  });
  const cookie = incoming.get("cookie");
  if (cookie !== null) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return failure("NATIVE_BRAND_PRESENTATION_COOKIE_LIMIT");
    values.set("cookie", cookie);
  }
  const branch = actor.scope.kind === "holding" ? undefined : actor.scope.branchSlug;
  if (branch !== undefined) {
    if (!identifier(branch)) return failure("NATIVE_BRAND_PRESENTATION_REQUEST_INVALID", 422);
    values.set("x-erpx-workspace-branch", branch);
  }
  if (actor.scope.kind === "department") {
    if (!identifier(actor.scope.departmentSlug))
      return failure("NATIVE_BRAND_PRESENTATION_REQUEST_INVALID", 422);
    values.set("x-erpx-workspace-department", actor.scope.departmentSlug);
  }
  return values;
}

function direct(url: URL, values: Headers, signal: AbortSignal): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import("node:http").ProxyEnv,
    });
    const outgoing = nodeRequest(
      url,
      {
        method: "GET",
        headers: Object.fromEntries(values),
        signal,
        agent,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once("close", () => agent.destroy());
        try {
          const response = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            response.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
              headers: response,
            }),
          );
        } catch (error) {
          incoming.destroy();
          reject(error);
        }
      },
    );
    outgoing.once("error", (error) => {
      agent.destroy();
      reject(error);
    });
    outgoing.end();
  });
}

async function boundedJson(reply: Response, signal: AbortSignal): Promise<unknown> {
  if (
    reply.redirected ||
    reply.status !== 200 ||
    reply.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json" ||
    !reply.body
  ) {
    void reply.body?.cancel().catch(() => undefined);
    return failure("NATIVE_BRAND_PRESENTATION_UNAVAILABLE");
  }
  const reader = reply.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const cancel = () => {
    void reader.cancel().catch(() => undefined);
  };
  signal.addEventListener("abort", cancel, { once: true });
  try {
    for (;;) {
      const part = await reader.read();
      if (signal.aborted) return failure("NATIVE_BRAND_PRESENTATION_UNAVAILABLE");
      if (part.done) break;
      size += part.value.length;
      if (size > RESPONSE_LIMIT) {
        await reader.cancel();
        return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_LIMIT", 502);
      }
      chunks.push(part.value);
    }
  } finally {
    signal.removeEventListener("abort", cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
  }
}

async function replyError(reply: Response): Promise<never> {
  const body: unknown = await reply.json().catch(() => null);
  const status = reply.status >= 400 && reply.status <= 599 ? reply.status : 503;
  const code =
    object(body) &&
    object(body.error) &&
    typeof body.error.code === "string" &&
    /^[A-Z0-9_]{3,100}$/.test(body.error.code)
      ? body.error.code
      : "NATIVE_BRAND_PRESENTATION_UNAVAILABLE";
  const message =
    object(body) &&
    object(body.error) &&
    typeof body.error.message === "string" &&
    body.error.message.length <= 300
      ? body.error.message
      : "The Brand presentation is unavailable.";
  throw new BusinessRuleError(status, code, message);
}

/** Keeps one error type for switched callers: the retained Brand reads threw
 * `BusinessRuleError`, and the shared request scope may raise `ApiError`. */
function translate(error: unknown): never {
  if (error instanceof BusinessRuleError) throw error;
  if (error instanceof ApiError) throw new BusinessRuleError(error.status, error.code, error.message);
  throw new BusinessRuleError(
    503,
    "NATIVE_BRAND_PRESENTATION_UNAVAILABLE",
    "The Brand presentation is unavailable.",
  );
}

export function createNativeBrandPresentationClient(
  raw: string,
  transport: NativeBrandPresentationTransport = direct,
) {
  const config = configuration(raw);
  const read = async (
    actor: WorkContext,
    incoming: Pick<Headers, "get">,
    kind: "workspace" | "document" | "profiles" | "evidence",
    query: Readonly<{ branch?: string; mode?: string }> = {},
  ) => {
    const target = selectedHolding(config, actor, incoming);
    if (!target) return failure("NATIVE_BRAND_PRESENTATION_TARGET_UNAVAILABLE");
    const search = new URLSearchParams({ kind });
    if (query.branch !== undefined) search.set("branch", query.branch);
    if (query.mode !== undefined) search.set("mode", query.mode);
    const endpoint = new URL(
      `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/branding/presentation?${search.toString()}`,
      config.base,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeout);
    try {
      const values = requestHeaders(incoming, target, actor);
      let reply: Response;
      try {
        reply = await transport(endpoint, values, controller.signal);
      } catch {
        return failure("NATIVE_BRAND_PRESENTATION_UNAVAILABLE");
      }
      if (reply.status !== 200) return replyError(reply);
      const body = await boundedJson(reply, controller.signal);
      if (!object(body)) return failure("NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", 502);
      return body.data;
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    async workspace(actor: WorkContext, incoming: Pick<Headers, "get">) {
      return toWorkspaceBrand(await read(actor, incoming, "workspace"));
    },
    async document(
      actor: WorkContext,
      incoming: Pick<Headers, "get">,
      scope: NativeBrandDocumentScope,
    ) {
      // A document without a Branch uses Holding mode; the retained reader
      // ignored a requested Branch mode in that case, so it is not forwarded.
      return toDocumentBrand(
        await read(
          actor,
          incoming,
          "document",
          scope.branchId === null
            ? {}
            : {
                branch: scope.branchId,
                ...(scope.requestedMode === null || scope.requestedMode === undefined
                  ? {}
                  : { mode: scope.requestedMode }),
              },
        ),
      );
    },
    async profiles(actor: WorkContext, incoming: Pick<Headers, "get">) {
      return toProfiles(await read(actor, incoming, "profiles"));
    },
    async evidence(actor: WorkContext, incoming: Pick<Headers, "get">, branchId: string) {
      if (!identifier(branchId)) return failure("NATIVE_BRAND_PRESENTATION_REQUEST_INVALID", 422);
      return toEvidence(await read(actor, incoming, "evidence", { branch: branchId }));
    },
  };
}

/** True when the native Brand presentation consumer switch is configured. */
export function nativeBrandPresentationConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[CONFIG_ENV] !== undefined;
}

function client() {
  const raw = process.env[CONFIG_ENV];
  if (raw === undefined) return failure("NATIVE_BRAND_PRESENTATION_CONFIGURATION_REQUIRED");
  return createNativeBrandPresentationClient(raw);
}

async function incoming() {
  try {
    return await headers();
  } catch {
    return failure("NATIVE_BRAND_PRESENTATION_UNAVAILABLE");
  }
}

/** Native workspace brand for the selected scope. */
export async function nativeResolveWorkspaceBrand(actor: WorkContext): Promise<WorkspaceBrand> {
  try {
    return await client().workspace(actor, await incoming());
  } catch (error) {
    return translate(error);
  }
}

/** Native document brand choice for the requested Branch and mode. */
export async function nativeResolveDocumentBrand(
  actor: WorkContext,
  scope: NativeBrandDocumentScope,
): Promise<DocumentBrand> {
  try {
    return await client().document(actor, await incoming(), scope);
  } catch (error) {
    return translate(error);
  }
}

/** Native Holding/Branch profile list plus the Company storefront overlay. */
export async function nativeListBrandProfiles(actor: WorkContext): Promise<NativeBrandProfiles> {
  try {
    return await client().profiles(actor, await incoming());
  } catch (error) {
    return translate(error);
  }
}

/** Native executive Brand profile evidence row; `null` keeps the null row. */
export async function nativeBrandProfileEvidence(
  actor: WorkContext,
  branchId: string,
): Promise<NativeBrandProfileEvidence | null> {
  try {
    return await client().evidence(actor, await incoming(), branchId);
  } catch (error) {
    return translate(error);
  }
}
