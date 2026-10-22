import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { runtimeSourceFingerprint, verificationSourceFingerprint } from '../local/source-fingerprint';

const root=resolve(import.meta.dirname,'../..'),args=process.argv.slice(2),baseline=args.find(arg=>arg.startsWith('--baseline='))?.slice(11);
if(Bun.version!=='1.4.3'||!baseline||!/^[a-f0-9]{7,40}$/.test(baseline)||args.some(arg=>!arg.startsWith('--baseline=')))throw new Error('PINNED_GRAPH_BASELINE_REQUIRED');
const sha=execFileSync('git',['rev-parse','--verify',`${baseline}^{commit}`],{cwd:root,encoding:'utf8'}).trim();
const node=resolve(root,'.runtime/node/node-v26.11.1-darwin-arm64/bin/node');
if(execFileSync(node,['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw new Error('PINNED_NODE_REQUIRED');
const directory=resolve(root,'.scratch/dev/performance/workspace-graph',new Date().toISOString().replaceAll(':','-').replaceAll('.','-'));
await mkdir(directory,{recursive:true,mode:0o700});
const runtime=await runtimeSourceFingerprint(root),verification=await verificationSourceFingerprint(root,runtime);
const old=execFileSync('git',['show',`${sha}:scripts/testing/workspace-graph.ts`],{cwd:root,encoding:'utf8',maxBuffer:8*1024*1024});
const graphModule=resolve(root,'scripts/testing/workspace-graph.ts'),entry=resolve(directory,'entry.ts');
await writeFile(entry,`import {loadWorkspaceGraph,appRuntimeFingerprint} from ${JSON.stringify(graphModule)};
import {appIds} from '@erpx/contracts/apps';import{createHash}from'node:crypto';
const started=performance.now(),graph=loadWorkspaceGraph(process.cwd(),{cacheRoot:${JSON.stringify(resolve(directory,'cache'))}}),milliseconds=performance.now()-started,rssAfterLoadBytes=process.memoryUsage().rss;
const hash=createHash('sha256');for(const[file,source]of [...graph.files].sort(([a],[b])=>a.localeCompare(b)))hash.update(file).update('\\0').update(source).update('\\0');
console.log(JSON.stringify({milliseconds,rssAfterLoadBytes,files:graph.files.size,inputs:hash.digest('hex'),cache:graph.sourceCache??null,runtime:Object.fromEntries(appIds.map(id=>[id,appRuntimeFingerprint(graph,id)]))}));
`,{mode:0o600});
for(const mode of ['baseline','current']){
 const built=await Bun.build({entrypoints:[entry],outdir:resolve(directory,mode),target:'bun',format:'esm',naming:'graph.mjs',external:['typescript'],plugins:mode==='baseline'?[{name:'recorded-graph-implementation',setup(build){build.onLoad({filter:/workspace-graph\.ts$/},args=>args.path===graphModule?{contents:old,loader:'ts'}:undefined);}}]:[]});
 if(!built.success)throw new Error('GRAPH_BENCHMARK_BUNDLE_FAILED');
}
type Sample={mode:string;milliseconds:number;rssAfterLoadBytes:number;files:number;inputs:string;cache:{hits:number;misses:number;loaded:boolean;persisted:boolean}|null;runtime:Record<string,string>};
const samples:Sample[]=[];
for(const mode of ['baseline','baseline','baseline','current','current','current','current']){
 const child=Bun.spawn([process.execPath,'--env-file=/dev/null',resolve(directory,mode,'graph.mjs')],{cwd:root,stdout:'pipe',stderr:'pipe'});
 const [stdout,stderr,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
 await writeFile(resolve(directory,`sample-${samples.length+1}.stderr.log`),stderr,{mode:0o600});
 if(code!==0)throw new Error('GRAPH_BENCHMARK_CHILD_FAILED');
 const sample={mode,...JSON.parse(stdout)} as Sample;samples.push(sample);
 await writeFile(resolve(directory,'samples.json'),JSON.stringify(samples,null,2)+'\n',{mode:0o600});
 console.log(JSON.stringify({mode,sample:samples.length,milliseconds:sample.milliseconds,cache:sample.cache}));
}
const median=(values:number[])=>[...values].sort((a,b)=>a-b)[Math.floor(values.length/2)],before=median(samples.filter(s=>s.mode==='baseline').map(s=>s.milliseconds)),after=median(samples.slice(4).map(s=>s.milliseconds));
const sameInputs=samples.every(s=>s.inputs===samples[0].inputs),sameAppRuntimeInputs=samples.every(s=>JSON.stringify(s.runtime)===JSON.stringify(samples[0].runtime));
const sourceStable=await runtimeSourceFingerprint(root)===runtime&&await verificationSourceFingerprint(root,runtime)===verification;
const passed=sameInputs&&sameAppRuntimeInputs&&sourceStable&&samples.slice(4).every(s=>s.cache?.loaded&&s.cache.misses===0);
const receipt={version:1,acceptance:false,scope:'same-source graph loader comparison; not application HMR/build or production acceptance',baseline:sha,baselineImplementationSha256:createHash('sha256').update(old).digest('hex'),runtimeFingerprint:runtime,verificationFingerprint:verification,passed,sameInputs,sameAppRuntimeInputs,sourceStable,medianBaselineMilliseconds:before,medianWarmMilliseconds:after,reductionPercent:100*(1-after/before),samples};
await writeFile(resolve(directory,'receipt.json'),JSON.stringify(receipt,null,2)+'\n',{mode:0o600});
console.log(JSON.stringify({receipt:resolve(directory,'receipt.json'),passed,medianBaselineMilliseconds:before,medianWarmMilliseconds:after,reductionPercent:receipt.reductionPercent}));
if(!passed)throw new Error('GRAPH_BENCHMARK_IDENTITY_OR_CACHE_MISMATCH');
