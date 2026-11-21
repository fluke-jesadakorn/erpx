import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeCrmPipelinePage,
  NativeCrmPipelineReadInput,
} from '../../public/native-crm-pipeline';

export function createNextNativeCrmPipelineReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeCrmPipelineReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeCrmPipelinePage>,
) {
  return async (input: NativeCrmPipelineReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
