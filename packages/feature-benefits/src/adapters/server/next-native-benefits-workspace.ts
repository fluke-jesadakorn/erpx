import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeBenefitsReadInput,
  NativeBenefitsWorkspace,
} from '../../public/native-benefits-workspace';

export function createNextNativeBenefitsWorkspaceReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeBenefitsReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeBenefitsWorkspace>,
) {
  return async (input: NativeBenefitsReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
