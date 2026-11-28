import 'server-only';
import { headers as requestHeaders } from 'next/headers';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { WorkItem } from '@erpx/work-projection-ui/lib/work-item';
export { supportPolicyCreateSchema, supportPolicyChangeSchema, supportClockAttachSchema, supportClockValue } from "@erpx/support-server/domain/support-sla";

type Settings = Readonly<{
  base: URL;
  timeoutMs: number;
  holdings: readonly Readonly<{ host: string; holdingSlug: string; holdingId: string }>[];
}>;
type Transport = (url: URL, outgoing: Headers, signal: AbortSignal) => Promise<Response>;
type SlaPolicyPoint = {
  id: string;
  company_id: string;
  name: string;
  policy_json: unknown;
  status: string;
  version: number;
  created_by: string;
  approved_by: string | null;
  approved_at: number | null;
  created_at: number;
  updated_at: number;
};

const unavailable = (): never => { throw new ApiError(503, 'NATIVE_SUPPORT_SLA_POLICY_READ_UNAVAILABLE', 'The native Support SLA policy reader is unavailable.'); };
const fail = (status: number, code: string, message: string): never => { throw new ApiError(status, code, message); };
const object = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= 200;
const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const queues = new Set(['todo', 'waiting', 'completed']);
const relations = new Set(['requested', 'assigned', 'approval', 'team']);

function settings(raw: string): Settings {
  let parsed: unknown;
  try { parsed = JSON.parse(raw); } catch { return unavailable(); }
  if (!object(parsed) || typeof parsed.privateBaseUrl !== 'string' || !Number.isSafeInteger(parsed.timeoutMs) ||
      Number(parsed.timeoutMs) < 250 || Number(parsed.timeoutMs) > 30000 ||
      !Array.isArray(parsed.holdings) || parsed.holdings.length < 1 || parsed.holdings.length > 64) return unavailable();
  let base: URL;
  try { base = new URL(parsed.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
      reserved.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
      base.origin !== parsed.privateBaseUrl) return unavailable();
  const holdings = parsed.holdings.map((item: unknown) => {
    if (!object(item) || typeof item.origin !== 'string' || !text(item.holdingSlug) ||
        !/^[a-z0-9_-]+$/.test(item.holdingSlug) || !text(item.holdingId)) return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || origin.origin !== item.origin || origin.username || origin.password) return unavailable();
    return { host: origin.host, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  return { base, timeoutMs: Number(parsed.timeoutMs), holdings };
}

const direct: Transport = (url, outgoing, signal) => fetch(url, {
  method: 'GET', headers: outgoing, cache: 'no-store', redirect: 'error', signal,
});

const workItem = (value: unknown): value is WorkItem => {
  if (!object(value)) return false;
  if (!text(value.id) || value.type !== 'support_policy' || !text(value.title) ||
      typeof value.detail !== 'string' || typeof value.href !== 'string' || typeof value.status !== 'string') return false;
  if (typeof value.updatedAt !== 'number' || !Number.isSafeInteger(value.updatedAt)) return false;
  if (typeof value.queue !== 'string' || !queues.has(value.queue)) return false;
  if (typeof value.canAct !== 'boolean' || typeof value.responsibleQueue !== 'string' || typeof value.nextAction !== 'string') return false;
  if (!(value.blocker === null || typeof value.blocker === 'string')) return false;
  return Array.isArray(value.relationships) && value.relationships.every(
    (relationship: unknown) => typeof relationship === 'string' && relations.has(relationship),
  );
};

/**
 * Fail-closed transport for the mounted native Support SLA policy reads. The
 * exact Holding target comes from configuration and the caller's work context;
 * no caller SQL, payload shaping or TypeScript read fallback remains.
 */
export function createSlaPolicyReader(raw: string, transport: Transport = direct) {
  const config = settings(raw);
  const read = async (actor: WorkContext, incoming: Pick<Headers, 'get'>, path: string): Promise<unknown> => {
    const host = incoming.get('host');
    const target = config.holdings.find((holding) =>
      holding.host === host && holding.holdingSlug === actor.holdingSlug && holding.holdingId === actor.holdingId);
    if (!target) return fail(409, 'NATIVE_SUPPORT_SLA_POLICY_AUTHORITY_INVALID', 'The native Support SLA policy target does not match this work context.');
    const outgoing = new Headers({ host: host!, accept: 'application/json', 'cache-control': 'no-cache' });
    for (const name of ['cookie', 'x-erpx-context-company', 'x-erpx-context-assignment', 'x-erpx-context-revision',
      'x-erpx-context-assignment-version', 'x-erpx-workspace-branch', 'x-erpx-workspace-department', 'origin']) {
      const value = incoming.get(name);
      if (value) outgoing.set(name, value);
    }
    if (!outgoing.has('x-erpx-context-company')) outgoing.set('x-erpx-context-company', actor.companyId);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    let response: Response;
    try {
      response = await transport(
        new URL(`/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/support/sla/policies${path}`, config.base),
        outgoing, controller.signal,
      );
    } catch { return unavailable(); } finally { clearTimeout(timer); }
    if (response.redirected || response.status !== 200) {
      const payload = await response.json().catch((): unknown => null);
      if (!(response.status >= 400 && response.status <= 599)) void response.body?.cancel().catch(() => undefined);
      const detail = object(payload) && object(payload.error) ? payload.error : null;
      const code = detail && typeof detail.code === 'string' ? detail.code : 'NATIVE_SUPPORT_SLA_POLICY_READ_UNAVAILABLE';
      const message = detail && typeof detail.message === 'string' ? detail.message : 'The native Support SLA policy reader is unavailable.';
      return fail(response.status >= 400 && response.status <= 599 ? response.status : 503, code, message);
    }
    const payload = await response.json().catch((): unknown => null);
    if (!object(payload) || !('data' in payload)) return unavailable();
    return payload.data;
  };
  return {
    point: async (actor: WorkContext, incoming: Pick<Headers, 'get'>, id: string): Promise<SlaPolicyPoint | null> => {
      if (!text(id) || id === 'work') return fail(422, 'SUPPORT_POLICY_ID_INVALID', 'Choose a policy.');
      const row = await read(actor, incoming, `/${encodeURIComponent(id)}`);
      if (!object(row) || !text(row.id) || !text(row.company_id) || !text(row.name) || !text(row.status) ||
          !text(row.created_by) || typeof row.version !== 'number' || !Number.isSafeInteger(row.version) ||
          typeof row.created_at !== 'number' || typeof row.updated_at !== 'number' || !('policy_json' in row) ||
          !(row.approved_by === null || typeof row.approved_by === 'string') ||
          !(row.approved_at === null || typeof row.approved_at === 'number')) return unavailable();
      return row as unknown as SlaPolicyPoint;
    },
    work: async (actor: WorkContext, incoming: Pick<Headers, 'get'>): Promise<WorkItem[]> => {
      const rows = await read(actor, incoming, '/work');
      if (!Array.isArray(rows) || !rows.every(workItem)) return unavailable();
      return rows as WorkItem[];
    },
  };
}

const nativeFailure = (error: unknown): Error => error instanceof ApiError ? error : unavailable();

export async function getSupportPolicy(a: WorkContext, id: string): Promise<SlaPolicyPoint | null> {
  const raw = process.env.ERPX_NATIVE_SUPPORT_READ_CONFIG;
  if (raw === undefined) return unavailable();
  try {
    return await createSlaPolicyReader(raw).point(a, await requestHeaders(), id);
  } catch (error) { throw nativeFailure(error); }
}

export async function supportPolicyWork(a: WorkContext): Promise<WorkItem[]> {
  const raw = process.env.ERPX_NATIVE_SUPPORT_READ_CONFIG;
  if (raw === undefined) return unavailable();
  try {
    return await createSlaPolicyReader(raw).work(a, await requestHeaders());
  } catch (error) { throw nativeFailure(error); }
}
