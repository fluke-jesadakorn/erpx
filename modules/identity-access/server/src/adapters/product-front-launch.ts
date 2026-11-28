import {requestProductOrigin} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {holdingRoutingEnabled,holdingFrontUiUrl} from '@erpx/platform-runtime-ui/lib/holding-hosts';
import 'server-only';
import {NextResponse} from 'next/server';
import {productHostForHost,configuredProductOrigin} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {frontProductLaunchTarget} from '@erpx/work-projection-ui/lib/product-navigation';
import {frontCompanyApp} from '@erpx/work-projection-server/public/front-app';
import {ApiError,errorResponse} from '@erpx/platform-runtime-server/public/http';
const headers={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer'};
/** Read-only host-bound Front launch. Original Front role/context and owner
 * readers still authorize the destination. Never changes the current role. */
export async function productFrontLaunch(request:Request){
 try{
  const app=productHostForHost(request.headers.get('host'));
  if(app!=='pos'&&app!=='pms')throw new ApiError(400,'INVALID_PRODUCT_WORKSPACE','Select a Front product.');
  if(request.method!=='GET')return new Response('Method not allowed',{status:405,headers:{...headers,Allow:'GET'}});
  const query=new URL(request.url).searchParams;
  if(query.getAll('companyId').length!==1||query.getAll('next').length!==1)throw new ApiError(422,'INVALID_PRODUCT_WORKSPACE','Select one Company and destination.');
  const companyId=query.get('companyId'),target=frontProductLaunchTarget(app,query.get('next'));
  if(!companyId||companyId.length>128||!target)throw new ApiError(400,'INVALID_PRODUCT_WORKSPACE','Select a valid Front workspace.');
  const entry=await frontCompanyApp(target.holdingSlug,target.companySlug,app);
  if(!entry||entry.companyId!==companyId||!entry.locations.some(location=>location.path===target.location))throw new ApiError(403,'ASSIGNMENT_UNAVAILABLE','Your current account cannot open this Company and work location.');
  return NextResponse.redirect(new URL(holdingRoutingEnabled(process.env)?holdingFrontUiUrl(target.target,app)??target.target:target.target,requestProductOrigin(app,request.headers)!),{status:303,headers});
 }catch(error){const response=errorResponse(error);for(const [key,value] of Object.entries(headers))response.headers.set(key,value);return response;}
}
