import {canonicalUiHref,routePrefixes} from './destination-registry';
/** Preserve whether an owner contract returns a relative or workspace URL. */
export function canonicalDestinationLink(href:string) {
 if(href.startsWith('/api/'))return href;
 if(href.startsWith('/holdings/')||href.startsWith('/platform')||/^https?:/.test(href))return canonicalUiHref(href);
 const source=href.slice(1).split(/[?#]/)[0];
 if(!href.startsWith('/')||!Object.keys(routePrefixes).some(key=>source===key||source.startsWith(key+'/'))&&!['work','calendar','inbox','sales','customers','knowledge','pos','pms'].includes(source))return href;
 const base='/holdings/__route_scope';
 return canonicalUiHref(base+href).slice(base.length);
}
/** Only destination fields change; command payload shapes and API URLs stay intact. */
export function canonicalDestinationLinks<T>(value:T):T {
 if(Array.isArray(value))return value.map(item=>canonicalDestinationLinks(item)) as T;
 if(!value||typeof value!=='object'||Object.getPrototypeOf(value)!==Object.prototype)return value;
 return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,typeof item==='string'&&/^(?:href|\w*Href|\w*_href)$/.test(key)?canonicalDestinationLink(item):canonicalDestinationLinks(item)])) as T;
}
