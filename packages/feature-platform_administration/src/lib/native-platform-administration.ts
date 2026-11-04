import 'server-only';
import {
  createNativePlatformAdminClient,
  nativePlatformAdminConfiguration,
  NativePlatformAdminError,
  type NativePlatformAdminActor,
  type NativePlatformOperationsSnapshot,
  type NativePlatformProvisioningResult,
  type NativePlatformRecoveryResult,
} from '../adapters/server/native-platform-administration';

export type PlatformAdminWorkInput = {
  userId: string;
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  assignmentId: string;
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[];
  iam?: { permissions: readonly string[] };
};

export type PlatformAdminPlatformInput = {
  userId: string;
  assignmentId: string;
  role: string;
  modules?: readonly string[];
};

export function platformAdminActor(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
): NativePlatformAdminActor {
  if (work.userId !== platform.userId)
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED');
  if (work.holdingId !== 'h003' || work.holdingSlug !== 'erpx')
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED');
  if (!['platform_admin', 'platform_auditor'].includes(platform.role))
    throw new NativePlatformAdminError('NATIVE_PLATFORM_ADMIN_CONTEXT_CHANGED');
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

/** Selector presence only; a configured failure never falls back to the retained reader. */
export function nativePlatformAdminAvailable(): boolean {
  return process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG !== undefined;
}

function nativeClient() {
  const raw = process.env.ERPX_NATIVE_PLATFORM_ADMIN_CONFIG;
  if (raw === undefined) return undefined;
  return createNativePlatformAdminClient(nativePlatformAdminConfiguration(raw));
}

/** Native control-plane snapshot. Unset configuration returns undefined so the
 * caller can fail closed with an explicit 503 instead of using the retained reader. */
export async function nativePlatformOperationsForPresentation(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
  incoming: Pick<Headers, 'get'>,
): Promise<NativePlatformOperationsSnapshot | undefined> {
  const native = nativeClient();
  if (!native) return undefined;
  return native.operations(incoming, platformAdminActor(work, platform));
}

export async function nativePlatformProvision(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
  incoming: Pick<Headers, 'get'>,
  body: unknown,
  idempotencyKey: string,
): Promise<NativePlatformProvisioningResult | undefined> {
  const native = nativeClient();
  if (!native) return undefined;
  return native.provision(incoming, platformAdminActor(work, platform), body, idempotencyKey);
}

export async function nativePlatformRecover(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
  incoming: Pick<Headers, 'get'>,
  body: unknown,
  idempotencyKey: string,
): Promise<NativePlatformRecoveryResult | undefined> {
  const native = nativeClient();
  if (!native) return undefined;
  return native.recover(incoming, platformAdminActor(work, platform), body, idempotencyKey);
}

export { nativePlatformAdminFailure } from '../adapters/server/native-platform-administration';
