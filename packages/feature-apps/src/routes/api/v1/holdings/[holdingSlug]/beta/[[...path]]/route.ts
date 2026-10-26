import {workSocialBetaPathAllowed} from '@erpx/contracts/work-social-api';
import {productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {betaHandler} from '@erpx/server-manage_erpx/public/beta-api';

const native=betaHandler('beta');
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export async function workSocialRequest(request:Request,context:Context){
 const{path=[]}=await context.params;
 if(productHostForHost(request.headers.get('host'))!=='apps'||!workSocialBetaPathAllowed(path,request.method))return Response.json({error:{code:'NOT_FOUND',message:'Resource unavailable.'}},{status:404,headers:{'Cache-Control':'private, no-store'}});
 // HEAD has GET semantics; the framework removes its response body. The native
 // command dispatcher must never interpret it as a mutation.
 return native(request.method==='HEAD'?new Request(request,{method:'GET'}):request,context);
}
export const GET=workSocialRequest;
export const POST=workSocialRequest;
