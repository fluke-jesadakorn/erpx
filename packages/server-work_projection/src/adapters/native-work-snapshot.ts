import 'server-only';
import {Agent, request as nodeRequest} from 'node:http';
import {Readable} from 'node:stream';
import {originProfile} from '@erpx/feature-runtime/lib/holding-hosts';
import {
  nativeHoldingConfiguration,
  nativeHoldingTargetById,
  nativeHoldingTargetForHost,
  type NativeHoldingConfiguration,
} from '@erpx/native-sdk/holding-target';
import {ApiError} from '@erpx/server-runtime/public/http';

/**
 * Strict private-loopback client for the native Work snapshot read.
 *
 * It forwards only the captured cookie and selected-work context headers. The
 * native route re-verifies the session and authority; this client grants
 * nothing. Every transport, configuration and response failure is a named
 * error, never a silent empty snapshot.
 */

export type NativeWorkSnapshotQueue = 'todo' | 'waiting' | 'completed';

export type NativeWorkSnapshotItem = {
  id: string;
  type: string;
  queue: NativeWorkSnapshotQueue;
  canAct: boolean | null;
  actionKind: string | null;
  relationships: string[];
  updatedAt: number;
  sourceId: string | null;
  href: string;
};

export type NativeWorkSnapshotAvailability = {
  items: NativeWorkSnapshotItem[];
  unavailableSources: string[];
};

type Actor = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId?: string | null;
  departmentId?: string | null;
  assignmentId: string;
  authority?: {revision: number; assignmentVersion: number};
  scope?: {kind: 'holding' | 'branch' | 'department'; branchSlug?: string; departmentSlug?: string};
};

export type NativeWorkSnapshotTransport = (
  url: URL,
  headers: Headers,
  signal: AbortSignal,
) => Promise<Response>;

const CONFIG_ENV = 'ERPX_NATIVE_WORK_PROJECTION_CONFIG';
const RESPONSE_LIMIT = 2 * 1024 * 1024;
const ITEM_LIMIT = 10_000;
const ALLOWED_ITEM_KEYS = [
  'actionKind',
  'canAct',
  'href',
  'id',
  'queue',
  'relationships',
  'sourceId',
  'type',
  'updatedAt',
] as const;

const object = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === 'object' && !Array.isArray(v);
const integer = (v: unknown, min = 0): v is number =>
  typeof v === 'number' && Number.isSafeInteger(v) && v >= min;
const text = (v: unknown, max = 8192): v is string =>
  typeof v === 'string' && new TextEncoder().encode(v).length <= max;
const nullableText = (v: unknown, max = 8192) => v === null || text(v, max);
const identifier = (v: unknown): v is string =>
  text(v, 200) && !/[\u0000-\u001f\u007f/\\]/.test(v);
const fail = (code: string, status: number, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = () =>
  fail('NATIVE_WORK_SNAPSHOT_UNAVAILABLE', 503, 'The native Work snapshot is unavailable.');

function configuration(raw: string): NativeHoldingConfiguration {
  try {
    return nativeHoldingConfiguration(raw, originProfile());
  } catch {
    return fail(
      'NATIVE_WORK_SNAPSHOT_CONFIGURATION_INVALID',
      503,
      'The native Work snapshot is not configured.',
    );
  }
}

function target(config: NativeHoldingConfiguration, actor: Actor, incoming: Pick<Headers, 'get'>) {
  const sourceHost = incoming.get('host');
  const loopback =
    originProfile() === 'loopback' && sourceHost !== null && /^127\.0\.0\.1:\d+$/.test(sourceHost);
  const selected = loopback
    ? (config.holdings.find((h) => h.holdingSlug === actor.holdingSlug) ??
      nativeHoldingTargetById(config, actor.holdingId))
    : (nativeHoldingTargetForHost(config, sourceHost) ??
      nativeHoldingTargetById(config, actor.holdingId));
  if (
    !selected ||
    selected.holdingId !== actor.holdingId ||
    selected.holdingSlug !== actor.holdingSlug
  )
    return fail(
      'NATIVE_WORK_SNAPSHOT_TARGET_UNAVAILABLE',
      503,
      'The native Work snapshot target is unavailable.',
    );
  return selected;
}

function requestHeaders(
  incoming: Pick<Headers, 'get'>,
  selected: {origin: string},
  actor: Actor,
) {
  const headers = new Headers({
    host: new URL(selected.origin).host,
    origin: selected.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion),
  });
  const branch = actor.scope?.kind === 'holding' ? null : actor.scope?.branchSlug;
  if (branch) headers.set('x-erpx-workspace-branch', branch);
  if (actor.scope?.kind === 'department' && actor.scope.departmentSlug)
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return fail(
        'NATIVE_WORK_SNAPSHOT_COOKIE_LIMIT',
        503,
        'The native Work snapshot request is unavailable.',
      );
    headers.set('cookie', cookie);
  }
  return headers;
}

function validateContext(value: unknown, actor: Actor) {
  if (!object(value)) return fail('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID', 502, 'The native Work snapshot response did not match its contract.');
  if (
    value.companyId !== actor.companyId ||
    value.assignmentId !== actor.assignmentId ||
    value.revision !== actor.authority?.revision ||
    (value.userId !== undefined && value.userId !== actor.userId) ||
    (value.holdingId !== undefined && value.holdingId !== actor.holdingId)
  )
    return fail('WORK_CONTEXT_CHANGED', 409, 'The current work context changed.');
}

function validateItem(value: unknown): NativeWorkSnapshotItem {
  if (!object(value)) return fail('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID', 502, 'The native Work snapshot response did not match its contract.');
  if (
    Object.keys(value).length !== ALLOWED_ITEM_KEYS.length ||
    Object.keys(value).some((key) => !(ALLOWED_ITEM_KEYS as readonly string[]).includes(key)) ||
    !identifier(value.id) ||
    !text(value.type, 128) ||
    !['todo', 'waiting', 'completed'].includes(String(value.queue)) ||
    !(value.canAct === null || typeof value.canAct === 'boolean') ||
    !nullableText(value.actionKind, 128) ||
    !Array.isArray(value.relationships) ||
    value.relationships.length > 8 ||
    !value.relationships.every((relationship) => text(relationship, 64)) ||
    !integer(value.updatedAt) ||
    !nullableText(value.sourceId, 200) ||
    !text(value.href, 2048)
  )
    return fail('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID', 502, 'The native Work snapshot response did not match its contract.');
  return value as NativeWorkSnapshotItem;
}

function validate(value: unknown, actor: Actor): NativeWorkSnapshotAvailability {
  if (!object(value) || !object(value.context)) return fail('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID', 502, 'The native Work snapshot response did not match its contract.');
  validateContext(value.context, actor);
  const data = value.data;
  if (
    !object(data) ||
    !Array.isArray(data.items) ||
    data.items.length > ITEM_LIMIT ||
    !Array.isArray(data.unavailableSources) ||
    data.unavailableSources.length > 100 ||
    !data.unavailableSources.every((source) => text(source, 200))
  )
    return fail('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID', 502, 'The native Work snapshot response did not match its contract.');
  return {
    items: data.items.map(validateItem),
    unavailableSources: data.unavailableSources as string[],
  };
}

function direct(
  url: URL,
  headers: Headers,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const req = nodeRequest(
      url,
      {
        method: 'GET',
        headers: Object.fromEntries(headers),
        signal,
        agent,
        maxHeaderSize: 8192,
      },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const output = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            output.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
              headers: output,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    req.once('error', (error) => {
      agent.destroy();
      reject(error);
    });
    req.end();
  });
}

async function boundedJson(response: Response, signal: AbortSignal): Promise<unknown> {
  if (
    response.redirected ||
    response.status !== 200 ||
    response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' ||
    !response.body
  ) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const cancel = () => {
    void reader.cancel().catch(() => undefined);
  };
  signal.addEventListener('abort', cancel, {once: true});
  try {
    for (;;) {
      const part = await reader.read();
      if (signal.aborted) return unavailable();
      if (part.done) break;
      size += part.value.length;
      if (size > RESPONSE_LIMIT) {
        await reader.cancel();
        return fail(
          'NATIVE_WORK_SNAPSHOT_RESPONSE_LIMIT',
          502,
          'The native Work snapshot response was too large.',
        );
      }
      chunks.push(part.value);
    }
  } finally {
    signal.removeEventListener('abort', cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(bytes));
  } catch {
    return fail('NATIVE_WORK_SNAPSHOT_RESPONSE_INVALID', 502, 'The native Work snapshot response did not match its contract.');
  }
}

/** Builds a reader against an explicit private configuration. */
export function createNativeWorkSnapshotReader(
  raw: string,
  transport: NativeWorkSnapshotTransport = direct,
) {
  const config = configuration(raw);
  return async (
    incoming: Pick<Headers, 'get'>,
    actor: Actor,
  ): Promise<NativeWorkSnapshotAvailability> => {
    if (
      !identifier(actor.holdingId) ||
      !identifier(actor.companyId) ||
      !identifier(actor.assignmentId) ||
      !actor.authority ||
      !integer(actor.authority.revision) ||
      !integer(actor.authority.assignmentVersion, 1)
    )
      return fail(
        'WORK_CONTEXT_CHANGED',
        409,
        'Your work role changed. Reload and review before retrying.',
      );
    const selected = target(config, actor, incoming);
    const headers = requestHeaders(incoming, selected, actor);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      let reply: Response;
      try {
        reply = await transport(
          new URL(
            `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/work/native-snapshot`,
            config.privateBaseUrl,
          ),
          headers,
          controller.signal,
        );
      } catch {
        return unavailable();
      }
      if (reply.status !== 200) {
        const body = await reply.json().catch(() => null);
        if (object(body) && object(body.error)) {
          const code =
            typeof body.error.code === 'string' && /^[A-Z0-9_]{3,100}$/.test(body.error.code)
              ? body.error.code
              : 'NATIVE_WORK_SNAPSHOT_UNAVAILABLE';
          const message =
            typeof body.error.message === 'string' && body.error.message.length <= 300
              ? body.error.message
              : 'The native Work snapshot is unavailable.';
          return fail(
            code,
            reply.status >= 400 && reply.status <= 599 ? reply.status : 503,
            message,
          );
        }
        return unavailable();
      }
      return validate(await boundedJson(reply, controller.signal), actor);
    } finally {
      clearTimeout(timer);
    }
  };
}

/** True when the native Work snapshot consumer switch is configured. */
export function nativeWorkSnapshotConfigured(): boolean {
  return process.env[CONFIG_ENV] !== undefined;
}

/**
 * Reads the native availability snapshot through the private loopback route.
 * An absent configuration is a programming error here: callers must check
 * `nativeWorkSnapshotConfigured()` and keep the retained TS path when unset.
 */
export async function readNativeWorkSnapshot(
  incoming: Pick<Headers, 'get'>,
  actor: Actor,
): Promise<NativeWorkSnapshotAvailability> {
  const raw = process.env[CONFIG_ENV];
  if (raw === undefined)
    return fail(
      'NATIVE_WORK_SNAPSHOT_CONFIGURATION_REQUIRED',
      503,
      'The native Work snapshot is not configured.',
    );
  return createNativeWorkSnapshotReader(raw)(incoming, actor);
}
