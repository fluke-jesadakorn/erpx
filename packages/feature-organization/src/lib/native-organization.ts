import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativeOrganizationClient,
  nativeOrganizationConfiguration,
  NativeOrganizationError,
  type NativeOrganizationPageContext,
  type NativeOrganizationWorkspace,
} from '../adapters/server/native-organization';

type Row = Record<string, unknown>;

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

export {nativeOrganizationFailure, nativeOrganizationOverview} from '../adapters/server/native-organization';
