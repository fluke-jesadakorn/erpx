import {holdingRoutingEnabled,holdingHostForHost,holdingOrigin} from '@erpx/feature-runtime/lib/holding-hosts';

export function publicHoldingPath(holdingSlug:string,root:'book'|'customer-portal'|'legal-portal'|'forms'){
 const useHolding=typeof window==='undefined'?holdingRoutingEnabled(process.env):!!holdingHostForHost(window.location.host);
 return useHolding&&holdingOrigin(holdingSlug)?`${holdingOrigin(holdingSlug)}/${root}`:`/${root}/${encodeURIComponent(holdingSlug)}`;
}
