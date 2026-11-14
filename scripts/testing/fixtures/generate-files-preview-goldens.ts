/** Development-only pinned Node/Sharp oracle; never a backend fallback. */
import sharp from 'sharp';
import {execFileSync} from 'node:child_process';
import { PDFDocument, rgb } from 'pdf-lib';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, join } from 'node:path';
import { sanitizedAttachmentPreview, detectedAttachmentMediaType } from '@erpx/server-files/public/attachments/preview';

if(process.version!=="v26.11.1")throw new Error("PINNED_FILES_PREVIEW_ORACLE_NODE_REQUIRED");
const sourceCommit="a5ed8737a2f05eff1ee9841fda4a094b3926d732";
const sourcePath="packages/server-files/src/adapters/attachments/preview.ts";
const currentSource=await readFile(sourcePath);
if(!currentSource.equals(execFileSync("git",["show",`${sourceCommit}:${sourcePath}`])))throw new Error("FILES_PREVIEW_B0_SOURCE_CHANGED");
const write=process.argv.slice(2).includes("--write");
if(process.argv.slice(2).some(arg=>arg!=="--write"&&arg!=="--check"))throw new Error("FILES_PREVIEW_ORACLE_ARGUMENT");
const directory=resolve(write?"fixtures/files/preview":".scratch/dev/files-preview-oracle");
await mkdir(directory,{recursive:true,mode:0o700});
const sha=(bytes:Uint8Array)=>createHash('sha256').update(bytes).digest('hex');
const cases:{id:string;mime:string;bytes:Uint8Array}[]=[];
const image=()=>sharp({create:{width:80,height:40,channels:3,background:{r:27,g:115,b:212}}});
cases.push({id:'png-exif',mime:'image/png',bytes:await image().withMetadata({exif:{IFD0:{ImageDescription:'Synthetic private preview metadata'}}}).png().toBuffer()});
cases.push({id:'jpeg-orientation-six',mime:'image/jpeg',bytes:await image().withMetadata({orientation:6,exif:{IFD0:{ImageDescription:'Synthetic private preview metadata'}}}).jpeg().toBuffer()});
cases.push({id:'webp-orientation-six',mime:'image/webp',bytes:await image().withMetadata({orientation:6}).webp().toBuffer()});
cases.push({id:'png-downsize',mime:'image/png',bytes:await sharp({create:{width:3200,height:40,channels:3,background:'red'}}).png().toBuffer()});
cases.push({id:'png-alpha',mime:'image/png',bytes:await sharp({create:{width:64,height:32,channels:4,background:{r:255,g:0,b:0,alpha:0.5}}}).png().toBuffer()});
cases.push({id:'jpeg-cmyk',mime:'image/jpeg',bytes:await image().toColourspace('cmyk').withMetadata().jpeg().toBuffer()});
cases.push({id:'heic-rotated',mime:'image/heic',bytes:await readFile(resolve('fixtures/files/attachment-formats/rotated-non-square.heic'))});
const pdf=await PDFDocument.create();pdf.setTitle('Synthetic private PDF preview metadata');
pdf.setCreationDate(new Date('2020-01-02T03:04:05Z'));pdf.setModificationDate(new Date('2020-01-02T03:04:05Z'));
pdf.addPage([80,40]).drawRectangle({x:0,y:0,width:80,height:40,color:rgb(1,0,0)});
pdf.addPage([80,40]).drawRectangle({x:0,y:0,width:80,height:40,color:rgb(0,0,1)});
cases.push({id:'pdf-first-page',mime:'application/pdf',bytes:await pdf.save({useObjectStreams:false})});
cases.push({id:'csv-no-preview',mime:'text/csv',bytes:Buffer.from('a,b\n1,2\n')});
cases.push({id:'invalid-png',mime:'image/png',bytes:cases[0].bytes.subarray(0,40)});
cases.push({id:'invalid-heic',mime:'image/heic',bytes:cases[6].bytes.subarray(0,32)});
cases.push({id:'invalid-pdf',mime:'application/pdf',bytes:Buffer.from('%PDF-1.7\ninvalid\n%%EOF\n')});
cases.push({id:'jpeg-appended-zip',mime:'image/jpeg',bytes:Buffer.concat([Buffer.from(cases[1].bytes),Buffer.from([0x50,0x4b,3,4,0,0,0,0])])});
cases.push({id:'png-over-40mp',mime:'image/png',bytes:await sharp({create:{width:8001,height:5000,channels:3,background:'red'}}).png().toBuffer()});
const pixels=Buffer.alloc(3201*101*3);for(let y=0;y<101;y++)for(let x=0;x<3201;x++){const i=(y*3201+x)*3;pixels[i]=(x*17+y*3)%256;pixels[i+1]=(Math.floor(x/7)*31+y*11)%256;pixels[i+2]=((x^y)*13)%256;}
cases.push({id:'png-pattern-downsize',mime:'image/png',bytes:await sharp(pixels,{raw:{width:3201,height:101,channels:3}}).png().toBuffer()});
cases.push({id:'jpeg-p3-orientation-eight',mime:'image/jpeg',bytes:await sharp(pixels,{raw:{width:3201,height:101,channels:3}}).withMetadata({orientation:8}).withIccProfile('p3').jpeg({quality:89}).toBuffer()});
cases.push({id:'jpeg-cmyk-no-icc',mime:'image/jpeg',bytes:await image().toColourspace('cmyk').jpeg().toBuffer()});
cases.push({id:'png-rgb16-p3',mime:'image/png',bytes:await sharp(pixels,{raw:{width:3201,height:101,channels:3}}).toColourspace('rgb16').withIccProfile('p3').png().toBuffer()});
const alphaPixels=Buffer.alloc(3201*101*4);for(let i=0,j=0;i<pixels.length;i+=3,j+=4){alphaPixels[j]=pixels[i];alphaPixels[j+1]=pixels[i+1];alphaPixels[j+2]=pixels[i+2];alphaPixels[j+3]=(j/4)%256;}
cases.push({id:'png-alpha-pattern-downsize',mime:'image/png',bytes:await sharp(alphaPixels,{raw:{width:3201,height:101,channels:4}}).png().toBuffer()});
const wide=Buffer.alloc(7201*31*3);for(let i=0;i<wide.length;i++)wide[i]=(i*31+Math.floor(i/7919))%256;
cases.push({id:'jpeg-shrink-pattern',mime:'image/jpeg',bytes:await sharp(wide,{raw:{width:7201,height:31,channels:3}}).withIccProfile('p3').jpeg({quality:89}).toBuffer()});
cases.push({id:'webp-shrink-pattern',mime:'image/webp',bytes:await sharp(pixels,{raw:{width:3201,height:101,channels:3}}).webp({quality:89}).toBuffer()});
const brokenIcc=Buffer.from(cases.find(c=>c.id==='jpeg-p3-orientation-eight')!.bytes);const profileHeader=brokenIcc.indexOf(Buffer.from('acsp'));if(profileHeader<0)throw new Error('ICC_FIXTURE_HEADER');brokenIcc.write('xxxx',profileHeader,'ascii');
cases.push({id:'jpeg-invalid-icc',mime:'image/jpeg',bytes:brokenIcc});
type PreviewOracle = {kind:"none"}|{kind:"failed";classification:string}|{kind:"rendered";file:string;bytes:number;sha256:string;mediaType:string;width:number|undefined;height:number|undefined;space:string;hasExif:boolean;hasIptc:boolean;hasXmp:boolean;hasIcc:boolean;hasAlpha:boolean;means:number[]};
type Outcome={id:string;mime:string;input:{file:string;bytes:number;sha256:string};detected:string|null;preview:PreviewOracle};
const outcomes:Outcome[]=[];
for(const input of cases){
  const source=input.id+'.input';await writeFile(join(directory,source),input.bytes,{mode:0o600});
  const row:Outcome={id:input.id,mime:input.mime,input:{file:source,bytes:input.bytes.length,sha256:sha(input.bytes)},detected:detectedAttachmentMediaType(input.bytes,input.mime),preview:{kind:"none"}};
  try{
    const result=await sanitizedAttachmentPreview(input.bytes,input.mime);
    if(!result)row.preview={kind:'none'};
    else{
      const file=input.id+(result.mediaType==='image/webp'?'.webp':'.jpg');await writeFile(join(directory,file),result.bytes,{mode:0o600});
      const metadata=await sharp(result.bytes).metadata();const stats=await sharp(result.bytes).stats();
      row.preview={kind:'rendered',file,bytes:result.bytes.length,sha256:sha(result.bytes),mediaType:result.mediaType,width:metadata.width,height:metadata.height,space:metadata.space,hasExif:!!metadata.exif,hasIptc:!!metadata.iptc,hasXmp:!!metadata.xmp,hasIcc:!!metadata.icc,hasAlpha:metadata.hasAlpha,means:stats.channels.map(c=>c.mean)};
    }
  }catch(error){const message=error instanceof Error?error.message:'';row.preview={kind:'failed',classification:message.includes('pixel limit')?'pixel-limit':message.includes('HEIF_PIXEL_LIMIT')?'pixel-limit':message.includes('timed out')?'timeout':'decoder-rejected'};}
  outcomes.push(row);
}
await writeFile(join(directory,'b0-results.json'),JSON.stringify({version:1,sourceCommit,source:{path:sourcePath,sha256:sha(currentSource)},scope:'Exact B0 provider on synthetic/owned inputs; direct renderer result and prior media admission are separate',node:process.version,sharp:sharp.versions,outcomes},null,2)+'\n',{mode:0o600});
if(!write){
 const expected=JSON.parse(await readFile(resolve("fixtures/files/preview/b0-results.json"),"utf8"));
 if(JSON.stringify(expected.outcomes)!==JSON.stringify(outcomes))throw new Error("FILES_PREVIEW_ORACLE_DIVERGENCE");
 for(const row of outcomes){for(const file of [row.input.file,...(row.preview.kind==="rendered"?[row.preview.file]:[])]){
  if(!(await readFile(join(directory,file))).equals(await readFile(resolve("fixtures/files/preview",file))))throw new Error("FILES_PREVIEW_ORACLE_ARTIFACT_DIVERGENCE");
 }}
}
console.log(`Files preview B0 oracle: ${outcomes.length} retained cases (${write?"written":"verified"}).`);
