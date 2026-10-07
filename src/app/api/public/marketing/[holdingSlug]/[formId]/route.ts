import {submitPublicMarketingForm,publicMarketingSubmissionSchema} from "@/server/modules/marketing/public/public-submission";
import {holdingBindingBySlug,holdingResources} from "@/server/modules/runtime/public/runtime";
import {assertProductionModuleEnabled} from "@/server/modules/pms/public/hospitality-rollout";
import {requestPublicOrigin} from "@/server/modules/manage_erpx/public/public-origin";
import {ApiError,errorResponse} from "@/server/modules/runtime/public/http";
import {loadPublicMarketingForm} from "@/server/modules/marketing/public/marketing-public-form";
type Context={params:Promise<{holdingSlug:string;formId:string}>};
async function form(context:Context){const {holdingSlug,formId}=await context.params,binding=await holdingBindingBySlug(holdingSlug),{write}=await holdingResources(binding.id),row=await loadPublicMarketingForm(write,formId);await assertProductionModuleEnabled(write,row.company_id,'marketing');return {write,row};}
export async function GET(_request:Request,context:Context){try{const {row}=await form(context);return Response.json({data:{id:row.id,name:row.name,version:row.version,preview:JSON.parse(row.preview_json),presentation:row.presentation,consentRequired:true}},{headers:{'Cache-Control':'no-store'}});}catch(e){return errorResponse(e);}}
export async function POST(request:Request,context:Context){try{
 if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the form website.');
 const key=request.headers.get('Idempotency-Key');if(!key||!/^[A-Za-z0-9:_.-]{8,180}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
 const bodyText=await request.text();if(Buffer.byteLength(bodyText)>12000)throw new ApiError(413,'BODY_TOO_LARGE','Form submission is too large.');
 let parsed:unknown;try{parsed=JSON.parse(bodyText);}catch{throw new ApiError(422,'INVALID_JSON','Invalid form submission.');}
 const body=publicMarketingSubmissionSchema.parse(parsed),{write,row}=await form(context);
 const ip=request.headers.get('cf-connecting-ip')??request.headers.get('x-forwarded-for')?.split(',')[0]??'local';
 const result=await submitPublicMarketingForm(write,row,body,key,ip);
 return Response.json({data:result},{status:201,headers:{'Cache-Control':'no-store'}});
 }catch(e){return errorResponse(e);}}
