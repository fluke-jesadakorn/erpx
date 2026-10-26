import { existsSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
const root=resolve(import.meta.dir,'../..'),args=process.argv.slice(2);
if(args.length>1||args.some(arg=>arg!=='--check'))throw new Error('WORK_MODULE_CONTRACT_ARGUMENTS_INVALID');
const result=Bun.spawnSync([process.env.ERPX_CARGO_BIN??'cargo','run','--quiet','--locked','--package','erpx-server-identity-access','--example','export-work-module-contracts'],{cwd:root,stdout:'pipe',stderr:'pipe'});
if(result.exitCode!==0){process.stderr.write(result.stderr);throw new Error('WORK_MODULE_CONTRACT_GENERATION_FAILED');}
const definitions:unknown=JSON.parse(result.stdout.toString());
if(!Array.isArray(definitions)||!definitions.length||definitions.some(d=>!d||typeof d.id!=='string'||!Array.isArray(d.modules)||d.modules.some((m:unknown)=>typeof m!=='string')))throw new Error('WORK_MODULE_CONTRACT_OUTPUT_INVALID');
const roles=Object.fromEntries(definitions.map(d=>[d.id,d.modules]));
const source='// Generated from the canonical Rust Work module catalog. Do not edit.\nexport const businessRoles = '+JSON.stringify(roles,null,2)+' as const;\n';
const destination=resolve(root,'packages/contracts/src/business-role-modules.generated.ts');
if(args.includes('--check')){if(!existsSync(destination)||readFileSync(destination,'utf8')!==source)throw new Error('WORK_MODULE_CONTRACTS_STALE');console.log('Frontend module metadata matches the canonical Rust catalog.');}
else{const temporary=`${destination}.${process.pid}.tmp`;writeFileSync(temporary,source);renameSync(temporary,destination);console.log('Generated frontend module metadata from Rust.');}
