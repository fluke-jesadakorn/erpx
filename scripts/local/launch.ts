import { spawn } from 'node:child_process';
import { appendFile, stat, rename, unlink } from 'node:fs/promises';
import { join } from 'node:path';
import { getRuntime,closeRuntime } from '../../src/server/runtime/env';

const role=process.argv[2];
if(role!=='web'&&role!=='jobs'&&role!=='tunnel')throw new Error('INVALID_SERVICE_ROLE');
const base=process.env.ERPX_HOME??'/Library/Application Support/ERPX';
const envFile=process.env.ERPX_ENV_FILE??'production.env';
if(!/^[A-Za-z0-9][A-Za-z0-9._-]*\.env$/.test(envFile))throw new Error('INVALID_ENV_FILE');
process.loadEnvFile(join(base,'config',envFile));
const port=process.env.ERPX_PORT??'3101';
if(!/^\d{1,5}$/.test(port)||Number(port)<1024||Number(port)>65535)throw new Error('INVALID_ERP_PORT');
const environment={NODE_ENV:'production',ERPX_DATA_DIR:join(base,'data'),ERPX_PROCESS_ROLE:role==='jobs'?'jobs':'app',
  ERPX_DATABASE_MANIFEST:join(base,'data/config',role==='jobs'?'databases-jobs.json':'databases.json'),HOSTNAME:'127.0.0.1',PORT:port} as Record<string,string>;
if(role==='web') environment.ERPX_MIGRATION_DATABASE_MANIFEST=join(base,'data/config','databases-migrations.json');
Object.assign(process.env,environment);
const log=join(base,'logs',`${role}.log`);
let logs=Promise.resolve();
function output(chunk:Buffer){logs=logs.then(async()=>{
  if((await stat(log).catch(()=>({size:0}))).size>5*1024*1024){
    await unlink(`${log}.7`).catch(()=>{});
    for(let i=6;i>=1;i--)await rename(`${log}.${i}`,`${log}.${i+1}`).catch(()=>{});
    await rename(log,`${log}.1`).catch(()=>{});
  }
  await appendFile(log,chunk,{mode:0o600});
}).catch(()=>{});}
let ready=false;
for(let i=0;i<60;i++){
  try{await getRuntime().CONTROL_DB.prepare('SELECT 1').first();ready=true;break;}catch{await new Promise(resolve=>setTimeout(resolve,1000));}
}
await closeRuntime();
if(!ready)throw new Error('DATABASE_NOT_READY');
const current=join(base,'current');
const executable=role==='tunnel'?join(base,'runtime/cloudflared/cloudflared'):process.execPath;
const args=role==='tunnel'?['--no-autoupdate','tunnel','--loglevel','warn','--metrics','127.0.0.1:0','run','--token-file',join(base,'config/tunnel.token')]:[role==='web'?'server.js':'tools/jobs.mjs'];
const child=spawn(executable,args,{cwd:current,env:process.env,stdio:['ignore','pipe','pipe']});
child.stdout.on('data',output);child.stderr.on('data',output);
for(const signal of ['SIGINT','SIGTERM'] as const)process.on(signal,()=>child.kill(signal));
child.on('error',()=>process.exit(1));
child.on('exit',async code=>{await logs;process.exit(code??1);});
