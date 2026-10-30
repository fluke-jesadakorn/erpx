// Retained B0 positive inputs through real private HTTP and the Compose worker.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import {createHash} from 'node:crypto';
const auth=JSON.parse(fs.readFileSync('/case/account-proof-maker.json','utf8'));
const context={'host':'acme.jesadakorn.com','origin':'https://acme.jesadakorn.com','cookie':auth.cookie,'x-erpx-context-company':'company','x-erpx-context-assignment':auth.assignmentId,'x-erpx-context-assignment-version':String(auth.assignmentVersion),'x-erpx-context-revision':String(auth.revision)};
function request(method,path,body,key,mime) {return new Promise((resolve,reject)=>{
 const raw=Buffer.isBuffer(body)?body:body===undefined?undefined:Buffer.from(JSON.stringify(body));
 const req=http.request({host:'127.0.0.1',port:44817,path,method,headers:{...context,...(raw?{'content-type':mime??'application/json','content-length':raw.length}:{}),...(key?{'idempotency-key':key}:{})}},res=>{const chunks=[];res.on('data',c=>chunks.push(c));res.on('end',()=>resolve({status:res.statusCode,headers:res.headers,body:Buffer.concat(chunks)}));});req.on('error',reject);req.end(raw);
});}
function dimensions(b){
 if(b.toString('ascii',0,4)==='RIFF'&&b.toString('ascii',8,12)==='WEBP'){
  const kind=b.toString('ascii',12,16);
  if(kind==='VP8X')return [1+b.readUIntLE(24,3),1+b.readUIntLE(27,3)];
  if(kind==='VP8L'){const bits=b.readUInt32LE(21);return [(bits&16383)+1,((bits>>>14)&16383)+1];}
  if(kind==='VP8 ')return [b.readUInt16LE(26)&16383,b.readUInt16LE(28)&16383];
 }
 if(b[0]===255&&b[1]===216){let p=2;while(p+8<b.length){if(b[p++]!==255)break;let marker=b[p++];while(marker===255)marker=b[p++];if([0xc0,0xc1,0xc2,0xc3].includes(marker))return [b.readUInt16BE(p+5),b.readUInt16BE(p+3)];const size=b.readUInt16BE(p);if(size<2)break;p+=size;}}
 throw Error('PREVIEW_DIMENSIONS_UNREADABLE');
}
async function json(method,path,body,key,status,mime){const r=await request(method,path,body,key,mime);assert.equal(r.status,status,`${method} ${path} status`);return JSON.parse(r.body).data;}
// Exercise every supported media type plus the separate ingress polyglot rule.
// The unchanged provider test covers the complete 22-case retained corpus.
const rows=JSON.parse(fs.readFileSync('/case/preview/b0-results.json','utf8')).outcomes.filter(r=>r.preview.kind==='rendered').slice(0,9);assert.equal(rows.length,9);
const base='/api/v1/holdings/acme/field-service/attachment-sets';let set;const results=[];
function record(value){results.push(value);fs.writeFileSync('/case/compose-corpus-result.json',JSON.stringify(results),{mode:0o600});}
for(const [index,row] of rows.entries()){
 if(index%8===0)set=await json('POST',base,{targetType:'field_service_visit',targetId:'issue-order',componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:4},`corpus-set-${index}`,201);
 const input=fs.readFileSync('/case/preview/'+row.input.file);
 const reserved=await request('POST',`${base}/${set.id}/files`,{filename:row.input.file,mediaType:row.mime,bytes:input.length},`corpus-reserve-${index}`);
 if(reserved.status!==201){let code;try{code=JSON.parse(reserved.body).error?.code;}catch{}record({id:row.id,result:`reserve-${reserved.status}`,code});continue;}
 const reserve=JSON.parse(reserved.body).data;
 const upload=await request('PUT',reserve.uploadUrl,input,undefined,row.mime);
 if(upload.status!==202){let code;try{code=JSON.parse(upload.body).error?.code;}catch{}record({id:row.id,result:row.id==='jpeg-appended-zip'&&upload.status===422&&code==='UPLOAD_TYPE'?'admission-rejected':`upload-${upload.status}`,code});continue;}
 const path=`/api/v1/holdings/acme/field-service/attachment-links/${reserve.attachmentLinkId}`;let ready=false;
 for(let i=0;i<120;i++){const state=await json('GET',`${path}/status`,undefined,undefined,200);if(state.sourceState==='available'){ready=true;break;}await new Promise(resolve=>setTimeout(resolve,500));}
 if(!ready){record({id:row.id,result:'not-available'});continue;}
 const preview=await request('GET',`${path}/content?variant=preview`);const expected=fs.readFileSync('/case/preview/'+row.preview.file);
 const size=preview.status===200?dimensions(preview.body):[];const checksum=createHash('sha256').update(preview.body).digest('hex');
 const exact=preview.body.equals(expected)&&preview.headers['content-type']===row.preview.mediaType&&size[0]===row.preview.width&&size[1]===row.preview.height&&checksum===preview.headers['x-erpx-checksum-sha256'];
 record({id:row.id,result:preview.status===200?(exact?'exact':'different-preview'):`http-${preview.status}`,mediaType:preview.headers['content-type'],dimensions:size,expectedDimensions:[row.preview.width,row.preview.height],checksum,checksumHeader:preview.headers['x-erpx-checksum-sha256'],bytes:preview.body.length,expectedBytes:expected.length});
}
assert.ok(results.length===9&&results.every(r=>r.result==='exact'||r.result==='admission-rejected'),'preview or ingress behavior differed; see private result');
console.log('eight preview inputs matched B0 bytes and the polyglot was rejected');
