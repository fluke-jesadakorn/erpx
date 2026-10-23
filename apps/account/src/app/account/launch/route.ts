import {accountWorkspaceRequest} from '@erpx/server-identity_access/public/account-workspace';
import {configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {applicationRequest} from '@erpx/server-runtime/public/application-request';

export const dynamic='force-dynamic';
export function GET(request: Request) {
  return accountWorkspaceRequest(applicationRequest(request,configuredProductOrigin('account'),process.env.ERPX_APP_PORT));
}
