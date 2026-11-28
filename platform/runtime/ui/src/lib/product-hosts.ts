import { developmentProductOrigin, isProductHostKey, productIds, sharedHostKeys, type ProductHostKey } from "@erpx/commercial-server/public/product-registry";
import { safeProductReturnPath } from "@erpx/platform-contracts/product-return-path";
import {holdingRoutingEnabled,holdingHostForHost,holdingOrigin,holdingUiUrl,holdingAudience,accountDevelopmentOrigin,holdingPublicUiUrl,holdingFrontUiUrl,oasisDemoStagingEnabled} from '@erpx/platform-runtime-ui/lib/holding-hosts';

export type ProductHostConfiguration = { routing: boolean; sso: boolean; environment: "development" | "oasis-demo-staging" | null };
export function productHostConfiguration(): ProductHostConfiguration {
  return {
    routing: process.env.ERPX_PRODUCT_ROUTING === "enabled",
    sso: process.env.ERPX_PRODUCT_SSO === "enabled",
    environment: oasisDemoStagingEnabled(process.env) ? 'oasis-demo-staging' :
      process.env.ERPX_PRODUCT_ENVIRONMENT === "development" && process.env.ERPX_ENVIRONMENT === "local" ? "development" : null,
  };
}
function supportsRouting(config: ProductHostConfiguration) {
  return config.routing && config.sso && (config.environment === 'development' ||
    config.environment === 'oasis-demo-staging' && oasisDemoStagingEnabled(process.env) && holdingRoutingEnabled(process.env));
}
function demoProduct(key: string) { return key === 'account' || key === 'apps' || key === 'sales'; }

/** Only explicitly registered origins qualify for product SSO. */
export function productHostForHost(host: string | null, config = productHostConfiguration()): ProductHostKey | null {
  if (!host || !supportsRouting(config)) return null;
  if (holdingRoutingEnabled(process.env)) {
    if (host === new URL(accountDevelopmentOrigin()).host) return 'account';
    const key = process.env.ERPX_APP_ID ?? '';
    return holdingHostForHost(host) && isProductHostKey(key) &&
      (config.environment !== 'oasis-demo-staging' || key !== 'account' && demoProduct(key)) ? key : null;
  }
  for (const key of [...productIds, ...sharedHostKeys]) if (new URL(developmentProductOrigin(key)).host === host) return key;
  return null;
}
export function trustedProductOrigin(value: string, config = productHostConfiguration()): string | null {
  if(!supportsRouting(config))return null;
  let url: URL;
  try { url = new URL(value); } catch { return null; }
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) return null;
  return (holdingRoutingEnabled(process.env) && holdingHostForHost(url.host)) || productHostForHost(url.host, config) ? url.origin : null;
}
export function configuredProductOrigin(key: string, config = productHostConfiguration(), holdingSlug?: string): string | null {
  if (config.environment === 'oasis-demo-staging' && !demoProduct(key)) return null;
  if (holdingRoutingEnabled(process.env) && key !== 'account') return holdingSlug && supportsRouting(config) ? holdingOrigin(holdingSlug) : null;
  return isProductHostKey(key) ? trustedProductOrigin(key === 'account' && holdingRoutingEnabled(process.env) ? accountDevelopmentOrigin() : developmentProductOrigin(key), config) : null;
}
export function requestProductOrigin(key: string, headers: Pick<Headers, 'get'>): string | null {
  if (oasisDemoStagingEnabled(process.env) && !demoProduct(key)) return null;
  const holding = holdingRoutingEnabled(process.env) && holdingHostForHost(headers.get('host'));
  return key !== 'account' && holding ? holdingOrigin(holding.holdingSlug) : configuredProductOrigin(key);
}
export function ssoAudienceForHost(host: string | null) {
  return host&&holdingRoutingEnabled(process.env)&&trustedProductOrigin(`https://${host}`) ? holdingAudience(host)??productHostForHost(host) : productHostForHost(host);
}

/** Authenticated GET navigation only. Re-enter through the central account
 * rather than silently keeping a different account's existing child cookie. */
export function configuredProductHandoff(key: ProductHostKey, next = "/", config = productHostConfiguration()): string | null {
  if (config.environment === 'oasis-demo-staging' && (!supportsRouting(config) || !demoProduct(key))) return null;
  if (holdingRoutingEnabled(process.env) && key !== 'account') {
    const nested = new URL(next, 'https://path.invalid');
    let target = nested.searchParams.get('next') ?? next;
    for(let depth=0;depth<3;depth++){const inner=new URL(target,'https://path.invalid').searchParams.get('next');if(!inner)break;target=inner;}
    const mapped = holdingUiUrl(target, nested.searchParams.get('companyId') ?? undefined)??holdingPublicUiUrl(target)??(key==='pos'||key==='pms'?holdingFrontUiUrl(target,key):null);
    if (mapped) {
      const origin = new URL(mapped).origin;
      if(['booking','forms','portal','legal-portal'].includes(key)){
        const entry=new URL('/sso/start',origin);entry.searchParams.set('next',next);return entry.href;
      }
      if(!/^\/(?:admin\/)?launch$/.test(nested.pathname))return mapped;
      const entry = new URL(key === 'admin' ? '/admin/launch' : '/launch', origin);
      entry.search = new URLSearchParams({app:key, ...Object.fromEntries(nested.searchParams), next:target}).toString();
      return entry.href;
    }
    const providerOrigin = key === 'admin' && next.startsWith('/provider') ? holdingOrigin('erpx') : null;
    if (providerOrigin) return new URL(next, providerOrigin).href;
    if(key==='apps'&&next==='/')return '/?organizations=open';
    return next === '/' ? '/' : next;
  }
  const origin = configuredProductOrigin(key, config);
  if (!origin) return null;
  const target = safeProductReturnPath(next);
  if (key === "account") return new URL(target, origin).href;
  const href = new URL("/sso/start", origin);
  href.searchParams.set("next", target);
  return href.href;
}
