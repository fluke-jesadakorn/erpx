import {holdingHostForHost, parseHoldingRoute, holdingPathMatches,holdingUiUrl,holdingPublicUiUrl,holdingFrontUiUrl} from '@erpx/contracts/holding-hosts';
import {destinationProduct, isProductHostKey, type ProductHostKey} from '@erpx/contracts/product-registry';
import catalogue from '../architecture/workspace-app-routes.json';

const apiRoutes = catalogue.apps.flatMap(app => app.entries.filter(entry => entry.endsWith('/route.ts') && entry.includes('/api/')).map(entry => {
  const path = '/' + entry.slice('src/app/'.length, -'/route.ts'.length);
  const pattern = path.split('/').map(part => /^\[\[\.\.\./.test(part) ? '(?:/.*)?' : /^\[\.\.\./.test(part) ? '/.+' : /^\[/.test(part) ? '/[^/]+' : part ? '/' + part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') : '').join('');
  return {app: app.id as ProductHostKey, regex: new RegExp('^' + pattern + '/?$'), rank: path.replace(/\[.*?\]/g, '').length};
}));
/** Owner redirects retain an explicitly selected Company through the public
 * address conversion. Foreign origins (especially Account exchange) are untouched. */
export function holdingGatewayLocation(host:string,value:string,location:string,app:ProductHostKey){
  if(!holdingHostForHost(host))return location;
  const request=new URL(value,`https://${host}`),target=new URL(location,request);
  if(target.origin!==request.origin)return location;
  return holdingUiUrl(target.href,request.searchParams.get('company')??request.searchParams.get('companyId')??undefined)
    ??holdingPublicUiUrl(target.href)
    ??(app==='pos'||app==='pms'?holdingFrontUiUrl(target.href,app):null)??location;
}
/** Referrer selects transport only among apps which declare this exact API.
 * It never supplies an organization scope or an authorization decision. */
export function holdingGatewayApp(host: string, value: string, referer?: string): ProductHostKey | null {
  const holding = holdingHostForHost(host); if (!holding) return null;
  const url = new URL(value, `https://${host}`);
  if (!holdingPathMatches(holding.holdingSlug, url.pathname)) return null;
  if (/^\/(?:admin\/)?(?:launch|choose-work-role)$/.test(url.pathname)||url.pathname==='/account/continue') {
    if (url.pathname.startsWith('/admin/')) return 'admin';
    const app = url.searchParams.get('app'); if (app && isProductHostKey(app) && app !== 'account') return app;
    const next = url.searchParams.get('next');
    if (next) { const parsed = parseHoldingRoute(holding.holdingSlug, new URL(next, url).pathname); if (parsed) return parsed.app; }
  }
  if (url.pathname.startsWith('/api/') && !['/api/health', '/api/ready'].includes(url.pathname)) {
    const candidates = apiRoutes.filter(route => route.regex.test(url.pathname)).sort((a,b) => b.rank - a.rank);
    let refApp: ProductHostKey | undefined;
    try { const ref = referer && new URL(referer); if (ref && ref.origin === url.origin) refApp = parseHoldingRoute(holding.holdingSlug, ref.pathname)?.app; } catch { /* Missing navigation context uses a declared owner. */ }
    return candidates.find(route => route.app === refApp)?.app ?? candidates.find(route => route.app === 'apps')?.app ?? candidates[0]?.app ?? null;
  }
  if (url.pathname === '/public/launch') {
    const next = url.searchParams.get('next'); return next?.startsWith('/legal-portal/') ? 'legal-portal' : 'portal';
  }
  return parseHoldingRoute(holding.holdingSlug, url.pathname)?.app ?? null;
}
