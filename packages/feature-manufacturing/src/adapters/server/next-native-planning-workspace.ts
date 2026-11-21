import 'server-only';
import { headers } from 'next/headers';
import type { NativePlanningWorkspaceReadInput } from '../../public/native-planning-workspace';
import type { NativePlanningWorkspace } from '../../lib/native-planning-types';

export function createNextNativePlanningWorkspaceReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativePlanningWorkspaceReadInput,
    signal?: AbortSignal,
  ) => Promise<NativePlanningWorkspace>,
) {
  return async (input: NativePlanningWorkspaceReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
