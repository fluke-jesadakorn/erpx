import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { withPayrollPurchasedRead } from '@erpx/server-people_payroll/public/payroll-purchases';
import {
  nativePeoplePayrollAvailable,
  nativePeoplePayrollDetail,
  nativePeoplePayrollFailure,
  nativePeoplePayrollRuns,
  nativePeoplePayrollSummary,
  peoplePayrollPageContext,
  type NativePayrollRun,
  type NativePayrollRunDetail,
  type NativePayrollSummaryRow,
} from './native-payroll-summary-page';

function ownedFailure(error: unknown): never {
  const failure = nativePeoplePayrollFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE',
    'The native People Payroll service is unavailable.',
  );
}

/** Keep the retained Commercial purchase and current Identity authority
 * preflight ahead of the native read. The native owner read still enforces its
 * own current actor scope; no silent TypeScript reader is used as fallback. */
async function admission(actor: WorkContext) {
  try {
    await withPayrollPurchasedRead(actor, async () => undefined);
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedPeoplePayrollSummary(
  actor: WorkContext,
  holdingSlug: string,
  payMonth?: string,
): Promise<NativePayrollSummaryRow[] | undefined> {
  if (!nativePeoplePayrollAvailable()) return undefined;
  await admission(actor);
  try {
    const data = await nativePeoplePayrollSummary(
      peoplePayrollPageContext(actor),
      holdingSlug,
      payMonth,
    );
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedPeoplePayrollRuns(
  actor: WorkContext,
  holdingSlug: string,
): Promise<NativePayrollRun[] | undefined> {
  if (!nativePeoplePayrollAvailable()) return undefined;
  await admission(actor);
  try {
    const data = await nativePeoplePayrollRuns(peoplePayrollPageContext(actor), holdingSlug);
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedPeoplePayrollRun(
  actor: WorkContext,
  holdingSlug: string,
  runId: string,
): Promise<NativePayrollRunDetail | undefined> {
  if (!nativePeoplePayrollAvailable()) return undefined;
  await admission(actor);
  try {
    const data = await nativePeoplePayrollDetail(
      peoplePayrollPageContext(actor),
      holdingSlug,
      runId,
    );
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
