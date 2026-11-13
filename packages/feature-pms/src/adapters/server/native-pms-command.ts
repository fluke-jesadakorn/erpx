import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativePmsClient,
  nativePmsConfiguration,
  nativePmsGuestAccountBody,
  NativePmsError,
  type NativePmsPageContext,
} from './native-pms';

/** Native PMS guest-account commands owned by pms_guest_account_router. */
export const nativePmsGuestAccountOperations: ReadonlySet<string> = new Set([
  'guest_account.create',
  'guest_account.update',
  'charge.add',
  'charge.import_pos',
  'guest_account.adjustment',
  'guest_account.transfer',
]);

function pageContext(actor: WorkContext): NativePmsPageContext {
  if (!actor.authority || !actor.iam) throw new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED');
  const scope =
    actor.scope.kind === 'holding'
      ? ({ kind: 'holding' } as const)
      : actor.scope.kind === 'branch'
        ? ({ kind: 'branch', branchSlug: actor.scope.branchSlug } as const)
        : ({
            kind: 'department',
            ...(actor.scope.branchSlug ? { branchSlug: actor.scope.branchSlug } : {}),
            departmentSlug: actor.scope.departmentSlug,
          } as const);
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
    scope,
    authority: {
      revision: actor.authority.revision,
      assignmentVersion: actor.authority.assignmentVersion,
    },
    modules: [...actor.modules],
    iam: { permissions: [...actor.iam.permissions] },
  };
}

function scopeInput(actor: NativePmsPageContext) {
  if (actor.scope.kind === 'holding') return {};
  if (actor.scope.kind === 'branch') return { branchSlug: actor.scope.branchSlug };
  return {
    ...(actor.scope.branchSlug ? { branchSlug: actor.scope.branchSlug } : {}),
    departmentSlug: actor.scope.departmentSlug,
  };
}

export function nativePmsGuestAccountAvailable(): boolean {
  return process.env.ERPX_NATIVE_PMS_CONFIG !== undefined;
}

export async function nativePmsGuestAccountCommand(
  request: Request,
  actor: WorkContext,
  holdingSlug: string,
  body: Record<string, unknown>,
  key: string,
) {
  const raw = process.env.ERPX_NATIVE_PMS_CONFIG;
  if (raw === undefined) return undefined;
  const context = pageContext(actor);
  if (!context.authority) throw new NativePmsError('NATIVE_PMS_CONTEXT_CHANGED');
  const client = createNativePmsClient(nativePmsConfiguration(raw));
  return client.guestAccountCommand(request.headers, context, {
    holdingSlug,
    body: nativePmsGuestAccountBody(body as { operation: string }),
    key,
    ...scopeInput(context),
    expected: {
      companyId: context.companyId,
      assignmentId: context.assignmentId,
      revision: context.authority.revision,
      assignmentVersion: context.authority.assignmentVersion,
    },
  });
}
