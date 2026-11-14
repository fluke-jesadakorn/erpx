// Executes only against the isolated synthetic Account session and leased Holding.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import {createHash} from 'node:crypto';
const auth=JSON.parse(fs.readFileSync('/case/account-proof-maker.json','utf8'));
const original=fs.readFileSync('/case/png-alpha.input');
const context={'host':'acme.jesadakorn.com','origin':'https://acme.jesadakorn.com','cookie':auth.cookie,'x-erpx-context-company':'company','x-erpx-context-assignment':auth.assignmentId,'x-erpx-context-assignment-version':String(auth.assignmentVersion),'x-erpx-context-revision':String(auth.revision)};
function request(method,path,body,key) { return new Promise((resolve,reject)=>{
 const raw=Buffer.isBuffer(body)?body:body===undefined?undefined:Buffer.from(JSON.stringify(body));
 const req=http.request({host:'127.0.0.1',port:44817,path,method,headers:{...context,...(raw?{'content-type':Buffer.isBuffer(body)?'image/png':'application/json','content-length':raw.length}:{}),...(key?{'idempotency-key':key}:{})}},res=>{const chunks=[];res.on('data',c=>chunks.push(c));res.on('end',()=>resolve({status:res.statusCode,headers:res.headers,body:Buffer.concat(chunks)}));});req.on('error',reject);req.end(raw);
});}
async function json(method,path,body,key,status){const result=await request(method,path,body,key);assert.equal(result.status,status,`${method} ${path} status`);return JSON.parse(result.body).data;}
const base='/api/v1/holdings/acme/field-service/attachment-sets';
const set=await json('POST',base,{targetType:'field_service_visit',targetId:'issue-order',componentKind:'record',purposeKey:'site_photo',expectedTargetVersion:3},'container-create',201);
const reservation=await json('POST',`${base}/${set.id}/files`,{filename:'photo.png',mediaType:'image/png',bytes:original.length},'container-reserve',201);
const expiredSet=await json('POST',base,{targetType:'field_service_visit',targetId:'issue-order',componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:3},'container-expiry-create',201);
const expiredReservation=await json('POST',`${base}/${expiredSet.id}/files`,{filename:'expiry.png',mediaType:'image/png',bytes:original.length},'container-expiry-reserve',201);
await json('PUT',reservation.uploadUrl,original,undefined,202);
const statusPath=`/api/v1/holdings/acme/field-service/attachment-links/${reservation.attachmentLinkId}/status`;
let ready=false;
for(let i=0;i<120;i++){const state=await json('GET',statusPath,undefined,undefined,200);if(state.sourceState==='available'){ready=true;break;}await new Promise(resolve=>setTimeout(resolve,500));}
assert.ok(ready,'real scanner and preview worker did not make source available');
await json('POST',`/api/v1/holdings/acme/attachment-sets/${set.id}/bind`,{expectedTargetVersion:3},'container-bind',200);
const content=await request('GET',`/api/v1/holdings/acme/field-service/attachment-links/${reservation.attachmentLinkId}/content?variant=original`);
assert.equal(content.status,200);assert.deepEqual(content.body,original);assert.equal(content.headers['x-erpx-checksum-sha256'],createHash('sha256').update(original).digest('hex'));
fs.writeFileSync('/case/files-result.json',JSON.stringify({setId:set.id,linkId:reservation.attachmentLinkId,expirySet:expiredSet.id,expiryLink:expiredReservation.attachmentLinkId,expiryUpload:expiredReservation.uploadIntentId,bytes:original.length}),{mode:0o600,flag:'wx'});
console.log('native upload, real scan and preview, bind and original download passed');
