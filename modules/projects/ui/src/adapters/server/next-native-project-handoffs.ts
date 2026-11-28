import 'server-only';
import { headers } from 'next/headers';
import type { NativeProjectHandoffView } from '../../public/native-project-handoffs';

export function createNextNativeProjectHandoffsReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    holdingSlug: string,
    projectId: string,
    signal?: AbortSignal,
  ) => Promise<NativeProjectHandoffView[]>,
) {
  return async (projectId: string, holdingSlug: string, signal?: AbortSignal) =>
    read(await headers(), holdingSlug, projectId, signal);
}
