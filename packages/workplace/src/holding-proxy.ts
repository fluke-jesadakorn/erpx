import {NextRequest,NextResponse} from 'next/server';
import {holdingRoutingEnabled,holdingHostForHost,parseHoldingRoute,holdingUiUrl,holdingPathMatches,internalWorkspaceBase} from '@erpx/feature-runtime/lib/holding-hosts';
import {product} from '@erpx/contracts/product-registry';
import {holdingBindingBySlug,holdingResources} from '@erpx/server-runtime/public/runtime';
import {workspaceRouteCompany} from '@erpx/server-organization/public/workspace-route-company';

const noStore={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Robots-Tag':'noindex, nofollow'};
export function holdingRewriteUrl(request:NextRequest,pathname:string,search=request.nextUrl.search){
  const target=new URL(request.url);
  // The pinned gateway binds each app to this exact loopback endpoint. NextURL
  // normalizes 127.0.0.1 to localhost, which its router treats as an external proxy.
  if(/^32\d\d$/.test(process.env.ERPX_APP_PORT??'')){target.hostname='127.0.0.1';target.port=process.env.ERPX_APP_PORT!;}
  target.pathname=pathname;target.search=search;return target;
}
export async function prepareHoldingRequest(request:NextRequest):Promise<NextRequest|NextResponse|null>{
  const registration=holdingRoutingEnabled(process.env)&&holdingHostForHost(request.headers.get('host'));
  if(!registration)return null;
  const missing=()=>new NextResponse('Not Found\n',{status:404,headers:noStore});
  const route=parseHoldingRoute(registration.holdingSlug,request.nextUrl.pathname);
  // Launch helpers are routed to their own app by the gateway.
  const app=process.env.ERPX_APP_ID;
  if(!route||!holdingPathMatches(registration.holdingSlug,request.nextUrl.pathname))return missing();
  if(route.kind!=='system'&&route.app!==app)return missing();
  if(request.nextUrl.pathname.startsWith('/provider')&&registration.holdingSlug!=='erpx')return missing();
  if(registration.holdingSlug!=='erpx'&&/^\/(?:__erpx\/admin\/)?(?:api\/v1\/platform|provider|platform)(?:\/|$)/.test(request.nextUrl.pathname))return missing();
  let binding:Awaited<ReturnType<typeof holdingBindingBySlug>>;
  try{binding=await holdingBindingBySlug(registration.holdingSlug);}catch(error){if(error instanceof Error&&error.message==='HOLDING_NOT_FOUND')return missing();throw error;}
  if(binding.id!==registration.holdingId)return missing();
  if(route.kind==='transport'&&route.internalPath.startsWith('/_next/'))return NextResponse.next();
  if(request.nextUrl.pathname.startsWith('/holdings/')&&['GET','HEAD'].includes(request.method)){
    const mapped=holdingUiUrl(request.nextUrl.pathname+request.nextUrl.search,request.nextUrl.searchParams.get('company')??undefined);
    if(mapped)return NextResponse.redirect(mapped,{status:307,headers:noStore});
  }
  const requestHeaders=new Headers(request.headers);
  for(const key of ['x-erpx-holding-slug','x-erpx-visible-path','x-erpx-front-company','x-erpx-front-app','x-erpx-route-company'])requestHeaders.delete(key);
  requestHeaders.set('x-erpx-holding-slug',registration.holdingSlug);
  requestHeaders.set('x-erpx-visible-path',request.nextUrl.pathname+request.nextUrl.search);
  let internalPath=route.internalPath;
  const company=request.nextUrl.searchParams.get('company');
  if(route.kind==='page'&&!route.branchSlug&&!route.departmentSlug&&!company&&internalPath.startsWith('/holdings/'))internalPath='/product-entry';
  if(route.branchSlug||route.departmentSlug||company&&(route.kind==='entry'||internalPath.startsWith('/holdings/'))){
    const {write}=await holdingResources(binding.id);
    const structural=await workspaceRouteCompany(write,route.branchSlug,route.departmentSlug,company??undefined);
    if(!structural)return missing();
    requestHeaders.set('x-erpx-front-company',structural.slug);
    requestHeaders.set('x-erpx-route-company',structural.slug);
    if(route.kind==='entry')internalPath=`${internalWorkspaceBase(registration.holdingSlug)}/${product(route.app)?.landing??'administration/technology/organization'}`;
    if(route.app==='pos'||route.app==='pms'){
      const visible=request.nextUrl.pathname.split('/').filter(Boolean),index=visible.indexOf(route.app),tail=visible.slice(index+1);
      const daily=route.app==='pos'?['sale','kitchen','history','shifts','notifications']:['front-desk','reservations','rooms','housekeeping','guest-accounts','cashier','night-audit','notifications'];
      if(!tail.length||daily.includes(tail[0])){
        const location=route.branchSlug?`branches/${route.branchSlug}${route.departmentSlug?`/departments/${route.departmentSlug}`:''}`:route.departmentSlug?`departments/${route.departmentSlug}`:'holding';
        internalPath=`/${registration.holdingSlug}/${structural.slug}/${location}/${tail.join('/')||product(route.app)!.landing}`;
      }
    }
  }
  const internal=new URL(request.url);internal.pathname=internalPath;
  return new NextRequest(internal,{method:request.method,headers:requestHeaders});
}
/** Carry Proxy's scoped headers through the final rewrite; never rewrite HTML. */
export function finishHoldingResponse(request:NextRequest,prepared:NextRequest,response:NextResponse){
  if(!holdingRoutingEnabled(process.env)||!holdingHostForHost(request.headers.get('host')))return response;
  const location=response.headers.get('location');
  if(location){
    const target=new URL(location,request.url),mapped=holdingUiUrl(target.pathname+target.search,prepared.headers.get('x-erpx-route-company')??request.nextUrl.searchParams.get('company')??undefined);
    if(mapped&&target.host===request.headers.get('host'))response.headers.set('location',mapped);
    if(/^\/(?:admin\/)?(?:launch|choose-work-role)$/.test(target.pathname)&&!target.searchParams.has('app')){target.searchParams.set('app',process.env.ERPX_APP_ID!);response.headers.set('location',target.href);}
  }
  if(response.headers.get('x-middleware-next')==='1'&&prepared.nextUrl.pathname!==request.nextUrl.pathname){response.headers.delete('x-middleware-next');response.headers.set('x-middleware-rewrite',prepared.url);}
  const rewrite=response.headers.get('x-middleware-rewrite');
  if(rewrite){const target=new URL(rewrite);response.headers.set('x-middleware-rewrite',holdingRewriteUrl(request,target.pathname,target.search).href);}
  for(const [key,value]of Object.entries(noStore))response.headers.set(key,value);
  return response;
}
