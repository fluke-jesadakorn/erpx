import {NextResponse} from 'next/server';
import {productFrontLaunch} from '@erpx/server-identity_access/public/product-front-launch';
import {productHostForHost,configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {productWorkspaceRequest,productWorkspaceTarget,productWorkRolePath} from '@erpx/server-identity_access/public/product-workspace';
export const dynamic='force-dynamic';
async function launch(request:Request){
 const audience=productHostForHost(request.headers.get('host'));
 const query=new URL(request.url).searchParams;
 const workspace=audience&&productWorkspaceTarget(audience,query.get('companyId'),query.get('next'));
 const front=(audience==='pos'||audience==='pms')&&!workspace&&!(request.method==='POST'&&request.headers.get('content-type')?.startsWith('application/x-www-form-urlencoded'));
 const response=await (front?productFrontLaunch(request):productWorkspaceRequest(request));
 // A browser uses the existing role/package recovery page. JSON callers and
 // POST failures retain their original status and never cross-host redirect.
 if(request.method==='GET'&&request.headers.get('accept')?.includes('text/html')&&[403,404].includes(response.status)&&audience){
  const query=new URL(request.url).searchParams,input=productWorkspaceTarget(audience,query.get('companyId'),query.get('next'));
  const origin=configuredProductOrigin(audience);
  if(input&&origin)return NextResponse.redirect(new URL(productWorkRolePath(input,'access-changed'),origin),{status:303,headers:{'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer'}});
 }
 return response;
}
export const GET=launch;
export const POST=launch;
