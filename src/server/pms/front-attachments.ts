import 'server-only';
import {z} from 'zod';
import type {WorkContext} from '../authorization';
import {ApiError} from '../http';
import {holdingResources} from '../runtime';
import {requirePermission} from '../iam-permissions';
import {requestPublicOrigin} from '../public-origin';
import {readBoundedUploadBody} from '../attachments/upload';
import {attachmentContent,attachmentLinkStatus,bindRecordAttachmentSet,completeAttachmentUpload,createAttachmentSet,listAttachmentSets,reserveAttachmentFile,withdrawAttachmentLink} from '../attachments/service';

const target=z.enum(['pms_task','pms_guest','pms_reservation','pms_stay','pms_lost_property']);
const create=z.object({targetType:target,targetId:z.string().min(1).max(180),componentKind:z.enum(['record','action']),componentRef:z.string().max(180).optional(),purposeKey:z.string().min(1).max(80),expectedTargetVersion:z.number().int().positive()});
export const frontPmsAttachmentPaths=new Set(['attachment-sets','attachment-links','uploads']);

/** Narrow Front adapter. All target, revision, file and branch guards remain with Files. */
export async function frontPmsAttachmentRequest(request:Request,actor:WorkContext,path:string[]){
 requirePermission(actor,'pms.read');
 const {write,files}=await holdingResources(actor.holdingId),[kind,id,action]=path;
 const query=new URL(request.url).searchParams;
 const reply=(data:unknown,status=200)=>Response.json({data},{status,headers:{'Cache-Control':'private, no-store'}});
 const key=()=>{const value=request.headers.get('Idempotency-Key');if(!value||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(value))throw new ApiError(422,'COMMAND_KEY_REQUIRED','A valid Idempotency-Key is required.');return value;};
 if(request.method!=='GET'&&request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
 async function setAllowed(setId:string){
  const row=await write.prepare('SELECT target_type FROM attachment_sets WHERE id=? AND company_id=?').bind(setId,actor.companyId).first<{target_type:string}>();
  if(!row||!target.safeParse(row.target_type).success)throw new ApiError(404,'NOT_FOUND','Hotel file set not found.');
 }
 if(kind==='attachment-sets'&&path.length===1){
  if(request.method==='GET'){
   const value=create.pick({targetType:true,targetId:true}).extend({purposeKey:z.string().optional(),componentKind:z.enum(['record','action']).optional(),expectedTargetVersion:z.coerce.number().int().positive().optional()}).parse(Object.fromEntries(query));
   return reply(await listAttachmentSets(write,actor,value));
  }
  if(request.method==='POST')return reply(await createAttachmentSet(write,actor,create.parse(await request.json()),key()),201);
 }
 if(kind==='attachment-sets'&&id&&path.length===3&&request.method==='POST'){
  await setAllowed(id);
  if(action==='files'){
   const value=z.object({filename:z.string().min(1).max(255),mediaType:z.string().min(1).max(100),bytes:z.number().int().positive().max(10*1024*1024),caption:z.string().max(1000).optional(),supersedesLinkId:z.string().min(1).max(180).optional()}).parse(await request.json());
   const result=await reserveAttachmentFile(write,actor,id,value,key());
   const url=new URL(request.url);url.search='';url.pathname=url.pathname.replace(/\/attachment-sets\/[^/]+\/files$/,`/uploads/${encodeURIComponent(result.uploadIntentId)}/content`);
   return reply({...result,uploadUrl:url.pathname},201);
  }
  if(action==='bind'){
   const value=z.object({expectedTargetVersion:z.number().int().positive()}).parse(await request.json());
   return reply(await bindRecordAttachmentSet(write,actor,id,value.expectedTargetVersion,key()));
  }
 }
 if(kind==='attachment-links'&&id&&path.length===3){
  const row=await write.prepare('SELECT l.set_id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id WHERE l.id=? AND s.company_id=?').bind(id,actor.companyId).first<{set_id:string}>();
  if(!row)throw new ApiError(404,'NOT_FOUND','Hotel file link not found.');
  await setAllowed(row.set_id);
  if(action==='status'&&request.method==='GET')return reply(await attachmentLinkStatus(write,actor,id));
  if(action==='withdraw'&&request.method==='POST')return reply(await withdrawAttachmentLink(write,actor,id,z.object({reason:z.string().min(3).max(1000)}).parse(await request.json()).reason,key()));
  if(action==='content'&&request.method==='GET'){
   const variant=z.enum(['original','preview']).parse(query.get('variant')??'original');
   const result=await attachmentContent(write,files,actor,id,variant),headers=new Headers({'Content-Type':result.mediaType,'Content-Disposition':`${result.inline?'inline':'attachment'}; filename*=UTF-8''${encodeURIComponent(result.inline?`preview-${result.filename}`:result.filename)}`,'Cache-Control':'private, no-store','X-Content-Type-Options':'nosniff','Accept-Ranges':'none'});
   if(result.inline)headers.set('Content-Security-Policy',"sandbox; default-src 'none'");
   return new Response(result.object.body,{headers});
  }
 }
 if(kind==='uploads'&&id&&action==='content'&&path.length===3&&request.method==='PUT'){
  const row=await write.prepare('SELECT attachment_set_id,declared_bytes,media_type FROM upload_intents WHERE id=? AND company_id=? AND created_by=?').bind(id,actor.companyId,actor.userId).first<{attachment_set_id:string;declared_bytes:number;media_type:string}>();
  if(!row?.attachment_set_id)throw new ApiError(404,'UPLOAD_NOT_FOUND','Hotel upload not found.');
  await setAllowed(row.attachment_set_id);
  if(request.headers.get('content-type')?.split(';')[0].trim().toLowerCase()!==row.media_type)throw new ApiError(422,'UPLOAD_TYPE','Content-Type must match the reserved file type.');
  return reply(await completeAttachmentUpload(write,files,actor,id,await readBoundedUploadBody(request,Number(row.declared_bytes))));
 }
 throw new ApiError(404,'NOT_FOUND','Hotel file action not found.');
}
