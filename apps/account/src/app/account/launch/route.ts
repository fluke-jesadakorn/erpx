import {accountWorkspaceRequest} from '@erpx/identity-access-server/public/account-workspace';
import {configuredProductOrigin} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {applicationRequest} from '@erpx/platform-runtime-server/public/application-request';

export const dynamic='force-dynamic';
export function GET(request: Request) {
  return accountWorkspaceRequest(applicationRequest(request,configuredProductOrigin('account'),process.env.ERPX_APP_PORT));
}
