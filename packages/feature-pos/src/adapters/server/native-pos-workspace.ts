import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  NativePosCashError,
  nativePosCashConfiguration,
  resolveNativePosCashConfiguration,
} from './native-pos-cash';

export function nativePosWorkspaceConfigured(): boolean {
  return process.env.ERPX_NATIVE_POS_CASH_CONFIG !== undefined;
}

/** Bounded native POS workspace read. Returns `undefined` only when the native
 * transport is not configured; a configured failure never falls back. */
export async function nativePosWorkspace(
  actor: WorkContext,
  incoming: Pick<Headers, 'get'>,
  query: URLSearchParams,
  signal?: AbortSignal,
): Promise<unknown | undefined> {
  const raw = process.env.ERPX_NATIVE_POS_CASH_CONFIG;
  if (raw === undefined) return undefined;
  const config = resolveNativePosCashConfiguration(nativePosCashConfiguration(raw));
  const host = incoming.get('host');
  const target = host ? config.holdings.get(host) : undefined;
  if (
    !target ||
    target.holdingSlug !== actor.holdingSlug ||
    target.holdingId !== actor.holdingId
  )
    throw new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID');
  const origin = incoming.get('origin');
  if (origin && origin !== target.origin)
    throw new NativePosCashError('ORIGIN_FORBIDDEN', 403);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/pos/workspace`,
    config.base,
  );
  for (const key of ['sale', 'q', 'tab', 'limit']) {
    const value = query.get(key);
    if (value !== null) url.searchParams.set(key, value);
  }
  const headers = new Headers({
    host: host!,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-holding': actor.holdingId,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority?.revision ?? ''),
    'x-erpx-context-assignment-version': String(actor.authority?.assignmentVersion ?? ''),
  });
  if (origin) headers.set('origin', origin);
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      throw new NativePosCashError('NATIVE_POS_CASH_COOKIE_LIMIT');
    headers.set('cookie', cookie);
  }
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    headers.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department' && actor.scope.departmentSlug)
    headers.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener('abort', cancel, { once: true });
  if (signal?.aborted) controller.abort();
  const timer = setTimeout(cancel, config.timeout);
  const response = await directGet(url, headers, controller.signal).finally(() => {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancel);
  });
  if (response.status !== 200) {
    let code = 'NATIVE_POS_WORKSPACE_UNAVAILABLE';
    try {
      const body = (await response.json()) as { error?: { code?: unknown } };
      if (typeof body.error?.code === 'string') code = body.error.code;
    } catch {}
    throw new NativePosCashError(code, response.status);
  }
  const value = (await response.json()) as { data?: unknown; context?: unknown };
  if (
    !value ||
    typeof value !== 'object' ||
    typeof value.data !== 'object' ||
    value.data === null ||
    typeof value.context !== 'object' ||
    value.context === null
  )
    throw new NativePosCashError('NATIVE_POS_WORKSPACE_RESPONSE_INVALID');
  const context = value.context as Record<string, unknown>;
  if (
    context.userId !== actor.userId ||
    context.holdingId !== actor.holdingId ||
    context.companyId !== actor.companyId ||
    context.assignmentId !== actor.assignmentId ||
    context.revision !== actor.authority?.revision ||
    context.assignmentVersion !== actor.authority?.assignmentVersion
  )
    throw new NativePosCashError('NATIVE_POS_WORKSPACE_CONTEXT_CHANGED', 409);
  return value.data;
}

function directGet(
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
    const outgoing = request(
      url,
      { method: 'GET', agent, headers: { ...Object.fromEntries(headers) }, signal, maxHeaderSize: 8192 },
      (incoming) => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          const values = new Headers();
          for (let index = 0; index < incoming.rawHeaders.length; index += 2)
            values.append(incoming.rawHeaders[index]!, incoming.rawHeaders[index + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: status >= 200 && status <= 599 ? status : 502,
              headers: values,
            }),
          );
        } catch (error) {
          reject(error);
        }
      },
    );
    outgoing.on('error', reject);
    outgoing.end();
  });
}
