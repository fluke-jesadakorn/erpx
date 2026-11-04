import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { requireProjectPurchasedRead } from '@erpx/server-projects/public/purchases';
import {
  nativeProjectWbs,
  nativeProjectsAvailable,
  nativeProjectsFailure,
  nativeProjectsPageContext,
  type NativeProjectWbs,
} from './native-project-wbs-page';

function ownedFailure(error: unknown): never {
  const failure = nativeProjectsFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_PROJECTS_UNAVAILABLE',
    'The native Projects service is unavailable.',
  );
}

/** Keep the retained Commercial project purchase and current Identity
 * authority preflight ahead of the native read. The native owner read still
 * enforces its own current actor scope; no silent TypeScript reader is used as
 * a fallback. */
async function admission(actor: WorkContext, projectId: string) {
  try {
    const database = (await holdingResources(actor.holdingId)).write;
    await requireProjectPurchasedRead(database, actor, projectId);
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedProjectWbs(
  actor: WorkContext,
  holdingSlug: string,
  projectId: string,
): Promise<NativeProjectWbs | undefined> {
  if (!nativeProjectsAvailable()) return undefined;
  await admission(actor, projectId);
  try {
    const data = await nativeProjectWbs(
      nativeProjectsPageContext(actor),
      holdingSlug,
      projectId,
    );
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
