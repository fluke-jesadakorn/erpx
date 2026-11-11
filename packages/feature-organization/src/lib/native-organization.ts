import 'server-only';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { OrganizationAccessData } from './organization-access';
import type { OrganizationOverview } from './organization-domain';
import type {
  PositionMoveInput,
  PositionMovePreview,
  PositionMoveRecord,
} from '@erpx/feature-people_payroll/lib/position-moves';
import {
  createNativeOrganizationClient,
  nativeOrganizationAccess,
  nativeOrganizationConfiguration,
  nativeOrganizationDetail,
  nativeOrganizationFailure,
  nativeOrganizationOverview,
  NativeOrganizationError,
  type NativeOrganizationPageContext,
  type NativeOrganizationWorkspace,
  type NativePositionMoveReceipt,
} from '../adapters/server/native-organization';

type Row = Record<string, unknown>;

/** Native Organization facts plus the retained People projection for one
 * People/Organization page render. */
export type NativeOrganizationPage = Readonly<{
  access: OrganizationAccessData;
  overview: OrganizationOverview;
}>;

function context(actor: WorkContext): NativeOrganizationPageContext {
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    scope:
      actor.scope.kind === 'holding'
        ? { kind: 'holding' }
        : actor.scope.kind === 'branch'
          ? { kind: 'branch', branchSlug: actor.scope.branchSlug }
          : { kind: 'department', branchSlug: actor.scope.branchSlug, departmentSlug: actor.scope.departmentSlug },
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

function client(actor: WorkContext) {
  const raw = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor.authority || !actor.iam)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED');
  return createNativeOrganizationClient(nativeOrganizationConfiguration(raw));
}

/** Exact native selector for the node lifecycle and organization setup
 * commands already used by the People organization route. The workspace read
 * is selected separately by `nativeOrganizationWorkspace`; other organization
 * writes stay with the retained owner until their native command surface is
 * available. */
export function nativeOrganizationSelector(parts: readonly string[]):
  | { kind: 'save'; id: string | null; submit: boolean }
  | { kind: 'preview' }
  | { kind: 'review'; id: string }
  | { kind: 'cancel'; id: string }
  | { kind: 'levels' }
  | { kind: 'templateSave'; id: string | null }
  | { kind: 'templateApply'; id: string }
  | undefined {
  if (parts[0] !== 'organization') return undefined;
  if (parts.length === 2 && parts[1] === 'levels') return { kind: 'levels' };
  if (parts[1] === 'templates') {
    if (parts.length === 2) return { kind: 'templateSave', id: null };
    if (parts.length === 3 && parts[2]) return { kind: 'templateSave', id: parts[2] };
    if (parts.length === 4 && parts[2] && parts[3] === 'apply')
      return { kind: 'templateApply', id: parts[2] };
    return undefined;
  }
  if (parts[1] !== 'changes') return undefined;
  if (parts.length === 3 && parts[2] === 'preview') return { kind: 'preview' };
  if (parts.length === 4 && parts[2] && parts[3] === 'review' && parts[2] !== 'preview')
    return { kind: 'review', id: parts[2] };
  if (parts.length === 4 && parts[2] && parts[3] === 'cancel') return { kind: 'cancel', id: parts[2] };
  if (parts.length === 3 && (parts[2] === 'draft' || parts[2] === 'submit'))
    return { kind: 'save', id: null, submit: parts[2] === 'submit' };
  if (
    parts.length === 4 &&
    parts[2] &&
    (parts[3] === 'draft' || parts[3] === 'submit') &&
    parts[2] !== 'preview'
  )
    return { kind: 'save', id: parts[2], submit: parts[3] === 'submit' };
  return undefined;
}

/** Explicit opt-in. A missing configuration keeps the retained owner; an
 * invalid configured request fails closed as 503 instead of silently writing
 * through the replaced TypeScript owner. */
export async function nativeOrganizationChange(
  actor: WorkContext,
  holdingSlug: string,
  parts: readonly string[],
  body: unknown,
  idempotencyKey: string,
  incoming: Pick<Headers, 'get'>,
): Promise<Row | undefined> {
  const selector = nativeOrganizationSelector(parts);
  if (!selector) return undefined;
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED');
  if (selector.kind === 'preview') {
    return await native.change(incoming, context(actor), {
      path: '/changes/preview',
      body,
      idempotencyKey: 'preview-command-key',
      preview: true,
    });
  }
  if (selector.kind === 'levels') {
    return await native.change(incoming, context(actor), {
      path: '/levels',
      body: {},
      idempotencyKey,
      result: 'levels',
    });
  }
  if (selector.kind === 'templateSave') {
    return await native.change(incoming, context(actor), {
      path:
        selector.id === null
          ? '/templates'
          : `/templates/${encodeURIComponent(selector.id)}`,
      body,
      idempotencyKey,
      result: 'template',
    });
  }
  if (selector.kind === 'templateApply') {
    return await native.change(incoming, context(actor), {
      path: `/templates/${encodeURIComponent(selector.id)}/apply`,
      body,
      idempotencyKey,
      result: 'apply',
    });
  }
  if (selector.kind === 'review') {
    return await native.change(incoming, context(actor), {
      path: `/changes/${encodeURIComponent(selector.id)}/review`,
      body,
      idempotencyKey,
    });
  }
  if (selector.kind === 'cancel') {
    return await native.change(incoming, context(actor), {
      path: `/changes/${encodeURIComponent(selector.id)}/cancel`,
      body,
      idempotencyKey,
    });
  }
  return await native.change(incoming, context(actor), {
    path: '/changes',
    body: { ...(body as Row), id: selector.id, submit: selector.submit },
    idempotencyKey,
  });
}

/** Native node/tree page read. The Organization owner serves nodes, positions,
 * levels, templates, changes, channels and command flags. A missing transport
 * configuration keeps the retained page read; a configured failure is mapped by
 * the route instead of silently falling back. */
export async function nativeOrganizationWorkspace(
  actor: WorkContext,
  holdingSlug: string,
  query: Record<string, string | undefined>,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeOrganizationWorkspace | undefined> {
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED');
  const search = new URLSearchParams(
    Object.entries(query).filter((entry): entry is [string, string] => entry[1] !== undefined),
  );
  return await native.workspace(incoming, context(actor), search.toString());
}

/** Native access projection read. A missing configuration returns `undefined`
 * so consumers keep the retained owner read; a configured failure throws
 * instead of silently returning retained data. */
export async function nativeOrganizationAccessRead(
  actor: WorkContext,
  holdingSlug: string,
  incoming: Pick<Headers, 'get'>,
): Promise<OrganizationAccessData | undefined> {
  // The native access ingress is currently gated by `people.manage` while the
  // retained projection also serves `iam.read` / `organization.manage`. Those
  // narrower-JIT readers keep the retained read rather than receiving a 403.
  if (!hasPermission(actor, 'people.manage')) return undefined;
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED');
  const data = await native.access(incoming, context(actor));
  if (
    data.actorId !== actor.userId ||
    data.people.some((person) => person.company_id !== actor.companyId) ||
    data.positions.some((position) => position.company_id !== actor.companyId)
  )
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return data;
}

/** Compose the native access projection over the retained read. The retained
 * callback is only invoked when the native transport is not configured, so a
 * configured failure is never masked by a fallback. */
export async function nativeOrganizationAccessPage(
  actor: WorkContext,
  holdingSlug: string,
  incoming: Pick<Headers, 'get'>,
  retained: () => Promise<OrganizationAccessData>,
  read: typeof nativeOrganizationAccessRead = nativeOrganizationAccessRead,
): Promise<OrganizationAccessData> {
  const native = await read(actor, holdingSlug, incoming);
  return native ?? retained();
}

export type NativePositionMoveSelector =
  | { kind: 'list'; id?: string }
  | { kind: 'preview' }
  | { kind: 'submit' }
  | { kind: 'cancel'; id: string };

/** Exact People catch-all selector for the native position-move surface. */
export function nativePositionMoveSelector(
  parts: readonly string[],
): NativePositionMoveSelector | undefined {
  if (parts[0] !== 'organization' || parts[1] !== 'position-moves') return undefined;
  if (parts.length === 2) return { kind: 'list' };
  if (parts.length === 3 && parts[2] === 'preview') return { kind: 'preview' };
  if (parts.length === 3 && parts[2] === 'submit') return { kind: 'submit' };
  if (parts.length === 3 && parts[2]) return { kind: 'list', id: parts[2] };
  if (parts.length === 4 && parts[2] && parts[3] === 'cancel')
    return { kind: 'cancel', id: parts[2] };
  return undefined;
}

/** Native list read. A missing configuration returns `undefined` so the
 * People route keeps its retained owner branch; a configured failure throws. */
export async function nativeOrganizationPositionMoveList(
  actor: WorkContext,
  holdingSlug: string,
  id: string | undefined,
  incoming: Pick<Headers, 'get'>,
): Promise<PositionMoveRecord[] | PositionMoveRecord | undefined> {
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED');
  const result = await native.positionMoves(incoming, context(actor), id);
  const rows = Array.isArray(result) ? result : [result];
  if (
    rows.some((row) => row.company_id !== actor.companyId) ||
    (id !== undefined && rows[0]?.id !== id)
  )
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_RESPONSE_INVALID');
  return result;
}

/** Native preview/submit/cancel command. Only the exact People catch-all
 * position-move selectors are handled; every other organization command stays
 * with its existing owner until a native replacement exists. */
export async function nativeOrganizationPositionMoveCommand(
  actor: WorkContext,
  holdingSlug: string,
  parts: readonly string[],
  body: unknown,
  idempotencyKey: string,
  incoming: Pick<Headers, 'get'>,
  origin?: string,
): Promise<PositionMovePreview | NativePositionMoveReceipt | undefined> {
  const selector = nativePositionMoveSelector(parts);
  if (!selector || selector.kind === 'list') return undefined;
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED');
  const page = context(actor);
  if (selector.kind === 'preview')
    return await native.previewPositionMove(incoming, page, body as PositionMoveInput);
  if (selector.kind === 'submit')
    return await native.submitPositionMove(incoming, page, { body, idempotencyKey, origin });
  const expectedVersion =
    body !== null && typeof body === 'object'
      ? (body as Record<string, unknown>).expectedVersion
      : undefined;
  if (typeof expectedVersion !== 'number' || !Number.isSafeInteger(expectedVersion) || expectedVersion < 1)
    throw new NativeOrganizationError('NATIVE_ORGANIZATION_REQUEST_INVALID');
  return await native.cancelPositionMove(incoming, page, {
    id: selector.id,
    expectedVersion,
    idempotencyKey,
    origin,
  });
}

/** Compose one page read from the native Organization workspace and the
 * retained People projection. A missing configuration returns `undefined` so
 * the page keeps its retained Organization read; a configured failure throws
 * instead of silently returning retained data. */
export async function nativeOrganizationPage(
  actor: WorkContext,
  holdingSlug: string,
  query: Record<string, string | undefined>,
  incoming: Pick<Headers, 'get'>,
  retained: OrganizationAccessData,
  read: typeof nativeOrganizationWorkspace = nativeOrganizationWorkspace,
): Promise<NativeOrganizationPage | undefined> {
  const native = await read(actor, holdingSlug, query, incoming);
  if (!native) return undefined;
  const access = nativeOrganizationAccess(native, retained, {
    userId: actor.userId,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
  });
  const positionIds = new Set(access.positions.map((position) => position.id));
  const nodeIds = new Set(access.nodes.map((node) => node.id));
  const scoped = {
    ...native,
    positions: native.positions.filter((position) => positionIds.has(position.id)),
    nodes: native.nodes.filter((node) => nodeIds.has(String(node.id))),
  };
  return {
    access,
    overview: {
      ...nativeOrganizationOverview(scoped, { people: retained.people }),
      canManage: access.canManage,
    },
  };
}

export {
  nativeOrganizationAccess,
  nativeOrganizationDetail,
  nativeOrganizationFailure,
  nativeOrganizationOverview,
};
