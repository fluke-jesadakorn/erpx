import 'server-only';
import sharp from 'sharp';
import { createHash } from 'node:crypto';
import type { WorkContext } from './authorization';
import { holdingResources } from './runtime';
import { requirePermission } from './iam-permissions';
import { PostgresDatabase } from './runtime/postgres';
import { one,run,posCommand,fail,str } from './pos-core';
import { readBoundedUploadBody } from './attachments/upload';
import { matchesUploadSignature } from './upload-validation';

export async function savePosProductImage(actor:WorkContext,productId:string,bytes:Uint8Array,mime:string,source:'generated'|'upload',key:string,expectedVersion:number,metadata:Record<string,unknown>={}) {
 requirePermission(actor,'pos.configure');
 if(!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key)||!Number.isSafeInteger(expectedVersion)||expectedVersion<1)fail('POS_IMAGE_INPUT','Use a command key and current catalog version',422);
 if(!['image/png','image/jpeg','image/webp'].includes(mime)||!matchesUploadSignature(mime,bytes)||bytes.byteLength>10*1024*1024)fail('POS_IMAGE_FORMAT','Use a PNG, JPEG or WebP image up to 10 MB',422);
 const {write,files}=await holdingResources(actor.holdingId);
 if(!await one(write,'SELECT product_id FROM pos_catalog WHERE product_id=? AND company_id=?',productId,actor.companyId))fail('POS_PRODUCT_INVALID','Save the product catalog first',422);
 if(actor.departmentId||actor.branchId&&!await one(write,'SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?',productId,actor.branchId,actor.companyId))fail('POS_PRODUCT_INVALID','Catalog is outside this Branch',404);
 const digest=createHash('sha256').update(bytes).digest('hex'),imageId=`pos-image-${createHash('sha256').update(`${actor.companyId}:${productId}:${digest}`).digest('hex')}`;
 const objectKey=`pos/${actor.companyId}/${productId}/${digest}/original`,thumbnailKey=`pos/${actor.companyId}/${productId}/${digest}/thumbnail.webp`;
 const image=sharp(bytes,{limitInputPixels:25000000,animated:false}),info=await image.metadata();
 if(!info.width||!info.height||info.pages&&info.pages>1)fail('POS_IMAGE_FORMAT','Use a single static image',422);
 const thumbnail=await image.rotate().resize(480,480,{fit:'cover'}).webp({quality:82}).toBuffer();
 await files.put(objectKey,bytes,{onlyIf:{etagDoesNotMatch:'*'},httpMetadata:{contentType:mime},customMetadata:{companyId:actor.companyId,productId,source,sha256:digest}});
 await files.put(thumbnailKey,thumbnail,{onlyIf:{etagDoesNotMatch:'*'},httpMetadata:{contentType:'image/webp'},customMetadata:{companyId:actor.companyId,productId,source,sha256:digest}});
 await run(write,'INSERT INTO pos_product_images(id,company_id,product_id,object_key,thumbnail_key,sha256,width,height,mime_type,source,metadata_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?::jsonb,?,?) ON CONFLICT(id) DO NOTHING',imageId,actor.companyId,productId,objectKey,thumbnailKey,digest,info.width,info.height,mime,source,JSON.stringify(metadata),actor.userId,Date.now());
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
 const row=await one(write,'SELECT * FROM pos_product_images WHERE id=? AND company_id=?',imageId,actor.companyId);
 if(!row||actor.departmentId)fail('POS_IMAGE_NOT_FOUND','Image not found',404);
 if(actor.branchId&&!await one(write,'SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.company_id=? AND r.branch_id=? AND rc.enabled=true',row.product_id,actor.companyId,actor.branchId))fail('POS_IMAGE_NOT_FOUND','Image not found in this Branch',404);
 const file=await files.get(str(row,original?'object_key':'thumbnail_key'));
 if(!file)fail('POS_IMAGE_MISSING','Image bytes are unavailable',404);
 return new Response(file.body,{headers:{'Content-Type':original?str(row,'mime_type'):'image/webp','Content-Length':String(file.size),'Cache-Control':'private, max-age=3600','X-Content-Type-Options':'nosniff','Content-Security-Policy':"default-src 'none'"}});
}
