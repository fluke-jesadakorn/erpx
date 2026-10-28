import 'server-only';
import type { FinanceJournalRecordWorkspace } from '@erpx/contracts/finance-journal-read';
import { createNativeJournalWorkspaceClient, NativeFinanceReadError, type NativeFinanceReadConfiguration, type NativeJournalWorkspaceRead } from './native-journal-detail';
import { createNextNativeJournalWorkspaceReader } from './next-native-journal-detail';

/** Data needed to compare the existing page authorization with the native read.
 * It is a rendering input, not an authority or a way to construct one. */
export type NativeJournalPageContext = {
  userId: string; holdingId: string; holdingSlug: string; companyId: string;
  branchId: string | null; departmentId: string | null;
  assignmentId: string; role: string; assignmentScope: string;
  scope: { kind: 'holding' } | { kind: 'branch'; branchSlug: string } | { kind: 'department'; branchSlug?: string; departmentSlug: string };
  authority?: { revision: number; assignmentVersion: number };
  modules: readonly string[]; iam?: { permissions: readonly string[] };
};
const sameSet = (left: readonly string[], right: readonly string[]) => {
  const a = new Set(left), b = new Set(right);
  return a.size === b.size && [...a].every(value => b.has(value));
};

/** The existing page still uses its own actor for branding and actions. The
 * native response supplies render data only and must describe the same scope. */
export function createNativeJournalPageReader(read: NativeJournalWorkspaceRead) {
  const nextRead = createNextNativeJournalWorkspaceReader(read);
  return async (actor: NativeJournalPageContext, holdingSlug: string, journalId: string): Promise<FinanceJournalRecordWorkspace> => {
    const authority = actor.authority;
    if (!authority || !actor.iam || actor.holdingSlug !== holdingSlug) throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
    const branchSlug = actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug;
    const departmentSlug = actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined;
    const captured = {
      userId: actor.userId, holdingId: actor.holdingId, companyId: actor.companyId,
      branchId: actor.branchId, departmentId: actor.departmentId,
      branchSlug: branchSlug ?? null, departmentSlug: departmentSlug ?? null,
      assignmentId: actor.assignmentId, assignmentVersion: authority.assignmentVersion,
      revision: authority.revision, role: actor.role, assignmentScopeKind: actor.assignmentScope,
    };
    const modules = [...actor.modules];
    const permissions = [...actor.iam.permissions];
    const result = await nextRead({
      holdingSlug, journalId, branchSlug, departmentSlug,
      expected: { companyId: captured.companyId, assignmentId: captured.assignmentId, revision: captured.revision, assignmentVersion: captured.assignmentVersion },
    });
    const context = result.context;
    if (Object.entries(captured).some(([key, value]) => context[key as keyof typeof context] !== value) ||
        !sameSet(modules, context.modules) || !sameSet(permissions, context.permissions)) {
      throw new NativeFinanceReadError('NATIVE_FINANCE_CONTEXT_CHANGED');
    }
    return result.data;
  };
}

/** Bounded native response outcomes mapped by the page's existing error policy. */
export function nativeJournalPageFailure(error: unknown): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeFinanceReadError)) return undefined;
  if (error.code === 'NATIVE_FINANCE_CONTEXT_CHANGED') return { status: 409, code: 'WORK_CONTEXT_CHANGED', message: 'Your work role changed. Reload and review before submitting.' };
  if (error.code !== 'NATIVE_FINANCE_UNAVAILABLE') return undefined;
  switch (error.status) {
    case 401: return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
    case 403: return { status: 403, code: 'FORBIDDEN', message: 'This action is unavailable in your current role.' };
    case 404: return { status: 404, code: 'NOT_FOUND', message: 'Journal not found.' };
    case 409: return { status: 409, code: 'NATIVE_FINANCE_CONFLICT', message: 'The journal is unavailable in the current work context.' };
    default: return undefined;
  }
}

function configuration(raw: string): NativeFinanceReadConfiguration {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID');
  const record = value as Record<string, unknown>;
  if (typeof record.privateBaseUrl !== 'string' || typeof record.timeoutMs !== 'number' || !Array.isArray(record.holdings) ||
    !record.holdings.every((holding: unknown) => !!holding && typeof holding === 'object' && !Array.isArray(holding) &&
      typeof (holding as Record<string, unknown>).origin === 'string' && typeof (holding as Record<string, unknown>).holdingSlug === 'string' && typeof (holding as Record<string, unknown>).holdingId === 'string')) {
    throw new NativeFinanceReadError('NATIVE_FINANCE_CONFIGURATION_INVALID');
  }
  return record as NativeFinanceReadConfiguration;
}

/** Explicit opt-in; an invalid configuration or failed native read never falls back. */
export async function nativeJournalPageWorkspace(actor: NativeJournalPageContext, holdingSlug: string, journalId: string): Promise<FinanceJournalRecordWorkspace | undefined> {
  const raw = process.env.ERPX_NATIVE_FINANCE_JOURNAL_CONFIG;
  if (raw === undefined) return undefined;
  const read = createNativeJournalWorkspaceClient(configuration(raw));
  return createNativeJournalPageReader(read)(actor, holdingSlug, journalId);
}
