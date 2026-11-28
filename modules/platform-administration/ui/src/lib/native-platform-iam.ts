import 'server-only';
import {
  createNativePlatformIamClient,
  nativePlatformIamConfiguration,
  NativePlatformIamError,
  type NativePlatformIamActor,
  type NativePlatformIamCommand,
} from '../adapters/server/native-platform-iam';
import type {
  PlatformAdminPlatformInput,
  PlatformAdminWorkInput,
} from './native-platform-administration';

/** Selector presence only; a configured failure never falls back to `iamApi`. */
export function nativePlatformIamAvailable(): boolean {
  return process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG !== undefined;
}

/** The retained Manage ERPX/Platform context binds both identities and roles. */
export function nativePlatformIamActor(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
): NativePlatformIamActor {
  if (work.userId !== platform.userId)
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  if (work.holdingId !== 'h003' || work.holdingSlug !== 'erpx')
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  if (!['platform_admin', 'platform_auditor'].includes(platform.role))
    throw new NativePlatformIamError('NATIVE_PLATFORM_IAM_CONTEXT_CHANGED');
  return {
    userId: work.userId,
    holdingId: work.holdingId,
    holdingSlug: work.holdingSlug,
    companyId: work.companyId,
    branchId: work.branchId,
    departmentId: work.departmentId,
    workAssignmentId: work.assignmentId,
    platformAssignmentId: platform.assignmentId,
    platformRole: platform.role,
    authority: work.authority,
    modules: work.modules,
    iam: work.iam,
  };
}

/**
 * Unset configuration returns undefined so the route keeps the retained
 * `iamApi` handler. A configured client failure is surfaced unchanged.
 */
export async function nativePlatformIamCommand(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
  incoming: Pick<Headers, 'get'>,
  command: NativePlatformIamCommand,
  body: string,
  idempotencyKey: string,
): Promise<Record<string, unknown> | undefined> {
  const raw = process.env.ERPX_NATIVE_PLATFORM_IAM_CONFIG;
  if (raw === undefined) return undefined;
  const client = createNativePlatformIamClient(nativePlatformIamConfiguration(raw));
  return client.command(
    incoming,
    nativePlatformIamActor(work, platform),
    command,
    body,
    idempotencyKey,
  );
}
