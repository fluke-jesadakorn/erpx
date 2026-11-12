import 'server-only';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { OrganizationAccessData } from '@erpx/feature-organization/lib/organization-access';
import {
  nativeOrganizationAccessPage,
  nativeOrganizationFailure,
} from '@erpx/feature-organization/lib/native-organization';
import { nativeOrganizationConfigured } from './native-organization-config';

/** Request-scoped headers used by the native Organization access ingress. */
export type NativeOrganizationIncomingHeaders = Pick<Headers, 'get'>;

export type PeopleOrganizationAccessDependencies = Readonly<{
  /** Request headers. Defaults to the Next request scope; supplied by tests
   * and by callers that already hold the rendered request. */
  request?: () => Promise<NativeOrganizationIncomingHeaders>;
  /** Native access page composition. Defaults to the feature-organization
   * owner helper; supplied by tests. */
  page?: typeof nativeOrganizationAccessPage;
}>;

/** Native-first Organization access read for the People workspace and move
 * adapters.
 *
 * `ERPX_NATIVE_ORGANIZATION_CONFIG` unset keeps the retained
 * `organizationAccessData` read. When configured, the native owner read is the
 * only source: a failure is mapped through the existing ApiError contract and
 * never silently falls back to the retained reader. The retained callback is
 * therefore invoked at most once, and only for the explicit unconfigured
 * path. */
export async function organizationAccessNativeFirst(
  actor: WorkContext,
  retained: () => Promise<OrganizationAccessData>,
  dependencies: PeopleOrganizationAccessDependencies = {},
): Promise<OrganizationAccessData> {
  if (!nativeOrganizationConfigured()) return retained();
  let incoming: NativeOrganizationIncomingHeaders;
  try {
    incoming = await (dependencies.request ?? headers)();
  } catch {
    // Configured native reads are request scoped. Without the rendered request
    // headers the owner ingress cannot be addressed, so fail closed instead of
    // silently serving the replaced TypeScript projection.
    throw new ApiError(
      503,
      'NATIVE_ORGANIZATION_UNAVAILABLE',
      'The organization workspace is unavailable.',
    );
  }
  try {
    return await (dependencies.page ?? nativeOrganizationAccessPage)(
      actor,
      actor.holdingSlug,
      incoming,
      retained,
    );
  } catch (error) {
    const failure = nativeOrganizationFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}
