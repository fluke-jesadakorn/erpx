import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { AssistantSource } from '@erpx/feature-work_projection/lib/ai/workspace-assistant';
import { scopeKey } from '@erpx/server-agent/domain/assistant';

/**
 * Fail-closed server-to-server client for the mounted native Agent assistant
 * source read at `/api/v1/holdings/<holding>/assistant/sources`. The Rust axum
 * owner owns the statement, the scope and the `sourceReference` projection.
 * This module prepares no SQL, carries no fallback and never touches the
 * retired TypeScript `assistantSources` helper: missing or invalid
 * configuration, an unmapped Holding, a failed fetch or an unexpected payload
 * surfaces a closed error.
 */
type Ref = { id: string; title: string; path: string; version: string; source?: string };

const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never =>
  fail(503, 'NATIVE_AGENT_ASSISTANT_SOURCES_UNAVAILABLE', 'The native Agent assistant source read is unavailable.');
const invalidConfig = (): never =>
  fail(503, 'NATIVE_AGENT_ASSISTANT_SOURCES_CONFIG_INVALID', 'The native Agent assistant source read is misconfigured.');
const invalidResponse = (): never =>
  fail(503, 'NATIVE_AGENT_ASSISTANT_SOURCES_RESPONSE_INVALID', 'The native Agent assistant source read returned an invalid response.');
const unsupported = (): never =>
  fail(422, 'AI_CONTEXT_UNSUPPORTED', 'This page is not available to the assistant yet.');
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const id = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f]/.test(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max && !/[\u0000-\u001f\u007f]/.test(value);
const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_SOURCES = 30;

type Parsed = {
  base: URL;
  timeoutMs: number;
  holdings: Map<string, { origin: string; holdingSlug: string; holdingId: string }>;
};

function configuration(raw: string): Parsed {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return invalidConfig(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' ||
    !Number.isSafeInteger(value.timeoutMs) || !Array.isArray(value.holdings))
    return invalidConfig();
  const timeoutMs = Number(value.timeoutMs);
  if (timeoutMs < 250 || timeoutMs > 30_000) return invalidConfig();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalidConfig(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port || reserved.has(port) || (port >= 3200 && port <= 3299) ||
    base.username || base.password || value.privateBaseUrl !== base.origin)
    return invalidConfig();
  const holdings = new Map<string, { origin: string; holdingSlug: string; holdingId: string }>();
  for (const item of value.holdings) {
    if (!record(item) || typeof item.origin !== 'string' ||
      !id(item.holdingId) || !id(item.holdingSlug, 128))
      return invalidConfig();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return invalidConfig(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.port ||
      origin.username || origin.password || holdings.has(origin.host))
      return invalidConfig();
    holdings.set(origin.host, {
      origin: item.origin,
      holdingId: item.holdingId as string,
      holdingSlug: item.holdingSlug as string,
    });
  }
  if (holdings.size < 1 || holdings.size > 64) return invalidConfig();
  return { base, timeoutMs, holdings };
}

function sourceRef(value: unknown): value is Ref {
  return record(value) && id(value.id) && text(value.title, 400) &&
    id(value.path, 1500) && id(value.version, 200) &&
    (value.source === undefined || typeof value.source === 'string');
}

/** One bounded source-reference read. No TypeScript SQL and no fallback path. */
export async function nativeAssistantSources(actor: WorkContext, path: string): Promise<AssistantSource[]> {
  const raw = process.env.ERPX_NATIVE_AGENT_CONFIG;
  if (raw === undefined) return unavailable();
  const config = configuration(raw);
  const target = [...config.holdings.values()].find(
    (item) => item.holdingSlug === actor.holdingSlug && item.holdingId === actor.holdingId,
  );
  if (!target)
    return fail(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before continuing.');
  if (!id(actor.companyId) || !id(actor.userId) || !id(actor.assignmentId)) return unsupported();
  const scope = scopeKey(actor);
  if (!text(scope, 2000) || scope.length === 0 || !text(path, 1500) || path.length === 0)
    return unsupported();
  const headers = new Headers({
    host: new URL(target.origin).host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-scope': scope,
  });
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/assistant/sources?path=${encodeURIComponent(path)}`,
    config.base,
  );
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET', headers, cache: 'no-store', redirect: 'error', signal: controller.signal,
    });
  } catch { return unavailable(); }
  finally { clearTimeout(timer); }
  if (response.status !== 200) {
    let code = 'NATIVE_AGENT_ASSISTANT_SOURCES_UNAVAILABLE';
    let message = 'The native Agent assistant source read is unavailable.';
    try {
      const error: unknown = await response.json();
      if (record(error) && record(error.error) &&
        typeof error.error.code === 'string' && typeof error.error.message === 'string') {
        code = error.error.code;
        message = error.error.message;
      }
    } catch { /* keep the closed default */ }
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status, code, message);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return invalidResponse();
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return invalidResponse(); }
  if (!record(parsed) || Object.keys(parsed).length !== 1 || !Object.hasOwn(parsed, 'sources') ||
    !Array.isArray(parsed.sources) || parsed.sources.length > MAX_SOURCES ||
    !parsed.sources.every(sourceRef))
    return invalidResponse();
  const sources: AssistantSource[] = (parsed.sources as Ref[]).map((item) => ({
    id: item.id, title: item.title, path: item.path, version: item.version, data: undefined,
  }));
  return sources;
}
