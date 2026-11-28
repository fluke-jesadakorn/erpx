import 'server-only';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { ProjectGraph } from '@erpx/projects-server/domain/graph';
import {
  NativeProjectsError,
  directHttp,
  nativeProjectsConfiguration,
  nativeProjectsTargets,
  type NativeProjectsPageContext,
} from './native-project-wbs';
import { nativeProjectsFailure, nativeProjectsPageContext } from './native-project-wbs-page';
import { createNextNativeProjectHandoffsReader } from '../adapters/server/next-native-project-handoffs';

/** The complete owner payload from the mounted Projects handoff-view read. */
export type NativeProjectHandoffView = Readonly<{
  id: string;
  company_id: string;
  project_id: string;
  source_project_version: number;
  recipient_id: string;
  status: 'pending' | 'accepted' | 'rejected' | 'cancelled';
  version: number;
  snapshot_sha256: string;
  evidence: string;
  prepared_by: string;
  prepared_at: number;
  decided_by: string | null;
  decided_at: number | null;
  decision_reason: string | null;
  snapshot: ProjectGraph;
}>;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Record<string, unknown> =>
  record(value) && Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const optionalText = (value: unknown, max = 4096) => value === null || text(value, max);
const optionalInteger = (value: unknown) => value === null || integer(value);
const statuses = new Set(['pending', 'accepted', 'rejected', 'cancelled']);

const viewKeys = [
  'id', 'company_id', 'project_id', 'source_project_version', 'recipient_id', 'status',
  'version', 'snapshot_sha256', 'evidence', 'prepared_by', 'prepared_at', 'decided_by',
  'decided_at', 'decision_reason', 'snapshot',
] as const;

function handoffView(value: unknown, projectId: string, companyId: string): NativeProjectHandoffView {
  if (
    !exact(value, viewKeys) ||
    !identifier(value.id) ||
    value.company_id !== companyId ||
    value.project_id !== projectId ||
    !integer(value.source_project_version, 1) ||
    !identifier(value.recipient_id) ||
    typeof value.status !== 'string' ||
    !statuses.has(value.status) ||
    !integer(value.version, 1) ||
    !text(value.snapshot_sha256, 64) ||
    !/^[a-f0-9]{64}$/.test(value.snapshot_sha256) ||
    !text(value.evidence, 10_000) ||
    !identifier(value.prepared_by) ||
    !integer(value.prepared_at) ||
    !optionalText(value.decided_by, 200) ||
    !optionalInteger(value.decided_at) ||
    !optionalText(value.decision_reason, 10_000) ||
    !record(value.snapshot) ||
    !record(value.snapshot.project) ||
    !Array.isArray(value.snapshot.packages) ||
    !Array.isArray(value.snapshot.tasks) ||
    !Array.isArray(value.snapshot.actuals) ||
    !record(value.snapshot.totals)
  )
    throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_INVALID');
  const row = value as unknown as NativeProjectHandoffView;
  if (row.decided_by !== null && !identifier(row.decided_by))
    throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_INVALID');
  return row;
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
        throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_LIMIT');
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
    offset += chunk.byteLength;
  }
  return data;
}

async function boundedJson(response: Response): Promise<unknown> {
  if (response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeProjectsError('NATIVE_PROJECTS_UNAVAILABLE', response.status);
  }
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200) {
    const bytes = await boundedBytes(response, 64 * 1024);
    let code: string | undefined;
    let message: string | undefined;
    if (contentType === 'application/json') {
      try {
        const value: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
        if (record(value) && record(value.error)) {
          if (text(value.error.code, 200)) code = value.error.code;
          if (text(value.error.message, 2000)) message = value.error.message;
        }
      } catch {
        // A malformed owner error keeps the bounded transport failure.
      }
    }
    throw new NativeProjectsError(code ?? 'NATIVE_PROJECTS_UNAVAILABLE', response.status, message);
  }
  if (contentType !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
  const bytes = await boundedBytes(response, 2 * 1024 * 1024);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch {
    throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_INVALID');
  }
}

/** Bounded, exact-host GET against the mounted owner handoff-view route. No
 * SQL and no TypeScript reader: an absent or invalid config fails closed. */
export async function nativeProjectHandoffs(
  incoming: Pick<Headers, 'get'>,
  actor: NativeProjectsPageContext,
  holdingSlug: string,
  projectId: string,
  signal?: AbortSignal,
): Promise<NativeProjectHandoffView[]> {
  const raw = process.env.ERPX_NATIVE_PROJECTS_CONFIG;
  if (raw === undefined) throw new NativeProjectsError('NATIVE_PROJECTS_UNAVAILABLE', 503);
  const config = nativeProjectsTargets(nativeProjectsConfiguration(raw));
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED');
  if (!identifier(projectId) || projectId.includes('/') || projectId.includes('\\'))
    throw new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID');
  const host = incoming.get('host');
  const holding = host === null ? undefined : config.holdings.get(host);
  if (!host || !holding || holding.holdingSlug !== holdingSlug)
    throw new NativeProjectsError('NATIVE_PROJECTS_AUTHORITY_INVALID');
  const headers = new Headers({
    host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-user': actor.userId,
  });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativeProjectsError('NATIVE_PROJECTS_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  if (!identifier(actor.companyId) || !identifier(actor.userId))
    throw new NativeProjectsError('NATIVE_PROJECTS_REQUEST_INVALID');
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/projects/${encodeURIComponent(projectId)}/handoffs`,
    config.base,
  );
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  let response: Response;
  try {
    response = await directHttp('GET', url, headers, undefined, controller.signal);
  } catch (error) {
    if (error instanceof NativeProjectsError) throw error;
    throw new NativeProjectsError('NATIVE_PROJECTS_UNAVAILABLE');
  } finally {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  }
  const value = await boundedJson(response);
  if (!exact(value, ['data', 'context']) || !record(value.context))
    throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_INVALID');
  if (value.context.companyId !== undefined && value.context.companyId !== actor.companyId)
    throw new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED');
  if (value.context.userId !== undefined && value.context.userId !== actor.userId)
    throw new NativeProjectsError('NATIVE_PROJECTS_CONTEXT_CHANGED');
  if (!Array.isArray(value.data) || value.data.length > 1000)
    throw new NativeProjectsError('NATIVE_PROJECTS_RESPONSE_INVALID');
  return value.data.map((row: unknown) => handoffView(row, projectId, actor.companyId));
}

function unavailable(): never {
  throw new ApiError(503, 'NATIVE_PROJECTS_UNAVAILABLE', 'The native Projects handoff reader is unavailable.');
}

function ownedFailure(error: unknown): never {
  const failure = nativeProjectsFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  if (error instanceof NativeProjectsError)
    throw new ApiError(503, 'NATIVE_PROJECTS_UNAVAILABLE', 'The native Projects reader is unavailable.');
  throw error;
}

/** Page-side entry: keeps the retained purchase preflight at the caller and
 * accepts only the native transport. There is no TypeScript SQL fallback. */
export async function ownedProjectHandoffs(
  actor: WorkContext,
  holdingSlug: string,
  projectId: string,
): Promise<NativeProjectHandoffView[]> {
  if (process.env.ERPX_NATIVE_PROJECTS_CONFIG === undefined) unavailable();
  try {
    const read = createNextNativeProjectHandoffsReader((incoming, slug, id, signal) =>
      nativeProjectHandoffs(incoming, nativeProjectsPageContext(actor), slug, id, signal),
    );
    const data = await read(projectId, holdingSlug);
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
