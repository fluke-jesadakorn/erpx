import {existsSync,readFileSync,renameSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
const root=resolve(import.meta.dir,'../..');
const args=process.argv.slice(2);if(args.some(a=>a!=='--check')||args.length>1)throw Error('DIRECTORY_SOURCE_CONTRACT_ARGUMENTS_INVALID');
const command=[process.env.ERPX_CARGO_BIN??'cargo','run','--quiet','--locked','--package','erpx-service','--example','export-directory-source-contracts','--features','contracts,directory-ldaps'];
const typed=Bun.spawnSync(command,{cwd:root,stdout:'pipe',stderr:'pipe'});if(typed.exitCode!==0){process.stderr.write(typed.stderr);throw Error('DIRECTORY_SOURCE_CONTRACT_GENERATION_FAILED');}
const source=typed.stdout.toString().replace(/[ \t]+$/gm,'');if(!source.startsWith('// Generated from native Rust Directory source-admin wire types.')||!source.includes('export type DirectorySourceInteger = number')||!source.includes('export type DirectorySourceResponse'))throw Error('DIRECTORY_SOURCE_CONTRACT_OUTPUT_INVALID');
const api=Bun.spawnSync([...command,'--','--openapi'],{cwd:root,stdout:'pipe',stderr:'pipe'});if(api.exitCode!==0){process.stderr.write(api.stderr);throw Error('DIRECTORY_SOURCE_OPENAPI_GENERATION_FAILED');}
const specification=api.stdout.toString();const document:unknown=JSON.parse(specification);if(!document||typeof document!=='object'||!('openapi'in document)||!('paths'in document))throw Error('DIRECTORY_SOURCE_OPENAPI_OUTPUT_INVALID');
for(const [file,content]of [['packages/contracts/src/directory-source-admin.generated.ts',source],['docs/contracts/next/directory-source-admin.openapi.generated.json',specification]]){
 const path=resolve(root,file);if(args.includes('--check')){if(!existsSync(path)||readFileSync(path,'utf8')!==content)throw Error(`DIRECTORY_SOURCE_CONTRACT_STALE:${file}`);}else{const temporary=`${path}.${process.pid}.tmp`;writeFileSync(temporary,content);renameSync(temporary,path);}
}
console.log(args.includes('--check')?'Native Directory source-admin contracts match Rust.':'Generated native Directory source-admin TypeScript and OpenAPI contracts.');
