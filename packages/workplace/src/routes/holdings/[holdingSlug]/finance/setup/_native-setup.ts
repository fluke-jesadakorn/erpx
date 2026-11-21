import 'server-only';
import { nativeFinanceSetupWorkspace, type NativeFinanceSetup } from '@erpx/feature-finance/lib/native-finance-setup';
import { financeSetupWorkspace } from '@erpx/server-finance/public/finance-workspace';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Native Finance setup readiness when the server-only configuration is present.
 * Branch/department setup remains an explicit legacy read because the native
 * owner read requires a Company assignment. Configured native failures close
 * instead of falling back to the TypeScript owner.
 */
export async function readFinanceSetup(
  actor: WorkContext,
  holdingSlug: string,
): Promise<{ setup: NativeFinanceSetup; source: 'native' | 'legacy' }> {
  const nativeActor =
    actor.authority && actor.iam
      ? {
          userId: actor.userId,
          holdingId: actor.holdingId,
          holdingSlug,
          companyId: actor.companyId,
          branchId: actor.branchId,
          departmentId: actor.departmentId,
          assignmentId: actor.assignmentId,
          role: actor.role,
          assignmentScope: actor.assignmentScope,
          scope:
            actor.scope.kind === 'holding'
              ? { kind: 'holding' as const }
              : actor.scope.kind === 'branch'
                ? { kind: 'branch' as const, branchSlug: actor.scope.branchSlug ?? '' }
                : {
                    kind: 'department' as const,
                    branchSlug: actor.scope.branchSlug ?? undefined,
                    departmentSlug: actor.scope.departmentSlug ?? '',
                  },
          authority: {
            revision: actor.authority.revision,
            assignmentVersion: actor.authority.assignmentVersion,
          },
          modules: actor.modules,
          iam: { permissions: actor.iam.permissions },
        }
      : undefined;
  const native = await nativeFinanceSetupWorkspace(nativeActor, holdingSlug);
  if (native) return { setup: native, source: 'native' };
  return { setup: await financeSetupWorkspace(actor), source: 'legacy' };
}
