import 'server-only';
import {currentProviderContext} from '@erpx/identity-access-server/public/provider-context';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {providerCommercialOverview} from '@erpx/commercial-server/adapters/provider-commercial';
import {createProviderCommercialPageContext} from '@erpx/commercial-server/application/provider-page-context';
export const providerCommercialPageContext=createProviderCommercialPageContext({context:currentProviderContext,overview:providerCommercialOverview,changed:()=>new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Your Provider role changed. Reload and review the current context.')});
