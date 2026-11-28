import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeProjectWbs,
  NativeProjectWbsReadInput,
} from '../../public/native-project-wbs';

export function createNextNativeProjectWbsReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    projectId: string,
    input: NativeProjectWbsReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeProjectWbs>,
) {
  return async (projectId: string, input: NativeProjectWbsReadInput, signal?: AbortSignal) =>
    read(await headers(), projectId, input, signal);
}
