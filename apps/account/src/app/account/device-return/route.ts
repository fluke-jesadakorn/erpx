import {NextResponse} from 'next/server';
import {deviceAccountReturn} from '@erpx/feature-identity_access/lib/device-account-return';
import {configuredProductOrigin,configuredProductHandoff,productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {holdingOrigin,holdingRoutingEnabled} from '@erpx/feature-runtime/lib/holding-hosts';
import {isProductHostKey} from '@erpx/contracts/product-registry';
import {sessionFromRequest} from '@erpx/server-identity_access/public/account-session';
import {errorResponse} from '@erpx/server-runtime/public/http';
export const dynamic='force-dynamic';
const headers={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff'};
/** UI navigation only. Rust verifies the Account session and the subsequent
 * SSO transition; this route cannot issue a credential or select a role. */
export async function GET(request:Request){
 try{
  const accountOrigin=configuredProductOrigin('account');
  if(!accountOrigin||productHostForHost(request.headers.get('host'))!=='account')return new Response(null,{status:404,headers});
  const url=new URL(request.url),target=deviceAccountReturn(url.pathname+url.search);
  if(!target)return new Response('Invalid account return',{status:400,headers});
  if(!await sessionFromRequest(request))return NextResponse.redirect(new URL('/login?'+new URLSearchParams({next:target}),accountOrigin),{status:303,headers});
  const app=url.searchParams.get('app')!;if(!isProductHostKey(app))return new Response(null,{status:404,headers});
  const next='/account/continue?'+new URLSearchParams({app,next:new URL(target,accountOrigin).searchParams.get('next')!});
  const holding=url.searchParams.get('holding'),origin=holding&&holdingRoutingEnabled(process.env)?holdingOrigin(holding):null;
  const destination=origin?new URL('/sso/start?'+new URLSearchParams({next}),origin):configuredProductHandoff(app,next);
  if(!destination)return new Response(null,{status:404,headers});
  return NextResponse.redirect(destination,{status:303,headers});
 }catch(error){return errorResponse(error);}
}
