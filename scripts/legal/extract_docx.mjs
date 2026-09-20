// Isolated, bounded process. Raw text only; no HTML or external resources.
import fs from 'node:fs/promises';
import mammoth from 'mammoth';
const bytes=await fs.readFile(process.argv[2]);
if(bytes.length>10*1024*1024)throw new Error('DOCX_TOO_LARGE');
const result=await mammoth.extractRawText({buffer:bytes});
if(result.value.length>200000)throw new Error('DOCX_TEXT_TOO_LARGE');
process.stdout.write(result.value);
