import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import {submitPublicMarketingForm,submitVersionedPublicMarketingForm,loadPublicMarketingSubmissionForm,publicMarketingSubmissionSchema} from "@erpx/marketing-server/public/public-submission";
import {productHostForHost} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {holdingBindingBySlug,holdingResources} from "../../../../../../../../../../platform/runtime/server/src/public/runtime";
import {requestPublicOrigin} from "@erpx/manage-erpx-server/public/public-origin";
import {ApiError,errorResponse} from "@erpx/platform-runtime-server/public/http";
import {loadPublicMarketingForm} from "@erpx/marketing-server/public/marketing-public-form";
import {readBoundedPublicMarketingBody} from "@erpx/marketing-server/public/public-form-body";
type Context={params:Promise<{holdingSlug:string;formId:string}>};
async function form(context:Context,submissionKey?:string){const {holdingSlug,formId}=await context.params,binding=await holdingBindingBySlug(holdingSlug),{write}=await holdingResources(binding.id),row=submissionKey?await loadPublicMarketingSubmissionForm(write,holdingSlug,formId,submissionKey):await loadPublicMarketingForm(write,holdingSlug,formId);return {write,row};}
export async function GET(_request:Request,context:Context){try{const {row}=await form(context);return Response.json(canonicalDestinationLinks({data:{id:row.id,name:row.name,version:row.version,preview:JSON.parse(row.preview_json),presentation:row.presentation,consentRequired:true}}),{headers:{'Cache-Control':'no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(request:Request,context:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the form website.');
 const key=request.headers.get('Idempotency-Key');if(!key||!/^[A-Za-z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 // Allow UTF-8 and JSON escaping at the displayed field limits without accepting an unbounded request.
 const bodyText=await readBoundedPublicMarketingBody(request);
 let parsed:unknown;try{parsed=JSON.parse(bodyText);}catch{throw new ApiError(422,'INVALID_JSON','Invalid form submission.');}
 const body=publicMarketingSubmissionSchema.parse(parsed),{write,row}=await form(context,key);
 const ip=request.headers.get('cf-connecting-ip')??request.headers.get('x-forwarded-for')?.split(',')[0]??'local';
 const submit=productHostForHost(request.headers.get('host'))==='forms'?submitVersionedPublicMarketingForm:submitPublicMarketingForm,result=await submit(write,row,body,key,ip);
 return Response.json(canonicalDestinationLinks({data:result}),{status:201,headers:{'Cache-Control':'no-store'}});
 }catch(e){return errorResponse(e);}}
