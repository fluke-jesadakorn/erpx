import 'server-only';
import {currentProviderContext} from '@erpx/server-identity_access/public/provider-context';
import {ApiError} from '@erpx/server-runtime/public/http';
import {providerCommercialOverview} from '@erpx/server-commercial/adapters/provider-commercial';
import {createProviderCommercialPageContext} from '@erpx/server-commercial/application/provider-page-context';
export const providerCommercialPageContext=createProviderCommercialPageContext({context:currentProviderContext,overview:providerCommercialOverview,changed:()=>new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Your Provider role changed. Reload and review the current context.')});
