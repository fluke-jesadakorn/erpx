import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';

/** Strict owner transport for the native Collaboration social feed router. */
export type NativeSocialConfiguration = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
}>;

const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): NativeSocialConfiguration {
  const raw = process.env.ERPX_NATIVE_COLLABORATION_CONFIG;
  if (raw === undefined)
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  let value: NativeSocialConfiguration;
  try {
    value = JSON.parse(raw) as NativeSocialConfiguration;
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  }
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin ||
    !Number.isInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000
  )
    throw new ApiError(503, 'NATIVE_COLLABORATION_CONFIGURATION_INVALID', 'Collaboration transport is misconfigured.');
  return value;
}

export function nativeSocialAvailable(): boolean {
  return process.env.ERPX_NATIVE_COLLABORATION_CONFIG !== undefined;
}

function headersFor(request: Request, key?: string): Headers {
  const headers = new Headers({ accept: 'application/json' });
  const cookie = request.headers.get('cookie');
  const origin = request.headers.get('origin');
  if (cookie) headers.set('cookie', cookie);
  if (origin) headers.set('origin', origin);
  for (const name of [
    'x-erpx-context-company',
    'x-erpx-context-assignment',
    'x-erpx-context-revision',
    'x-erpx-context-assignment-version',
    'x-erpx-workspace-branch',
    'x-erpx-workspace-department',
  ]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  if (key) {
    headers.set('content-type', 'application/json');
    headers.set('idempotency-key', key);
  }
  return headers;
}

async function call(
  request: Request,
  holdingSlug: string,
  path: string,
  init?: { body: unknown; key: string },
): Promise<unknown> {
  const config = configuration();
  let response: Response;
  try {
    response = await fetch(
      `${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(holdingSlug)}${path}`,
      {
        method: init ? 'POST' : 'GET',
        headers: headersFor(request, init?.key),
        body: init ? JSON.stringify(init.body) : undefined,
        cache: 'no-store',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  const payload = (await response.json().catch(() => null)) as
    | { data?: unknown; error?: { code?: string; message?: string } }
    | null;
  if (!response.ok)
    throw new ApiError(
      response.status,
      payload?.error?.code ?? 'NATIVE_COLLABORATION_FAILED',
      payload?.error?.message ?? 'The Collaboration request failed.',
    );
  return payload?.data;
}

export function nativeFeed(request: Request, holdingSlug: string, filter: string) {
  return call(request, holdingSlug, `/feed?filter=${encodeURIComponent(filter)}`);
}

export function nativeCreatePost(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
) {
  return call(request, holdingSlug, '/feed', { body, key });
}

export function nativeSaveDraft(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
) {
  return call(request, holdingSlug, '/feed/drafts', { body, key });
}

export function nativePublishDraft(
  request: Request,
  holdingSlug: string,
  id: string,
  version: number,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/drafts/${encodeURIComponent(id)}/publish`,
    { body: { version }, key },
  );
}

export function nativePostAction(
  request: Request,
  holdingSlug: string,
  id: string,
  body: unknown,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(id)}/actions`,
    { body, key },
  );
}

export function nativeReportPost(
  request: Request,
  holdingSlug: string,
  id: string,
  reason: string,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(id)}/reports`,
    { body: { reason }, key },
  );
}

export function nativeCommentUploadCreate(
  request: Request,
  holdingSlug: string,
  postId: string,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(postId)}/comment-uploads`,
    { body: {}, key },
  );
}

export function nativeCommentUploadCancel(
  request: Request,
  holdingSlug: string,
  id: string,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/comment-uploads/${encodeURIComponent(id)}/cancel`,
    { body: {}, key },
  );
}

export function nativeEditUploadCreate(
  request: Request,
  holdingSlug: string,
  postId: string,
  expectedVersion: number,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(postId)}/edit-uploads`,
    { body: { expectedVersion }, key },
  );
}

export function nativeEditUploadCancel(
  request: Request,
  holdingSlug: string,
  id: string,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/edit-uploads/${encodeURIComponent(id)}/cancel`,
    { body: {}, key },
  );
}

export function nativeCreateComment(
  request: Request,
  holdingSlug: string,
  postId: string,
  body: unknown,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(postId)}/comments`,
    { body, key },
  );
}

export function nativeSetPostReaction(
  request: Request,
  holdingSlug: string,
  postId: string,
  active: boolean,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(postId)}/reactions`,
    { body: { active }, key },
  );
}

export function nativeSetPostFollow(
  request: Request,
  holdingSlug: string,
  postId: string,
  active: boolean,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/feed/${encodeURIComponent(postId)}/follows`,
    { body: { active }, key },
  );
}

export function nativePublishHandover(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
) {
  return call(request, holdingSlug, '/feed/handovers', { body, key });
}
