/** Runtime-only selection of an explicitly bounded origin profile. */
import * as hosts from '@erpx/platform-contracts/holding-hosts';
export * from '@erpx/platform-contracts/holding-hosts';
export function originProfile(): hosts.OriginProfile {
  if (hosts.oasisDemoStagingEnabled(process.env)) return 'oasis-demo-staging';
  if (process.env.NODE_ENV === 'development' && (process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE === 'loopback' || process.env.ERPX_NATIVE_ORIGIN_PROFILE === 'loopback')) return 'loopback';
  return process.env.NODE_ENV === 'development' && process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE === 'container-localhost' ? 'contributor' : 'public';
}
export function contributorOriginProfile() { return originProfile() === 'contributor'; }
export function activeHoldingHosts() { return hosts.activeHoldingHosts(originProfile()); }
export function accountDevelopmentOrigin() { return hosts.accountDevelopmentOrigin(originProfile()); }
export function contributorExactOrigin(host: string | null | undefined) { return hosts.contributorExactOrigin(host, originProfile()); }
export function holdingHostForHost(host: string | null | undefined) { return hosts.holdingHostForHost(host, originProfile()); }
export function holdingHostForSlug(slug: string) { return hosts.holdingHostForSlug(slug, originProfile()); }
export function holdingOrigin(slug: string) { return hosts.holdingOrigin(slug, originProfile()); }
export function holdingAudience(host: string | null | undefined) { return hosts.holdingAudience(host, originProfile()); }
export function companyAddressRouting(holdingSlug:string) {
  const setting=typeof window==='undefined'
    ? process.env.ERPX_COMPANY_ADDRESS_ROUTING
    : process.env.NEXT_PUBLIC_ERPX_COMPANY_ADDRESS_ROUTING;
  return setting==='enabled'||setting===holdingSlug;
}
export function parseHoldingRoute(holdingSlug: string, pathname: string) { return hosts.parseHoldingRoute(holdingSlug, pathname, originProfile(), companyAddressRouting(holdingSlug)); }
export function holdingUiUrl(value: string, company?: string) { return hosts.holdingUiUrl(value, company, originProfile(), companyAddressRouting(/^\/holdings\/([^/]+)/.exec(new URL(value,'https://owner.invalid').pathname)?.[1]??'')); }
export function holdingPublicUiUrl(value: string) { return hosts.holdingPublicUiUrl(value, originProfile()); }
export function holdingFrontUiUrl(value: string, app: 'pos'|'pms') { return hosts.holdingFrontUiUrl(value, app, originProfile(), companyAddressRouting(/^\/(?:front\/)?([^/]+)/.exec(new URL(value,'https://owner.invalid').pathname)?.[1]??'')); }
