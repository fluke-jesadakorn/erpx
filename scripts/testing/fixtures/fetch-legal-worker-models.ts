/** Explicit developer fixture download. Ordinary tests/CI never fetch models. */
import {createHash}from'node:crypto';
import {chmodSync,existsSync,lstatSync,mkdirSync,readFileSync,writeFileSync,renameSync}from'node:fs';
import {resolve}from'node:path';
const root=resolve(import.meta.dirname,'../../..');
const receipt=JSON.parse(readFileSync(resolve(root,'fixtures/legal/documents/worker-bootstrap-provider.v1.json'),'utf8')) as {schema:number;models:{license:string;commit:string;resources:{file:string;source:string;bytes:number;sha256:string}[]}};
if(receipt.schema!==1||receipt.models.license!=='Apache-2.0'||!/^[a-f0-9]{40}$/.test(receipt.models.commit)||receipt.models.resources.length!==3||new Set(receipt.models.resources.map(resource=>resource.file)).size!==3)throw new Error('PINNED_OFFICIAL_MODEL_RECEIPT_REQUIRED');
let destination=root;
for(const component of ['.scratch','dev','legal-native-provider','tessdata']){
 destination=resolve(destination,component);
 if(existsSync(destination)){const entry=lstatSync(destination);if(entry.isSymbolicLink()||!entry.isDirectory())throw new Error('PRIVATE_MODEL_DIRECTORY_REQUIRED');}
 else mkdirSync(destination,{mode:0o700});
}
chmodSync(destination,0o700);
for(const resource of receipt.models.resources){
 if(!['eng.traineddata','tha.traineddata','LICENSE'].includes(resource.file)||resource.source!==`https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/${receipt.models.commit}/${resource.file}`||!Number.isSafeInteger(resource.bytes)||resource.bytes<1||resource.bytes>16*1024*1024||!/^[a-f0-9]{64}$/.test(resource.sha256))throw new Error('PINNED_OFFICIAL_MODEL_RECEIPT_REQUIRED');
 const path=resolve(destination,resource.file);
 if(existsSync(path)&&lstatSync(path).isSymbolicLink())throw new Error('PRIVATE_MODEL_FILE_REQUIRED');
 let bytes=existsSync(path)?readFileSync(path):undefined;
 if(!bytes||createHash('sha256').update(bytes).digest('hex')!==resource.sha256){
  const response=await fetch(resource.source,{redirect:'error',signal:AbortSignal.timeout(60_000)});
  if(!response.ok)throw new Error(`MODEL_FETCH_FAILED:${response.status}`);
  const reader=response.body?.getReader();if(!reader)throw new Error('MODEL_BODY_REQUIRED');const chunks:Uint8Array[]=[];let count=0;
  try{for(;;){const next=await reader.read();if(next.done)break;count+=next.value.byteLength;if(count>resource.bytes)throw new Error('MODEL_BYTES_EXCEED_PIN');chunks.push(next.value);}}finally{await reader.cancel();}
  bytes=Buffer.concat(chunks);
  if(bytes.length!==resource.bytes||createHash('sha256').update(bytes).digest('hex')!==resource.sha256)throw new Error('MODEL_CHECKSUM_MISMATCH');
  const temporary=path+'.download';if(existsSync(temporary))throw new Error('OWNED_MODEL_DOWNLOAD_ALREADY_EXISTS');writeFileSync(temporary,bytes,{mode:0o600,flag:'wx'});renameSync(temporary,path);
 }
 chmodSync(path,0o600);
 console.log(JSON.stringify({file:resource.file,bytes:bytes.length,sha256:resource.sha256}));
}
console.log('Pinned official eng+tha models are ready in the ignored private fixture directory.');
