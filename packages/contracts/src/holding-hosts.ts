import { productIds, sharedHostKeys, destinationProduct, product, type ProductHostKey } from './product-registry.ts';

/** Operator-owned exact registrations. Production names are metadata, never trust. */
export const holdingHosts = [
  { holdingId: 'h001', holdingSlug: 'acme', hostname: 'acme.jesadakorn.com', environment:'development' },
  { holdingId: 'h002', holdingSlug: 'northstar', hostname: 'northstar.jesadakorn.com', environment:'development' },
  { holdingId: 'h003', holdingSlug: 'erpx', hostname: 'erpx-internal.jesadakorn.com', environment:'development' },
] as const;
const contributorHoldingHosts = [
  { holdingId: 'h001', holdingSlug: 'acme', hostname: 'acme.erpx.localhost:3443', environment:'development' },
  { holdingId: 'h002', holdingSlug: 'northstar', hostname: 'northstar.erpx.localhost:3443', environment:'development' },
  { holdingId: 'h003', holdingSlug: 'erpx', hostname: 'erpx-internal.erpx.localhost:3443', environment:'development' },
] as const;
/** Origin choice is supplied by the outer runtime; this module has no ambient state. */
export type OriginProfile = 'public' | 'contributor';
export function activeHoldingHosts(profile: OriginProfile = 'public') { return profile === 'contributor' ? contributorHoldingHosts : holdingHosts; }
export function accountDevelopmentOrigin(profile: OriginProfile = 'public') { return profile === 'contributor' ? 'https://account.erpx.localhost:3443' : 'https://account-dev-erpx.jesadakorn.com'; }
export function contributorExactOrigin(host: string | null | undefined, profile: OriginProfile = 'public') {
  if (profile !== 'contributor' || !host) return null;
  return host === 'account.erpx.localhost:3443' || contributorHoldingHosts.some(entry => entry.hostname === host) ? `https://${host}` : null;
}
export type HoldingHost = typeof holdingHosts[number] | typeof contributorHoldingHosts[number];
export function holdingRoutingEnabled(env: Record<string, string | undefined>) {
  return env.ERPX_HOLDING_ROUTING === 'enabled' && env.ERPX_ENVIRONMENT === 'local'
    && env.ERPX_PRODUCT_ENVIRONMENT === 'development';
}
export function holdingHostForHost(host: string | null | undefined, profile: OriginProfile = 'public'): HoldingHost | undefined {
  return activeHoldingHosts(profile).find(entry => entry.hostname === host);
}
export function holdingHostForSlug(slug: string, profile: OriginProfile = 'public') { return activeHoldingHosts(profile).find(entry => entry.holdingSlug === slug); }
export function holdingOrigin(slug: string, profile: OriginProfile = 'public') {
  const entry = holdingHostForSlug(slug, profile); return entry ? `https://${entry.hostname}` : null;
}
export function futureHoldingOrigin(slug: string) { return `https://${slug}.erpx.com`; }
export function holdingAudience(host: string | null | undefined, profile: OriginProfile = 'public') {
  const entry = holdingHostForHost(host, profile); return entry ? `holding:${entry.environment}:${entry.holdingId}:${entry.hostname}` : null;
}
const appKeys = [...productIds, ...sharedHostKeys.filter(key => key !== 'account')];
const publicRoots: Record<string, ProductHostKey> = { book: 'booking', 'customer-portal': 'portal', 'legal-portal': 'legal-portal', forms: 'forms' };
export const reservedWorkspaceSlugs = [...new Set([...appKeys, ...Object.keys(publicRoots), 'departments', 'sso', 'api', '_next', '__erpx', 'login', 'logout', 'launch', 'choose-work-role', 'applications', 'account', 'provider', 'platform', 'robots.txt', 'manifest.webmanifest', 'brand', 'fonts', 'vendor'])];
export function reservedWorkspaceSlug(slug: string) { return reservedWorkspaceSlugs.includes(slug); }
export type HoldingRoute = { app: ProductHostKey; internalPath: string; kind: 'page' | 'entry' | 'transport' | 'system'; branchSlug?: string; departmentSlug?: string };
const ownerRoots: Partial<Record<ProductHostKey, string>> = {
  finance: 'finance', people: 'people-organization', sales: 'customers-sales', supply: '', assets: 'services/assets',
  projects: 'projects-routines', recruit: 'people-organization/talent', learn: 'knowledge/learning',
  support: 'customers-sales/customers/service', service: 'services/field-service', legal: 'services/legal',
  manufacturing: 'production', admin: 'administration', pos: 'pos', pms: 'pms', apps: '',
};
function appKey(value: string): value is ProductHostKey { return (appKeys as readonly string[]).includes(value); }
function validSegment(value: string) { return /^[a-z0-9][a-z0-9_-]*$/.test(value); }
export function internalWorkspaceBase(holdingSlug: string, branchSlug?: string, departmentSlug?: string) {
  return `/holdings/${holdingSlug}${branchSlug ? `/branches/${branchSlug}` : ''}${departmentSlug ? `/departments/${departmentSlug}` : ''}`;
}
function ownerDestination(app: ProductHostKey, tail: string[]) {
  if (!tail.length) return product(app)?.landing ?? (app === 'admin' ? 'administration/technology/organization' : '');
  const root = ownerRoots[app] ?? ''; return [root, ...tail].filter(Boolean).join('/');
}
/** Parse presentation only. Organization lineage and current authority remain owner reads. */
export function parseHoldingRoute(holdingSlug: string, pathname: string): HoldingRoute | null {
  if (!holdingHostForSlug(holdingSlug) || /[\\\u0000-\u001f\u007f]/u.test(pathname) || /%(?:2f|5c|00)/i.test(pathname)) return null;
  const parts = pathname.split('/').filter(Boolean);
  if (parts.some(part => part === '.' || part === '..')) return null;
  if (parts[0] === '__erpx') {
    if(appKey(parts[1])&&parts[2]==='erpx-push-sw.js'&&parts.length===3)return {app:parts[1],internalPath:'/erpx-push-sw.js',kind:'transport'};
    if(['pos','pms'].includes(parts[1])&&parts[2]==='front'&&parts[3]===holdingSlug&&validSegment(parts[4]??'')&&parts[5]==='logo'&&parts.length===6)
      return {app:parts[1] as 'pos'|'pms',internalPath:`/front/${holdingSlug}/${parts[4]}/${parts[1]}/logo`,kind:'transport'};
    if (!appKey(parts[1]) || !['api', '_next'].includes(parts[2])) return null;
    const internalPath = '/' + parts.slice(2).join('/');
    if (!holdingPathMatches(holdingSlug, internalPath)) return null;
    return { app: parts[1], internalPath, kind: 'transport' };
  }
  if (!parts.length) return { app: 'apps', internalPath: '/product-entry', kind: 'entry' };
  if (['api', '_next', 'brand', 'fonts', 'vendor','erpx-push-sw.js'].includes(parts[0]) || /^(?:favicon\.|icon[.-]|apple-icon\.|manifest\.|robots\.)/.test(parts[0])) {
    if (!holdingPathMatches(holdingSlug, pathname)) return null;
    return { app: 'apps', internalPath: pathname, kind: 'system' };
  }
  if (['sso', 'login', 'account', 'launch', 'choose-work-role', 'applications'].includes(parts[0])) return { app: 'apps', internalPath: pathname, kind: 'system' };
  if (parts[0] === 'provider') return holdingSlug === 'erpx' ? { app: 'admin', internalPath: pathname, kind: 'system' } : null;
  if (parts[0] === 'admin' && ['launch','choose-work-role'].includes(parts[1])) return {app:'admin',internalPath:pathname,kind:'system'};
  if (pathname === '/public/launch') return {app:'portal',internalPath:pathname,kind:'system'};
  if (publicRoots[parts[0]]) return { app: publicRoots[parts[0]], internalPath: `/${parts[0]}/${holdingSlug}${parts.length > 1 ? '/' + parts.slice(1).join('/') : ''}`, kind: 'page' };
  if (parts[0] === 'holdings') {
    if (!holdingPathMatches(holdingSlug, pathname)) return null;
    let dest = parts.slice(2); if (dest[0] === 'branches') dest = dest.slice(2); if (dest[0] === 'departments') dest = dest.slice(2);
    const app = dest[0] === 'administration' || dest[0] === 'erpx-management' ? 'admin' : destinationProduct(dest.join('/'));
    return app ? { app, internalPath: pathname, kind: 'system' } : null;
  }
  let branchSlug: string | undefined, departmentSlug: string | undefined, offset = 0;
  if (parts[0] === 'departments') { departmentSlug = parts[1]; offset = 2; }
  else if (!appKey(parts[0])) { branchSlug = parts[0]; offset = 1; if (!appKey(parts[offset])) { departmentSlug = parts[offset]; offset++; } }
  if (branchSlug && (!validSegment(branchSlug) || reservedWorkspaceSlug(branchSlug)) || departmentSlug && (!validSegment(departmentSlug) || reservedWorkspaceSlug(departmentSlug))) return null;
  const app = parts[offset]; if (!appKey(app) || app in { booking: 1, portal: 1, forms: 1, 'legal-portal': 1 }) return null;
  if (!branchSlug && !departmentSlug && parts.length === 1) return { app, internalPath: '/product-entry', kind: 'entry' };
  return { app, internalPath: `${internalWorkspaceBase(holdingSlug, branchSlug, departmentSlug)}/${ownerDestination(app, parts.slice(offset + 1))}`, kind: 'page', branchSlug, departmentSlug };
}
export function holdingPathMatches(holdingSlug: string, pathname: string) {
  const match = pathname.match(/^\/(?:api\/(?:v1|public)\/)?holdings\/([^/]+)(?:\/|$)/)
    ?? pathname.match(/^\/(?:api\/v1\/front|front)\/([^/]+)(?:\/|$)/)
    ?? pathname.match(/^\/(?:api\/(?:public\/booking|customer-portal|legal-portal|public\/marketing)|book|customer-portal|legal-portal|forms)\/([^/]+)(?:\/|$)/);
  return !match || match[1] === holdingSlug;
}
/** Translate an owner URL without changing its query, fragment, or record identity. */
export function holdingUiUrl(value: string, company?: string, profile: OriginProfile = 'public'): string | null {
  const url = new URL(value, 'https://owner.invalid');
  const match = url.pathname.match(/^\/holdings\/([^/]+)(?:\/branches\/([^/]+))?(?:\/departments\/([^/]+))?(?:\/(.*))?$/);
  if (!match) return null;
  const [, holding, branch, department, destination = 'home'] = match, origin = holdingOrigin(holding, profile);
  if (!origin) return null;
  const app: ProductHostKey | undefined = /^(administration|erpx-management)(?:\/|$)/.test(destination) ? 'admin' : destinationProduct(destination);
  if (!app) return null;
  const root = ownerRoots[app] ?? '', landing = product(app)?.landing;
  const tail = destination === landing ? '' : root && (destination === root || destination.startsWith(root + '/')) ? destination.slice(root.length).replace(/^\//, '') : destination;
  url.pathname = [branch ?? (department ? 'departments' : ''), department, app, tail].filter(Boolean).join('/');
  if (!branch && !department && company) url.searchParams.set('company', company);
  return origin + url.pathname + url.search + url.hash;
}
export function holdingTransportPath(app: ProductHostKey, path: string) { return `/__erpx/${app}${path.startsWith('/') ? path : '/' + path}`; }

export function holdingPublicUiUrl(value:string,profile: OriginProfile = 'public'){
  const url=new URL(value,'https://owner.invalid'),match=url.pathname.match(/^\/(book|customer-portal|legal-portal|forms)\/([^/]+)(.*)$/);
  if(!match)return null;const origin=holdingOrigin(match[2],profile);if(!origin)return null;
  return origin+'/'+match[1]+match[3]+url.search+url.hash;
}
export function holdingFrontUiUrl(value:string,app:'pos'|'pms',profile: OriginProfile = 'public'){
  const url=new URL(value,'https://owner.invalid');
  const match=url.pathname.match(/^\/(?:front\/)?([^/]+)\/([^/]+)\/(?:pos\/|pms\/)?(holding|branches\/([^/]+)(?:\/departments\/([^/]+))?|departments\/([^/]+))(?:\/(.*))?$/);
  if(!match)return null;const [,holding,company,location,branch,department,directDepartment,tail='']=match,origin=holdingOrigin(holding,profile);if(!origin)return null;
  if(location==='holding')url.searchParams.set('company',company);
  return origin+'/'+[branch??(directDepartment?'departments':''),department??directDepartment,app,tail].filter(Boolean).join('/')+url.search+url.hash;
}
