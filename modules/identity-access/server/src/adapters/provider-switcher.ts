import {ssoAudienceForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import 'server-only';
import { productIds } from '@erpx/platform-contracts/product-registry';
import { productHostForHost } from '@erpx/platform-runtime-ui/lib/product-hosts';
import { sessionFromRequest } from '@erpx/identity-access-server/public/account-session';
import { providerContextStore } from '@erpx/identity-access-server/adapters/provider-context-store';
import { runtimeEnv } from '../../../../../platform/runtime/server/src/public/runtime';
import { isTransactionalSqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/** Presentation bridge for authenticated staff hosts. The original complete
 * Provider store still resolves Platform assignments, IAM, versions and CAS. */
export async function providerContextStoreForRequest(request: Request) {
  const session = await sessionFromRequest(request);
  if (!session) throw new ApiError(401, 'SIGN_IN_REQUIRED', 'Sign in again to continue.');
  const owner = (await runtimeEnv()).CONTROL_DB;
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503, 'PROVIDER_CONTEXT_TRANSACTION_REQUIRED', 'Provider selection is unavailable.');
  return providerContextStore(owner, { userId: session.user.id, sessionId: session.session.id, selectedCookie: null, audience:ssoAudienceForHost(request.headers.get('host'))??undefined, remember: true });
}
export async function providerSwitcherStore(request: Request) {
  const audience = productHostForHost(request.headers.get('host'));
  if (!audience || !['apps', 'admin', ...productIds].includes(audience)) throw new ApiError(404, 'NOT_FOUND', 'Not found.');
  return providerContextStoreForRequest(request);
}
