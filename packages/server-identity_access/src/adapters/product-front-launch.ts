import 'server-only';
import {NextResponse} from 'next/server';
import {productHostForHost,configuredProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {frontProductLaunchTarget} from '@erpx/feature-work_projection/lib/product-navigation';
import {frontCompanyApp} from '@erpx/server-work_projection/public/front-app';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
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
  return NextResponse.redirect(new URL(target.target,configuredProductOrigin(app)!),{status:303,headers});
 }catch(error){const response=errorResponse(error);for(const [key,value] of Object.entries(headers))response.headers.set(key,value);return response;}
}
