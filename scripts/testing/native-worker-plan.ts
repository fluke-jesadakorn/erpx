/** Reads the same exact candidate/quality command as affected verification.
 * This only plans prerequisites: it never installs tools or runs acceptance. */
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { loadCatalogue, type Suite } from './catalogue';
import { scopedArguments, scopedCargoQualityCommand, scopedPlan } from './scoped';

export function nativeWorkerPlanArguments(input:readonly string[]) {
  const args=input[0]==='--'?input.slice(1):[...input];let base:string|undefined,head:string|undefined;
  for(const arg of args){const match=/^--(base|head)=(.+)$/.exec(arg);if(!match)throw Error('NATIVE_WORKER_EXACT_CANDIDATE_ARGUMENTS_REQUIRED');
    if(match[1]==='base'){if(base)throw Error('NATIVE_WORKER_DUPLICATE_CANDIDATE_ARGUMENT');base=match[2];}
    else{if(head)throw Error('NATIVE_WORKER_DUPLICATE_CANDIDATE_ARGUMENT');head=match[2];}}
  if(!base||!head)throw Error('NATIVE_WORKER_EXACT_CANDIDATE_ARGUMENTS_REQUIRED');return {base,head};
}
export function nativePrerequisitePlan(native:readonly Suite[],changed:readonly string[],files:ReadonlyMap<string,string>,suites:readonly Suite[]) {
  const command=scopedCargoQualityCommand(native,changed,files,suites);
  const index=command?.indexOf('--features')??-1;
  const features=index<0?[]:command![index+1].split(',');
  const requiring=features.filter(feature=>feature==='erpx-server-files/preview-provider'||feature==='erpx-service/files-scan-worker');
  return {version:1,required:requiring.length>0,requiringFeatures:requiring,qualityCommand:command,acceptance:false};
}
export function planSelectedNativeWorker(input:readonly string[]) {
  const {base,head}=nativeWorkerPlanArguments(input);
  // Uses strict clean checked-out candidate validation, not a separate guessed diff.
  const {graph,plan}=scopedPlan(scopedArguments(['ci',`--base=${base}`,`--head=${head}`]));
  return {...nativePrerequisitePlan(plan.native,plan.changed,graph.files,loadCatalogue().suites),base:plan.base,candidate:plan.candidate,selectedSuites:plan.native.map(suite=>suite.id)};
}
if(import.meta.main){
  try{
    const plan=planSelectedNativeWorker(process.argv.slice(2));
    const directory=resolve('.scratch/dev/test-suites/native-worker');mkdirSync(directory,{recursive:true,mode:0o700});
    writeFileSync(resolve(directory,'affected-plan.json'),JSON.stringify(plan,null,2)+'\n',{mode:0o600});
    if(process.env.GITHUB_OUTPUT){if(process.env.CI!=='true'||process.env.GITHUB_ACTIONS!=='true')throw Error('NATIVE_WORKER_GITHUB_OUTPUT_CONTEXT_REQUIRED');appendFileSync(process.env.GITHUB_OUTPUT,`required=${String(plan.required)}\n`);}
    console.log(JSON.stringify({required:plan.required,requiringFeatures:plan.requiringFeatures,base:plan.base,candidate:plan.candidate,acceptance:false}));
  }catch(error){console.error(error instanceof Error?error.message:'NATIVE_WORKER_PLAN_FAILED');process.exitCode=1;}
}
