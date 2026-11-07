import 'server-only';
import { headers } from 'next/headers';
import type {
  NativePeoplePayrollReadInput,
  NativePayrollRun,
  NativePayrollSummaryRow,
} from '../../public/native-payroll-summary';

export function createNextNativePeoplePayrollSummaryReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativePeoplePayrollReadInput,
    signal?: AbortSignal,
  ) => Promise<NativePayrollSummaryRow[]>,
) {
  return async (input: NativePeoplePayrollReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}

export function createNextNativePeoplePayrollRunsReader(
  runs: (
    incoming: Pick<Headers, 'get'>,
    input: NativePeoplePayrollReadInput,
    signal?: AbortSignal,
  ) => Promise<NativePayrollRun[]>,
) {
  return async (input: NativePeoplePayrollReadInput, signal?: AbortSignal) =>
    runs(await headers(), input, signal);
}
