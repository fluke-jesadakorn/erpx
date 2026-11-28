import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeMaintenanceCommandInput,
  NativeMaintenanceReadInput,
} from './native-maintenance';
import type {
  NativeMaintenanceDetail,
  NativeMaintenancePlan,
  NativeMaintenancePlanDetail,
  NativeMaintenanceWorkspace,
} from '../../lib/maintenance-view';

export function createNextNativeMaintenanceReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeMaintenanceReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeMaintenanceWorkspace>,
) {
  return async (input: NativeMaintenanceReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}

export function createNextNativeMaintenanceDetailReader(
  detail: (
    incoming: Pick<Headers, 'get'>,
    input: NativeMaintenanceReadInput & { id: string },
    signal?: AbortSignal,
  ) => Promise<NativeMaintenanceDetail>,
) {
  return async (input: NativeMaintenanceReadInput & { id: string }, signal?: AbortSignal) =>
    detail(await headers(), input, signal);
}

export function createNextNativeMaintenancePlanReader(
  plans: (
    incoming: Pick<Headers, 'get'>,
    input: NativeMaintenanceReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeMaintenancePlan[]>,
) {
  return async (input: NativeMaintenanceReadInput, signal?: AbortSignal) =>
    plans(await headers(), input, signal);
}

export function createNextNativeMaintenancePlanDetailReader(
  detail: (
    incoming: Pick<Headers, 'get'>,
    input: NativeMaintenanceReadInput & { id: string },
    signal?: AbortSignal,
  ) => Promise<NativeMaintenancePlanDetail>,
) {
  return async (input: NativeMaintenanceReadInput & { id: string }, signal?: AbortSignal) =>
    detail(await headers(), input, signal);
}

export function createNextNativeMaintenanceCommand(
  command: (
    incoming: Pick<Headers, 'get'>,
    input: NativeMaintenanceCommandInput,
  ) => Promise<{ id: string; version: number; status: string }>,
) {
  return async (input: Omit<NativeMaintenanceCommandInput, 'origin'>) => {
    const incoming = await headers();
    return command(incoming, { ...input, origin: incoming.get('origin') ?? undefined });
  };
}
