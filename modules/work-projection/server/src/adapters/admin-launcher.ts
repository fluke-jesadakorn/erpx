import 'server-only';
import {configuredProductOrigin} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {productWorkspaceChoices} from '@erpx/organization-server/public/workspace-choices';
import {adminWorkspaceAvailabilityReader} from '@erpx/identity-access-server/public/admin-workspace';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {createAdminLauncher} from '@erpx/work-projection-server/application/admin-launcher';
export function adminLaunchChoices(){return createAdminLauncher({choices:productWorkspaceChoices,available:adminWorkspaceAvailabilityReader(),origin:slug=>configuredProductOrigin('admin',undefined,slug),denied:error=>error instanceof ApiError&&[401,403,404].includes(error.status)||error instanceof Error&&['UNAUTHENTICATED','FORBIDDEN','WORKSPACE_NOT_FOUND','COMPANY_REQUIRED'].includes(error.message)})();}
