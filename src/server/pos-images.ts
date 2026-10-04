import 'server-only';
import sharp from 'sharp';
import { createHash } from 'node:crypto';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import { requirePermission } from './iam-permissions';
import { PostgresDatabase } from './runtime/postgres';
import { one,run,posCommand,fail } from './pos-core';
import { readBoundedUploadBody } from './attachments/upload';
import { matchesUploadSignature } from './upload-validation';
import { createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,scanAttachmentNow } from './attachments/service';
import { requirePosImageSource } from './pos-image-source';
import { authorizePosProductImageTarget } from './attachments/policy';

export async function savePosProductImage(actor:WorkContext,productId:string,bytes:Uint8Array,mime:string,source:'generated'|'upload',key:string,expectedVersion:number,metadata:Record<string,unknown>={}) {
 requirePermission(actor,'pos.configure');
 if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key)||!Number.isSafeInteger(expectedVersion)||expectedVersion<1)fail('POS_IMAGE_INPUT','Use a command key and current catalog version',422);
 if(!['image/png','image/jpeg','image/webp'].includes(mime)||!matchesUploadSignature(mime,bytes)||bytes.byteLength>10*1024*1024)fail('POS_IMAGE_FORMAT','Use a PNG, JPEG or WebP image up to 10 MB',422);
 const {write,files}=await holdingResources(actor.holdingId);
 const catalog=await one(write,'SELECT product_id,version FROM pos_catalog WHERE product_id=? AND company_id=?',productId,actor.companyId);
 if(!catalog)fail('POS_PRODUCT_INVALID','Save the product catalog first',422);
 if(actor.departmentId||actor.branchId&&!await one(write,'SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?',productId,actor.branchId,actor.companyId))fail('POS_PRODUCT_INVALID','Catalog is outside this Branch',404);
 const digest=createHash('sha256').update(bytes).digest('hex'),imageId=`pos-image-${createHash('sha256').update(`${actor.companyId}:${productId}:${digest}`).digest('hex')}`;
 const prior=await one(write,'SELECT attachment_source_id FROM pos_product_images WHERE id=? AND company_id=? AND product_id=?',imageId,actor.companyId,productId);
 if(prior?.attachment_source_id)return posCommand(write as PostgresDatabase,actor,'catalog.image',{productId,imageId,expectedVersion},key);
 if(Number(catalog.version)!==expectedVersion)fail('POS_VERSION_CONFLICT','Product data changed');
 await authorizePosProductImageTarget(write,actor,productId,'write');
 const image=sharp(bytes,{limitInputPixels:25000000,animated:false}),info=await image.metadata();
 if(!info.width||!info.height||info.pages&&info.pages>1)fail('POS_IMAGE_FORMAT','Use a single static image',422);
 const product=await one(write,'SELECT version FROM commerce_products WHERE id=? AND company_id=?',productId,actor.companyId);
 if(!product)fail('POS_PRODUCT_INVALID','Product not found',404);
 const options={compatibilityAdapter:true};
 const set=await createAttachmentSet(write,actor,{targetType:'product',targetId:productId,componentKind:'record',componentRef:imageId,
   purposeKey:'product_image',expectedTargetVersion:Number(product.version)},`${key}:image-set`,options);
 const reserved=await reserveAttachmentFile(write,actor,set.id,{filename:`${imageId}.${mime==='image/png'?'png':mime==='image/webp'?'webp':'jpg'}`,
   mediaType:mime,bytes:bytes.byteLength},`${key}:image-file`,options);
 const uploaded=await completeAttachmentUpload(write,files,actor,reserved.uploadIntentId,bytes.slice().buffer,options);
 const state=await scanAttachmentNow(write,files,uploaded.sourceId);
 if(state!=='available')fail(state==='malware_rejected'?'POS_IMAGE_REJECTED':'POS_IMAGE_SCAN_PENDING','The image must pass file scanning before it can be used',409);
 const available=await one(write,"SELECT object_key,preview_object_key,preview_checksum_sha256 FROM attachments WHERE id=? AND company_id=? AND source_state='available'",uploaded.sourceId,actor.companyId);
 if(!available?.preview_object_key)fail('POS_IMAGE_PREVIEW_UNAVAILABLE','A safe image preview could not be generated',409);
 await run(write,`INSERT INTO pos_product_images(id,company_id,product_id,object_key,thumbnail_key,sha256,width,height,mime_type,source,metadata_json,created_by,created_at,
   attachment_source_id,attachment_link_id,attachment_set_id,thumbnail_checksum_sha256) VALUES(?,?,?,?,?,?,?,?,?,?,?::jsonb,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING`,
   imageId,actor.companyId,productId,available.object_key,available.preview_object_key,digest,info.width,info.height,mime,source,JSON.stringify(metadata),actor.userId,Date.now(),
   uploaded.sourceId,reserved.attachmentLinkId,set.id,available.preview_checksum_sha256);
 return posCommand(write as PostgresDatabase,actor,'catalog.image',{productId,imageId,expectedVersion},key);
}

export async function uploadPosProductImage(request:Request,actor:WorkContext) {
 requirePermission(actor,'pos.configure');
 const length=Number(request.headers.get('content-length'));
 const bytes=new Uint8Array(await readBoundedUploadBody(request,length));
 return savePosProductImage(actor,request.headers.get('x-pos-product')??'',bytes,request.headers.get('content-type')??'','upload',request.headers.get('Idempotency-Key')??'',Number(request.headers.get('x-pos-catalog-version')));
}

export async function posProductImage(actor:WorkContext,imageId:string,original=false) {
 requirePermission(actor,'pos.read');
 const {write,files}=await holdingResources(actor.holdingId);
 const row=await requirePosImageSource(write,actor,imageId);
 const file=await files.get(original?row.object_key:row.thumbnail_key);
 if(!file)fail('POS_IMAGE_MISSING','Image bytes are unavailable',404);
 const buffer=await file.arrayBuffer(),checksum=createHash('sha256').update(new Uint8Array(buffer)).digest('hex');
 if(checksum!==(original?row.sha256:row.thumbnail_checksum_sha256))fail('POS_IMAGE_CHECKSUM_MISMATCH','Product image integrity check failed',409);
 await run(write,"INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,'content.read',?::jsonb,?)",
   crypto.randomUUID(),actor.companyId,row.attachment_source_id,row.attachment_link_id,row.attachment_set_id,actor.userId,JSON.stringify({owner:'pos_product_image',variant:original?'original':'preview'}),Date.now());
 return new Response(buffer,{headers:{'Content-Type':original?row.mime_type:'image/webp','Content-Length':String(file.size),'Cache-Control':'private, no-store','X-Content-Type-Options':'nosniff','Content-Security-Policy':"default-src 'none'"}});
}
