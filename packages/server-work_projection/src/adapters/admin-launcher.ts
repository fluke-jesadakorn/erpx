import 'server-only';
import {configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {productWorkspaceChoices} from '@erpx/server-organization/public/workspace-choices';
import {adminWorkspaceAvailabilityReader} from '@erpx/server-identity_access/public/admin-workspace';
import {ApiError} from '@erpx/server-runtime/public/http';
import {createAdminLauncher} from '@erpx/server-work_projection/application/admin-launcher';
export function adminLaunchChoices(){return createAdminLauncher({choices:productWorkspaceChoices,available:adminWorkspaceAvailabilityReader(),origin:()=>configuredProductOrigin('admin'),denied:error=>error instanceof ApiError&&[401,403,404].includes(error.status)||error instanceof Error&&['UNAUTHENTICATED','FORBIDDEN','WORKSPACE_NOT_FOUND','COMPANY_REQUIRED'].includes(error.message)})();}
