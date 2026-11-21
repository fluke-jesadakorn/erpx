// Independent development-only B0 provider receipt using the existing pinned
// native tools. No database, network, service or production directory is used.
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import crypto from 'node:crypto';
import pdfLib from 'pdf-lib';
import sharp from 'sharp';
const {PDFDocument,StandardFonts}=pdfLib;
const exec=promisify(execFile);
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
const tools={pdftotext:'/opt/homebrew/bin/pdftotext',pdfinfo:'/opt/homebrew/bin/pdfinfo',pdftoppm:'/opt/homebrew/bin/pdftoppm',tesseract:'/opt/homebrew/bin/tesseract',tessdata:'/opt/homebrew/share/tessdata',languages:'eng'};
const options={env:{LANG:'C',TZ:'UTC',OMP_THREAD_LIMIT:'1',OMP_NUM_THREADS:'1'},timeout:30000,maxBuffer:2*1024*1024};
(async()=>{
  const png=await sharp(Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="300"><rect width="1200" height="300" fill="white"/><text x="40" y="130" font-family="Arial" font-size="64" fill="black">ERPX LEGAL SAMPLE 2026</text></svg>')).png().toBuffer();
  const jpeg=await sharp(png).jpeg({quality:95}).toBuffer();
  async function pdf(scanned,count){const pdf=await PDFDocument.create();const font=await pdf.embedFont(StandardFonts.Helvetica);const image=scanned?await pdf.embedPng(png):null;for(let i=0;i<count;i++){const page=pdf.addPage([595,842]);if(image)page.drawImage(image,{x:40,y:600,width:515,height:128.75});else page.drawText(`ERPX legal page ${i+1}`,{font,x:52,y:790,size:18});}pdf.setCreationDate(new Date('2026-10-27T00:00:00Z'));pdf.setModificationDate(new Date('2026-10-27T00:00:00Z'));return Buffer.from(await pdf.save());}
  const inputs=[['text-pdf','application/pdf',await pdf(false,2)],['png','image/png',png],['jpeg','image/jpeg',jpeg],['scanned-pdf','application/pdf',await pdf(true,2)],['scanned-page-limit','application/pdf',await pdf(true,26)]];
  const cases=[];
  for(const [id,mediaType,bytes] of inputs){const root=await fs.mkdtemp(path.join(os.tmpdir(),'erpx-legal-oracle-'));await fs.chmod(root,0o700);try{
    const input=path.join(root,'input');await fs.writeFile(input,bytes,{mode:0o600});let text='',pages=[],method;
    if(mediaType==='application/pdf'){
      text=(await exec(tools.pdftotext,['-layout',input,'-'],options)).stdout;method='pdf-text';const segments=text.split('\f');if(!segments.at(-1)?.trim())segments.pop();pages=segments.map((text,index)=>({page:index+1,text:text.slice(0,200000)}));
      if(!text.trim()){
        const info=(await exec(tools.pdfinfo,[input],{...options,timeout:15000,maxBuffer:100000})).stdout;const count=Number(info.match(/^Pages:\s+(\d+)/m)?.[1]);if(!count||count>25){cases.push({id,mediaType,hex:bytes.toString('hex'),sha256:sha(bytes),expected:{error:'SCANNED_PDF_PAGE_LIMIT'}});continue;}
        await exec(tools.pdftoppm,['-png','-scale-to','1600','-f','1','-l',String(count),input,path.join(root,'page')],{...options,timeout:90000,maxBuffer:100000});
        const files=(await fs.readdir(root)).filter(name=>/^page-.*\.png$/.test(name)).sort();const texts=[];for(const name of files)texts.push((await exec(tools.tesseract,[path.join(root,name),'stdout','-l',tools.languages,'--tessdata-dir',tools.tessdata],{...options,timeout:20000,maxBuffer:1000000})).stdout);text=texts.join('\n\n');pages=texts.map((text,index)=>({page:index+1,text:text.slice(0,200000)}));method='pdf-ocr';
      }
    }else{text=(await exec(tools.tesseract,[input,'stdout','-l',tools.languages,'--tessdata-dir',tools.tessdata],{...options,timeout:45000})).stdout;method='image-ocr';}
    if(!pages.length)pages=[{page:1,text}];cases.push({id,mediaType,hex:bytes.toString('hex'),sha256:sha(bytes),expected:{text,method,pages}});
  }finally{await fs.rm(root,{recursive:true,force:true});}}
  const toolReceipts=[];for(const name of ['pdftotext','pdfinfo','pdftoppm','tesseract'])toolReceipts.push({name,path:tools[name],sha256:sha(await fs.readFile(tools[name]))});
  const receipt={schema:1,oracle:{node:process.version,tools,toolReceipts,language:{file:'eng.traineddata',sha256:sha(await fs.readFile(path.join(tools.tessdata,'eng.traineddata')))},providerSource:{path:'packages/server-legal/src/adapters/legal/document-provider.ts',sha256:sha(await fs.readFile('packages/server-legal/src/adapters/legal/document-provider.ts'))}},cases};
  await fs.writeFile('fixtures/legal/documents/native-conformance.v1.json',JSON.stringify(receipt,null,2)+'\n');console.log(`Captured ${cases.length} independent native provider cases (explicit English fixture profile)`);
})().catch(error=>{console.error(error.message);process.exitCode=1;});
