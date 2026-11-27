import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { AccountAccessContext, WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { HomePost, SocialPageData } from './native-social-page-types';

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
  const host = request.headers.get('host');
  const cookie = request.headers.get('cookie');
  const origin = request.headers.get('origin');
  if (host) headers.set('host', host);
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

export function nativeCreateStory(
  request: Request,
  holdingSlug: string,
  body: unknown,
  key: string,
) {
  return call(request, holdingSlug, '/stories', { body, key });
}

export function nativeArchiveStory(
  request: Request,
  holdingSlug: string,
  id: string,
  expectedVersion: number,
  key: string,
) {
  return call(
    request,
    holdingSlug,
    `/stories/${encodeURIComponent(id)}/archive`,
    { body: { expectedVersion }, key },
  );
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

/** Paged comments with the retained ancestors the conversation client renders. */
/** Author-only revision history of one readable post. */
export function nativePostRevisions(request: Request, holdingSlug: string, id: string) {
  return call(request, holdingSlug, `/feed/${encodeURIComponent(id)}/revisions`);
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

type SocialPageActor = Pick<WorkContext, 'holdingSlug' | 'companyId' | 'assignmentId' | 'authority' | 'scope'> &
  Partial<Pick<WorkContext, 'branchId' | 'departmentId' | 'userId' | 'role' | 'iam'>>;

/** Carry the workspace rendered by Next to the owner for current authorization. */
function ssrHeaders(actor: SocialPageActor, incoming: Pick<Headers, 'get'>): Headers {
  const host = incoming.get('host');
  if (!host || !actor.authority)
    throw new ApiError(403, 'NATIVE_COLLABORATION_CONTEXT_CHANGED', 'The selected Collaboration workspace is unavailable.');
  const outgoing = new Headers({
    host,
    accept: 'application/json',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
    // Match the presentation token used by existing Social controls. Rust
    // independently validates the selected assignment and its revision.
    'x-erpx-social-page-context': createHash('sha256').update(JSON.stringify([
      actor.companyId, actor.branchId, actor.departmentId, actor.userId,
      actor.assignmentId, actor.role, actor.authority, actor.iam ?? null,
    ])).digest('hex'),
  });
  const cookie = incoming.get('cookie');
  if (cookie) outgoing.set('cookie', cookie);
  if (actor.scope.kind !== 'holding' && actor.scope.branchSlug)
    outgoing.set('x-erpx-workspace-branch', actor.scope.branchSlug);
  if (actor.scope.kind === 'department')
    outgoing.set('x-erpx-workspace-department', actor.scope.departmentSlug);
  return outgoing;
}

const failed = (): never => {
  throw new ApiError(
    503,
    'NATIVE_COLLABORATION_FAILED',
    'The Collaboration request failed.',
  );
};

const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);

/** Strict full-page validation. A malformed reply throws instead of degrading
 *  into a partial native payload merged with retained TypeScript state. */
function asPage(value: unknown): SocialPageData {
  if (!object(value)) return failed();
  const { posts, milestones, options, moderate, sourceOnly, presentation, post } = value;
  if (
    !Array.isArray(posts) ||
    !Array.isArray(milestones) ||
    !object(options) ||
    !Array.isArray(options.branches) ||
    !Array.isArray(options.audiences) ||
    !Array.isArray(options.projects) ||
    typeof moderate !== 'boolean' ||
    (sourceOnly !== undefined && typeof sourceOnly !== 'boolean')
  )
    return failed();
  for (const row of [...posts, ...milestones])
    if (!object(row) || typeof row.id !== 'string') return failed();
  if (
    post !== null &&
    post !== undefined &&
    (!object(post) || typeof post.id !== 'string')
  )
    return failed();
  if (presentation !== null && presentation !== undefined) {
    if (!object(presentation) || typeof presentation.context !== 'string')
      return failed();
    if (
      presentation.purchase !== undefined &&
      presentation.purchase !== null &&
      !object(presentation.purchase)
    )
      return failed();
  }
  return {
    posts: posts as HomePost[],
    milestones: milestones as HomePost[],
    options: options as SocialPageData['options'],
    moderate,
    sourceOnly: sourceOnly === true,
    presentation: (presentation ?? undefined) as SocialPageData['presentation'],
    initialDay: value.initialDay,
    post: (post ?? null) as HomePost | null,
  };
}

/** Aggregate SSR page read for server components without a Request object.
 *  Every field the home/document presenters read comes from this one owner
 *  reply; there is no field-by-field native-over-TypeScript overlay. */
export async function nativeSocialPageDataSSR(
  actor: SocialPageActor,
  input: {
    postId?: string;
    tag?: string;
    skipDay?: boolean;
    skipMilestones?: boolean;
    sourceOnly?: boolean;
  },
  incoming: Pick<Headers, 'get'>,
): Promise<SocialPageData> {
  const config = configuration();
  const query = new URLSearchParams();
  if (input.postId) query.set('postId', input.postId);
  if (input.tag) query.set('tag', input.tag);
  if (input.skipDay) query.set('skipDay', '1');
  if (input.skipMilestones) query.set('skipMilestones', '1');
  if (input.sourceOnly) query.set('sourceOnly', '1');
  const search = query.toString();
  const requestHeaders = ssrHeaders(actor, incoming);
  let response: Response;
  try {
    response = await nativeRequest(
      new URL(`${config.privateBaseUrl}/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/page${search ? `?${search}` : ''}`),
      { headers: Object.fromEntries(requestHeaders) },
      config.timeoutMs,
    );
  } catch {
    throw new ApiError(
      503,
      'NATIVE_COLLABORATION_UNAVAILABLE',
      'The native Collaboration service is unavailable.',
    );
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
  return asPage(payload?.data);
}


/** Read source eligibility before the Account has selected a working role. */
export async function nativeSocialSourceEntry(
  { scope, requestHeaders }: Pick<AccountAccessContext, 'scope' | 'requestHeaders'>,
  assignment: { id: string; version?: number },
  postId: string,
): Promise<{ purchasePurpose: 'history' } | null> {
  const version = assignment.version;
  if (version === undefined || !Number.isSafeInteger(version) || version < 1) return null;
  const host = requestHeaders.get('host');
  if (!host) throw new ApiError(404, 'NOT_FOUND', 'Not found.');
  const { privateBaseUrl, timeoutMs } = configuration();
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(scope.holdingSlug)}/posts/${encodeURIComponent(postId)}/source-entry`, privateBaseUrl);
  url.search = new URLSearchParams({ companyId: scope.companyId, assignmentId: assignment.id, assignmentVersion: String(version) }).toString();
  const headers: Record<string, string> = { host, accept: 'application/json' };
  const cookie = requestHeaders.get('cookie');
  if (cookie) headers.cookie = cookie;
  if (scope.kind !== 'holding' && scope.branchSlug) headers['x-erpx-workspace-branch'] = scope.branchSlug;
  if (scope.kind === 'department') headers['x-erpx-workspace-department'] = scope.departmentSlug;
  let response: Response;
  try {
    response = await nativeRequest(url, { headers }, timeoutMs);
  } catch {
    throw new ApiError(503, 'NATIVE_COLLABORATION_UNAVAILABLE', 'The native Collaboration service is unavailable.');
  }
  const payload = await response.json().catch(() => null) as {
    data?: { eligible?: unknown }; error?: { code?: string; message?: string };
  } | null;
  if (!response.ok) throw new ApiError(
    response.status,
    payload?.error?.code ?? 'NATIVE_COLLABORATION_FAILED',
    payload?.error?.message ?? 'The Collaboration request failed.',
  );
  if (typeof payload?.data?.eligible !== 'boolean') return failed();
  return payload.data.eligible ? { purchasePurpose: 'history' } : null;
}
