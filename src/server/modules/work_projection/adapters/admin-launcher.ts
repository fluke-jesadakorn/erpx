import 'server-only';
import {configuredProductOrigin} from '@/lib/runtime/product-hosts';
import {productWorkspaceChoices} from '../../organization/public/workspace-choices';
import {adminWorkspaceAvailabilityReader} from '../../identity_access/public/admin-workspace';
import {ApiError} from '../../runtime/public/http';
import {createAdminLauncher} from '../application/admin-launcher';
export function adminLaunchChoices(){return createAdminLauncher({choices:productWorkspaceChoices,available:adminWorkspaceAvailabilityReader(),origin:()=>configuredProductOrigin('admin'),denied:error=>error instanceof ApiError&&[401,403,404].includes(error.status)||error instanceof Error&&['UNAUTHENTICATED','FORBIDDEN','WORKSPACE_NOT_FOUND','COMPANY_REQUIRED'].includes(error.message)})();}
