import 'server-only';
import {configuredProductOrigin,productHostForHost} from '@/lib/runtime/product-hosts';
import {ApiError} from '../../runtime/public/http';
import {currentSession} from './authorization';
import {accountWorkspaceTarget,accountWorkspaceLoginReturn} from '../domain/account-workspace';

const headers={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff'};
const redirect=(location:URL)=>new Response(null,{status:303,headers:{...headers,Location:location.href}});
/** Account is the central session origin, so it does not create a child SSO
 * transaction. This GET bridge retains an exact self-page fragment through
 * the normal sanitized login return while each destination repeats policy. */
export async function accountWorkspaceRequest(request:Request){
 const origin=configuredProductOrigin('account'),url=new URL(request.url);
 if(!origin||productHostForHost(request.headers.get('host'))!=='account'||url.origin!==origin||request.headers.get('host')!==url.host||url.pathname!=='/account/launch')return new Response('Not found',{status:404,headers});
 if(request.method!=='GET')return new Response('Method not allowed',{status:405,headers:{...headers,Allow:'GET'}});
 const target=url.searchParams.getAll('next').length===1&&[...url.searchParams.keys()].every(key=>key==='next')?accountWorkspaceTarget(url.searchParams.get('next')):null;
 if(!target)return new Response('Invalid account destination',{status:422,headers});
 try{await currentSession();}catch(error){
  if(!(error instanceof Error&&error.message==='UNAUTHENTICATED')&&!(error instanceof ApiError&&error.status===401))throw error;
  const next=accountWorkspaceLoginReturn(target);
  if(!next)return new Response('Invalid account destination',{status:422,headers});
  const login=new URL('/login',origin);login.searchParams.set('next',next);return redirect(login);
 }
 return redirect(new URL(target,origin));
}
