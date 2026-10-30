import { developmentProductOrigin, isProductHostKey, productIds, sharedHostKeys, type ProductHostKey } from "@erpx/server-commercial/public/product-registry";
import { safeProductReturnPath } from "@erpx/server-identity_access/public/rules/product-sso";
import {holdingRoutingEnabled,holdingHostForHost,holdingOrigin,holdingUiUrl,holdingAudience,accountDevelopmentOrigin,holdingPublicUiUrl,holdingFrontUiUrl} from '@erpx/feature-runtime/lib/holding-hosts';

export type ProductHostConfiguration = { routing: boolean; sso: boolean; environment: "development" | null };
export function productHostConfiguration(): ProductHostConfiguration {
  return {
    routing: process.env.ERPX_PRODUCT_ROUTING === "enabled",
    sso: process.env.ERPX_PRODUCT_SSO === "enabled",
    environment: process.env.ERPX_PRODUCT_ENVIRONMENT === "development" && process.env.ERPX_ENVIRONMENT === "local" ? "development" : null,
  };
}
/** Exact, owned development origins only; no suffix/wildcard or future-origin trust. */
export function productHostForHost(host: string | null, config = productHostConfiguration()): ProductHostKey | null {
  if (!host || !config.routing || !config.sso || config.environment !== "development") return null;
  if (holdingRoutingEnabled(process.env)) {
    if (host === new URL(accountDevelopmentOrigin()).host) return 'account';
    return holdingHostForHost(host) && isProductHostKey(process.env.ERPX_APP_ID ?? '') ? process.env.ERPX_APP_ID as ProductHostKey : null;
  }
  for (const key of [...productIds, ...sharedHostKeys]) if (new URL(developmentProductOrigin(key)).host === host) return key;
  return null;
}
export function trustedProductOrigin(value: string, config = productHostConfiguration()): string | null {
  if(!config.routing||!config.sso||config.environment!=='development')return null;
  let url: URL;
  try { url = new URL(value); } catch { return null; }
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) return null;
  return (holdingRoutingEnabled(process.env) && holdingHostForHost(url.host)) || productHostForHost(url.host, config) ? url.origin : null;
}
export function configuredProductOrigin(key: string, config = productHostConfiguration(), holdingSlug?: string): string | null {
  if (holdingRoutingEnabled(process.env) && key !== 'account') return holdingSlug && config.routing && config.sso && config.environment === 'development' ? holdingOrigin(holdingSlug) : null;
  return isProductHostKey(key) ? trustedProductOrigin(key === 'account' && holdingRoutingEnabled(process.env) ? accountDevelopmentOrigin() : developmentProductOrigin(key), config) : null;
}
export function requestProductOrigin(key: string, headers: Pick<Headers, 'get'>): string | null {
  const holding = holdingRoutingEnabled(process.env) && holdingHostForHost(headers.get('host'));
  return key !== 'account' && holding ? holdingOrigin(holding.holdingSlug) : configuredProductOrigin(key);
}
export function ssoAudienceForHost(host: string | null) {
  return host&&holdingRoutingEnabled(process.env)&&trustedProductOrigin(`https://${host}`) ? holdingAudience(host)??productHostForHost(host) : productHostForHost(host);
}

/** Authenticated GET navigation only. Re-enter through the central account
 * rather than silently keeping a different account's existing child cookie. */
export function configuredProductHandoff(key: ProductHostKey, next = "/", config = productHostConfiguration()): string | null {
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
    if (key === 'admin' && next.startsWith('/provider')) return new URL(next, holdingOrigin('erpx')!).href;
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
