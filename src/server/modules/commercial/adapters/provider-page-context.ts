import 'server-only';
import {currentProviderContext} from '../../identity_access/public/provider-context';
import {ApiError} from '../../runtime/public/http';
import {providerCommercialOverview} from './provider-commercial';
import {createProviderCommercialPageContext} from '../application/provider-page-context';
export const providerCommercialPageContext=createProviderCommercialPageContext({context:currentProviderContext,overview:providerCommercialOverview,changed:()=>new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Your Provider role changed. Reload and review the current context.')});
