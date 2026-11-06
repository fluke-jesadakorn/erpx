import 'server-only';
import {
  createNativePlatformAdminClient,
  nativePlatformAdminConfiguration,
  NativePlatformAdminError,
  type NativePlatformAdminActor,
  type NativePlatformMigrationResult,
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

const configurationNames = [
  'ERPX_NATIVE_PLATFORM_ADMIN_CONFIG',
  'ERPX_NATIVE_PLATFORM_ADMINISTRATION_CONFIG',
] as const;

/** Trusted deployment input only. The short name stays canonical; the longer
 * name is accepted so deployment notes may use either label. */
function configurationValue(): string | undefined {
  for (const name of configurationNames) {
    const value = process.env[name];
    if (value !== undefined) return value;
  }
  return undefined;
}

/** Selector presence only; a configured failure never falls back to the retained reader. */
export function nativePlatformAdminAvailable(): boolean {
  return configurationValue() !== undefined;
}

function nativeClient() {
  const raw = configurationValue();
  if (raw === undefined) return undefined;
  return createNativePlatformAdminClient(nativePlatformAdminConfiguration(raw));
}

/**
 * Keep the explicit retained implementation only while the native transport is
 * unset. `native` throws for a configured but unavailable service, so this
 * selector can never replace a configured failure with retained data.
 */
export async function nativePlatformAdministrationOrRetained<T>(
  native: () => Promise<T | undefined>,
  retained: () => Promise<T>,
): Promise<T> {
  const result = await native();
  return result ?? (await retained());
}

/** Native control-plane snapshot. Unset configuration returns undefined so the
 * caller keeps the retained reader; a configured failure is surfaced. */
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

export async function nativePlatformMigrate(
  work: PlatformAdminWorkInput,
  platform: PlatformAdminPlatformInput,
  incoming: Pick<Headers, 'get'>,
  body: unknown,
  idempotencyKey: string,
): Promise<NativePlatformMigrationResult | undefined> {
  const native = nativeClient();
  if (!native) return undefined;
  return native.migrate(incoming, platformAdminActor(work, platform), body, idempotencyKey);
}

export { nativePlatformAdminFailure } from '../adapters/server/native-platform-administration';
