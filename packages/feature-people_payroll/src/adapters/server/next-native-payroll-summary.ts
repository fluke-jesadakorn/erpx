import 'server-only';
import { headers } from 'next/headers';
import type {
  NativePayrollRun,
  NativePayrollRunDetail,
  NativePayrollSummaryRow,
  NativePeoplePayrollReadInput,
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

export function createNextNativePeoplePayrollDetailReader(
  detail: (
    incoming: Pick<Headers, 'get'>,
    input: NativePeoplePayrollReadInput,
    signal?: AbortSignal,
  ) => Promise<NativePayrollRunDetail>,
) {
  return async (input: NativePeoplePayrollReadInput, signal?: AbortSignal) =>
    detail(await headers(), input, signal);
}
