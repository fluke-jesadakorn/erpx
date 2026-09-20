import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {createHash} from 'node:crypto';
const root=process.cwd(),python=resolve('.runtime/legal-python/bin/python');
async function run(args:string[]){const p=Bun.spawn(args,{cwd:root,stdout:'inherit',stderr:'inherit'});if(await p.exited!==0)throw new Error('LEGAL_DEPENDENCY_SETUP_FAILED')}
await run(['python3','-m','venv',resolve('.runtime/legal-python')]);
await run([python,'-m','pip','install','-r','scripts/legal/requirements.lock']);
const directory=resolve('.runtime/legal-ocr/tessdata');await mkdir(directory,{recursive:true});
const expected={eng:'7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2',tha:'294227cc2d1292b0acb28d61d4115c88252b96d466ca90b417cf4cf0c67bf07c'};
for(const [language,checksum] of Object.entries(expected)){
 const path=join(directory,`${language}.traineddata`);let bytes:Uint8Array;
 try{bytes=await readFile(path)}catch{const response=await fetch(`https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/${language}.traineddata`,{signal:AbortSignal.timeout(60000)});if(!response.ok)throw new Error('OCR_LANGUAGE_DOWNLOAD_FAILED');bytes=new Uint8Array(await response.arrayBuffer())}
 if(createHash('sha256').update(bytes).digest('hex')!==checksum)throw new Error('OCR_LANGUAGE_CHECKSUM_MISMATCH');await writeFile(path,bytes,{mode:0o600});
}
await run([process.env.ERPX_TESSERACT_BIN??'/opt/homebrew/bin/tesseract','--tessdata-dir',directory,'--list-langs']);
await run([python,'scripts/legal/test_validation.py']);
console.log('Legal Python validator and verified English/Thai OCR data are ready.');
