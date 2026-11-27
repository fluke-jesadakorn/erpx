import {get} from "node:http";
import {NextRequest,NextResponse} from 'next/server';
import {holdingRoutingEnabled,holdingHostForHost,parseHoldingRoute,holdingUiUrl,holdingPathMatches,internalWorkspaceBase,companyAddressRouting} from '@erpx/feature-runtime/lib/holding-hosts';
import {product} from '@erpx/contracts/product-registry';
import {holdingBindingBySlug} from '@erpx/server-runtime/public/runtime';

const noStore={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Robots-Tag':'noindex, nofollow'};
export function holdingRewriteUrl(request:NextRequest,pathname:string,search=request.nextUrl.search){
  const target=new URL(request.url);
  // The pinned gateway binds each app to this exact loopback endpoint. NextURL
  // normalizes 127.0.0.1 to localhost, which its router treats as an external proxy.
  if(/^32\d\d$/.test(process.env.ERPX_APP_PORT??'')){target.hostname='127.0.0.1';target.port=process.env.ERPX_APP_PORT!;}
  target.pathname=pathname;target.search=search;return target;
}
function frontHoldingPath(request:NextRequest,holding:string,company:string,app:'pos'|'pms',branch?:string,department?:string){
  const visible=request.nextUrl.pathname.split('/').filter(Boolean),index=visible.indexOf(app),tail=visible.slice(index+1);
  const daily=app==='pos'?['sale','kitchen','history','shifts','notifications']:['front-desk','reservations','rooms','housekeeping','guest-accounts','cashier','night-audit','notifications'];
  if(tail.length&&!daily.includes(tail[0]))return null;
  const location=branch?`branches/${branch}${department?`/departments/${department}`:''}`:department?`departments/${department}`:'holding';
  return `/${holding}/${company}/${location}/${tail.join('/')||product(app)!.landing}`;
}
type NativeAddress = {companySlug:string;branchSlug:string|null;departmentSlug:string|null};
function readHoldingAddress(target:URL,host:string):Promise<{status:number;data?:NativeAddress}>{
  return new Promise((resolve,reject)=>{
    const outgoing=get(target,{headers:{host},signal:AbortSignal.timeout(5000)},response=>{
      const status=response.statusCode??503;
      if(status!==200){response.resume();resolve({status});return;}
      const chunks:Buffer[]=[];
      let length=0;
      response.on('data',(chunk:Buffer)=>{
        length+=chunk.length;
        if(length>8192){response.destroy(new Error('HOLDING_ADDRESS_BODY_LIMIT'));return;}
        chunks.push(chunk);
      });
      response.on('error',reject);
      response.on('end',()=>{
        try{resolve({status,data:JSON.parse(Buffer.concat(chunks).toString('utf8')).data});}catch(error){reject(error);}
      });
    });
    outgoing.on('error',reject);
  });
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
  if(request.nextUrl.pathname.startsWith('/holdings/')&&!/^\/holdings\/[^/]+\/(?:administration|erpx-management)(?:\/|$)/.test(request.nextUrl.pathname)&&['GET','HEAD'].includes(request.method)){
    const current=request.nextUrl.pathname+request.nextUrl.search;
    const mapped=holdingUiUrl(current,request.nextUrl.searchParams.get('company')??undefined);
    if(mapped){const target=new URL(mapped);if(target.pathname+target.search!==current)return NextResponse.redirect(mapped,{status:307,headers:noStore});}
  }
  const requestHeaders=new Headers(request.headers);
  for(const key of ['x-erpx-holding-slug','x-erpx-visible-path','x-erpx-front-company','x-erpx-front-app','x-erpx-route-company','x-erpx-plugin-id'])requestHeaders.delete(key);
  requestHeaders.set('x-erpx-holding-slug',registration.holdingSlug);
  requestHeaders.set('x-erpx-visible-path',request.nextUrl.pathname+request.nextUrl.search);
  let internalPath=route.internalPath;
  const company=route.companySlug??request.nextUrl.searchParams.get('company');
  let branchSlug=route.branchSlug;
  let departmentSlug=route.departmentSlug;
  if(route.kind==='page'&&!branchSlug&&!departmentSlug&&!company&&internalPath.startsWith('/holdings/'))internalPath='/product-entry';
  if(route.companySlug||branchSlug||departmentSlug||company&&(route.kind==='entry'||internalPath.startsWith('/holdings/'))){
    const unavailable=()=>new NextResponse('Workspace unavailable\n',{status:503,headers:noStore});
    const base=process.env.ERPX_HOLDING_ADDRESS_API??process.env.ERPX_DEV_NATIVE_API;
    if(!base)return unavailable();
    try{
      const target=new URL(base);
      if(target.protocol!=='http:'||target.hostname!=='127.0.0.1'||target.username||target.password)return unavailable();
      target.pathname=`/internal/organization/holdings/${encodeURIComponent(registration.holdingSlug)}/workspace-address`;
      target.search=new URLSearchParams({
        ...(company?{company}:{}),
        ...(route.workspaceSlug?{workspace:route.workspaceSlug}:{}),
        ...(branchSlug?{branch:branchSlug}:{}),
        ...(departmentSlug?{department:departmentSlug}:{}),
      }).toString();
      const {status,data}=await readHoldingAddress(target,registration.hostname);
      if(status===404)return missing();
      if(status!==200)return unavailable();
      if(typeof data?.companySlug!=='string'||route.companySlug&&data.companySlug!==route.companySlug)return missing();
      const previousBase=internalWorkspaceBase(registration.holdingSlug,branchSlug,departmentSlug);
      branchSlug=data.branchSlug??undefined;
      departmentSlug=data.departmentSlug??undefined;
      internalPath=internalWorkspaceBase(registration.holdingSlug,branchSlug,departmentSlug)+internalPath.slice(previousBase.length);
      requestHeaders.set('x-erpx-front-company',data.companySlug);
      requestHeaders.set('x-erpx-route-company',data.companySlug);
      if(route.kind==='entry')internalPath=`${internalWorkspaceBase(registration.holdingSlug)}/${product(route.app)?.landing??'administration/technology/organization'}`;
      if(route.app==='pos'||route.app==='pms') internalPath=frontHoldingPath(request,registration.holdingSlug,data.companySlug,route.app,branchSlug,departmentSlug)??internalPath;
    }catch{return unavailable();}
  }

  if(route.pluginId){
    const cookie=request.headers.get('cookie');
    if(request.cookies.has('__Host-erpx-product') && cookie && route.companySlug){
      const origin=`https://${registration.hostname}`;
      const workspace=internalWorkspaceBase(registration.holdingSlug,branchSlug,departmentSlug);
      try{
        const catalogue=await fetch(`${origin}/__erpx/apps/api/v1${workspace}/plugins?catalogue=0`,{
          headers:{cookie,origin,'x-erpx-requested-company':route.companySlug},cache:'no-store',signal:AbortSignal.timeout(5000),
        });
        if([403,404,409].includes(catalogue.status))return missing();
        if(catalogue.ok){
          const {data}=await catalogue.json() as {data?:{installed?:{id:string}[]}};
          if(!data?.installed?.some(item=>item.id===route.pluginId))return missing();
        }else if(catalogue.status!==401){
          return new NextResponse('Plugin unavailable\n',{status:503,headers:noStore});
        }
      }catch{
        return new NextResponse('Plugin unavailable\n',{status:503,headers:noStore});
      }
    }
    requestHeaders.set('x-erpx-plugin-id',route.pluginId);
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
    if(target.host===request.headers.get('host')&&['/login','/sso/start'].includes(target.pathname)
      &&target.searchParams.get('next')?.startsWith('/holdings/')&&prepared.headers.get('x-erpx-route-company')
      &&companyAddressRouting(prepared.headers.get('x-erpx-holding-slug')??'')){
      target.searchParams.set('next',request.nextUrl.pathname+request.nextUrl.search);
      response.headers.set('location',target.href);
    }
    if(/^\/(?:admin\/)?(?:launch|choose-work-role)$/.test(target.pathname)&&!target.searchParams.has('app')){target.searchParams.set('app',process.env.ERPX_APP_ID!);response.headers.set('location',target.href);}
  }
  if(response.headers.get('x-middleware-next')==='1'&&prepared.nextUrl.pathname!==request.nextUrl.pathname){response.headers.delete('x-middleware-next');response.headers.set('x-middleware-rewrite',prepared.url);}
  const rewrite=response.headers.get('x-middleware-rewrite');
  if(rewrite){const target=new URL(rewrite);response.headers.set('x-middleware-rewrite',holdingRewriteUrl(request,target.pathname,target.search).href);}
  for(const [key,value]of Object.entries(noStore))response.headers.set(key,value);
  return response;
}
