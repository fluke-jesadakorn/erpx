import { createHash } from "node:crypto";
import { chmod, copyFile, mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import type { EngineeringGate, EngineeringGateId, EngineeringManifest, EngineeringObservation } from "../../../src/lib/runtime/engineering-completion";
import { evaluateEngineeringPerformance, type PerformanceSample } from "../../../src/lib/runtime/engineering-performance";
import { attachmentReconciliationEvidenceValid,businessWorkerEvidenceValid,financialContinuityEvidenceValid,stockContinuityEvidenceValid } from "../../../src/lib/runtime/engineering-gate-evidence";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../../local/source-fingerprint";
import { playwrightObservations,requiredBrowserExecutionsValid } from "./engineering-test-results";

const hash=(bytes:string|Buffer)=>createHash("sha256").update(bytes).digest("hex");
type IntegrationResult={gate:EngineeringGate;observations:EngineeringObservation[]};

/** Mutate aggregate evidence only inside a write that has finished its predecessor. */
export function serializeEngineeringEvidenceWrites() {
  let pending=Promise.resolve();
  return (write:()=>Promise<void>)=>{
    const next=pending.then(write);
    pending=next.catch(()=>{});
    return next;
  };
}

/** Drain both streams and await the real child exit even if either drain fails. */
export async function drainEngineeringProcess(child:{stdout:ReadableStream<Uint8Array>;stderr:ReadableStream<Uint8Array>;exited:Promise<number>},write:(bytes:Uint8Array)=>unknown) {
  const drain=async(stream:ReadableStream<Uint8Array>)=>{
    const reader=stream.getReader();
    try{while(true){const {done,value}=await reader.read();if(done)break;write(value);}}
    catch(error){await reader.cancel().catch(()=>{});throw error;}
    finally{reader.releaseLock();}
  };
  const settled=await Promise.allSettled([drain(child.stdout),drain(child.stderr),child.exited]);
  const childExitCode=settled[2].status==="fulfilled"?settled[2].value as number:undefined;
  return {exitCode:settled.every(result=>result.status==="fulfilled")?childExitCode??-1:-1,childExitCode};
}

/** The release branch may run early, but native-dependent gates await its durable receipt. */
export async function runConcurrentEngineeringVerification(tasks:{
  integration:()=>Promise<IntegrationResult>;
  recordIntegration:(result:IntegrationResult)=>Promise<void>;
  release:(waitForIntegration:()=>Promise<EngineeringObservation[]>)=>Promise<void>;
}) {
  const integration=Promise.resolve().then(tasks.integration).then(async result=>{
    await tasks.recordIntegration(result);
    return result;
  });
  const waitForIntegration=async()=>{
    const result=await integration;
    if(result.gate.id!=="integration"||result.gate.status!=="passed"||!result.observations.length
      ||result.observations.some(item=>item.runner!=="vitest"||!["passed","skipped"].includes(item.status)))
      throw new Error("NATIVE_INTEGRATION_REQUIRED");
    // Never expose run.observations: browser recording must not alias native evidence.
    return result.observations.map(item=>({...item,artifacts:item.artifacts?.slice()}));
  };
  const release=Promise.resolve().then(()=>tasks.release(waitForIntegration));
  // Even a release/build/pipe failure must wait for the native child and its evidence.
  return Promise.allSettled([integration,release]);
}

type Context={root:string;directory:string;sourceFingerprint:string;verificationFingerprint:string;manifest:EngineeringManifest;
  waitForIntegration:()=>Promise<EngineeringObservation[]>;add:(gate:EngineeringGate,observations?:EngineeringObservation[])=>Promise<void>};
async function privateJson(path:string,value:unknown) {await writeFile(path,JSON.stringify(value,null,2)+"\n",{mode:0o600});await chmod(path,0o600);}
export async function runReleaseVerification(context:Context) {
  const {root,directory,sourceFingerprint,verificationFingerprint}=context,node=join(root,".runtime/node/node-v26.11.1-darwin-arm64/bin/node");
  const stable=async()=>{const source=await runtimeSourceFingerprint(root);return source===sourceFingerprint&&await verificationSourceFingerprint(root,source)===verificationFingerprint;};
  async function command(label:string,args:string[],environment:Partial<NodeJS.ProcessEnv>={}) {
    console.log(`${label}: running; evidence ${relative(root,directory)}`);
    const path=join(directory,`${label}.log`),writer=Bun.file(path).writer(),startedAt=new Date().toISOString();
    let exitCode=-1,childExitCode:number|undefined;
    try {
      const child=Bun.spawn(args,{cwd:root,env:{...process.env,ERPX_NODE_BIN:node,...environment},stdout:"pipe",stderr:"pipe"});
      ({exitCode,childExitCode}=await drainEngineeringProcess(child,bytes=>writer.write(bytes)));
    }catch{/* A spawn failure still receives a failed, checksum-bound phase receipt. */}
    await writer.end();await chmod(path,0o600);
    const sourceStable=await stable();await privateJson(join(directory,`${label}.phase.json`),{startedAt,endedAt:new Date().toISOString(),exitCode,childExitCode,sourceStable,sourceFingerprint,verificationFingerprint,logSha256:hash(await readFile(path))});
    if(exitCode!==0||!sourceStable)throw new Error(`${label.toUpperCase().replaceAll(/[^A-Z0-9]/g,"_")}_${sourceStable?"FAILED":"SOURCE_CHANGED"}`);
    return path;
  }
  async function add(id:EngineeringGateId,artifact:string,passed:boolean,observations?:EngineeringObservation[]) {
    const bytes=await readFile(join(directory,artifact));
    await context.add({id,status:passed&&await stable()?"passed":"failed",sourceFingerprint,verificationFingerprint,artifact,sha256:hash(bytes)},observations);
  }
  const release=await realpath(join(root,".runtime/releases/latest")),build=JSON.parse(await readFile(join(release,"release.json"),"utf8"));
  if(!release.startsWith(join(root,".runtime/releases")+"/")||build.sourceFingerprint!==sourceFingerprint||build.node!=="26.11.1")throw new Error("COMPILED_CANDIDATE_REQUIRED");
  const buildFiles=await readFile(join(release,"release-files.json"));
  if(hash(buildFiles)!==build.fileManifestSha256)throw new Error("COMPILED_CANDIDATE_MANIFEST_MISMATCH");
  for(const [name,digest] of Object.entries(JSON.parse(buildFiles.toString()) as Record<string,string>)) {
    const path=resolve(release,name);if(!path.startsWith(release+"/")||hash(await readFile(path))!==digest)throw new Error("COMPILED_CANDIDATE_BYTES_MISMATCH");
  }
  await privateJson(join(directory,"compiled-candidate.json"),{releaseDirectory:relative(root,release),...build});
  await add("release-build","compiled-candidate.json",true);
  const recoveryEnvironment={ERPX_ENGINEERING_RELEASE_DIR:release,ERPX_ENGINEERING_SOURCE_FINGERPRINT:sourceFingerprint,ERPX_ENGINEERING_EVIDENCE_DIR:directory};
  await command("candidate-upgrade-recovery",[process.execPath,"scripts/testing/test.ts","tests/integration/shared.evidence/engineering-migration-upgrade.test.ts","tests/integration/shared.evidence/engineering-recovery.test.ts","tests/integration/finance/finance-reporting.test.ts","tests/integration/people_payroll/payroll-statutory.test.ts","--reporter=json",`--outputFile=${join(directory,"candidate-recovery-vitest.json")}`],recoveryEnvironment);
  const upgrade=JSON.parse(await readFile(join(directory,"migration-upgrade.json"),"utf8")),recovery=JSON.parse(await readFile(join(directory,"encrypted-recovery.json"),"utf8"));
  const candidateReportSha256=hash(await readFile(join(directory,"candidate-recovery-vitest.json")));
  for(const [name,receipt] of [["migration-upgrade.json",upgrade],["encrypted-recovery.json",recovery]] as const)
    await privateJson(join(directory,name),{...receipt,candidateReportSha256});
  await add("migration-upgrade","migration-upgrade.json",upgrade.compiledCandidate&&upgrade.sourceFingerprint===sourceFingerprint&&upgrade.fileManifestSha256===build.fileManifestSha256&&upgrade.tracks===8&&upgrade.beforeSha256===upgrade.afterSha256&&upgrade.priorReceiptsPreserved&&upgrade.backfillCount===1&&upgrade.replayBackfillCount===0&&upgrade.postedEvidenceImmutable&&upgrade.checksumsVerified&&upgrade.quotaDeltaBytes===0&&upgrade.pmsPriceBackfill?.count===1&&upgrade.pmsPriceBackfill?.replayCount===0&&["originalQuoteRetained","unquotedNotInvented","ownerHistoryRetained"].every(key=>upgrade.pmsPriceBackfill[key]===true));
  await add("recovery","encrypted-recovery.json",recovery.compiledCandidate&&recovery.sourceFingerprint===sourceFingerprint&&recovery.fileManifestSha256===build.fileManifestSha256&&recovery.databaseCount===8&&recovery.tableCount>=555&&recovery.restored&&recovery.compatibleRollback&&recovery.incompatibleRollbackRejected&&recovery.ciphertextTamperingRejected);

  const files=(await import("node:fs/promises")).readdir;
  const browserFiles=[...(await files(join(root,"tests/e2e"),{recursive:true})).filter(name=>/(?:^|\/)demo-.*\.spec\.ts$/.test(name)).map(name=>`tests/e2e/${name}`),"tests/uat/legal/legal-uat.spec.ts"];
  for(const capability of context.manifest.capabilities)for(const test of capability.cases)if(test.runner==="playwright"&&!browserFiles.includes(test.file))browserFiles.push(test.file);
  const reports:unknown[]=[],attachmentReports:string[]=[],cohorts:Array<{run:string;project:string;sourceFingerprint:string;fileManifestSha256:string}>=[];
  const collectionArtifact='browser-required-collection.json';
  await command('browser-required-collection',[node,'node_modules/@playwright/test/cli.js','test','--config','tests/config/playwright.config.ts',...browserFiles,'--project=desktop','--project=mobile','--list','--reporter=json'],{PLAYWRIGHT_JSON_OUTPUT_NAME:join(directory,collectionArtifact)});
  const collected=playwrightObservations(root,JSON.parse(await readFile(join(directory,collectionArtifact),'utf8')));
  if(!['desktop','mobile'].every(project=>collected.some(item=>item.project===project)))throw new Error('ALL_REQUIRED_BROWSER_PROJECTS_MUST_BE_COLLECTED');
  let performanceRun:string|undefined;
  const simulationEnvironment={ERPX_BROWSER_SIMULATION_PORT:"3104",ERPX_BROWSER_SIMULATION_ATTACHMENTS:"local",ERPX_BROWSER_SIMULATION_JOBS:"1",ERPX_BROWSER_SIMULATION_LEGAL:"1"};
  async function ready() {
    const deadline=Date.now()+60_000;
    while(Date.now()<deadline) {
      try{const health=await fetch("http://127.0.0.1:3104/api/health",{signal:AbortSignal.timeout(3_000)}),readiness=await fetch("http://127.0.0.1:3104/api/ready",{signal:AbortSignal.timeout(3_000)});
        if(health.status===200&&readiness.status===200)return;}catch{/* Startup evidence remains in the private process logs. */}
      await Bun.sleep(500);
    }
    throw new Error("ISOLATED_COMPILED_WEB_JOBS_NOT_READY");
  }
  for(const project of ["desktop","mobile"] as const) {
    const preparation=await command(`browser-${project}-prepare`,[process.execPath,"scripts/local/run-server-script.ts","scripts/local/browser-simulation.ts","prepare"],simulationEnvironment);
    const last=(await readFile(preparation,"utf8")).trim().split("\n").findLast(line=>line.startsWith('{"run":'));
    if(!last)throw new Error("EXACT_BROWSER_PREPARATION_RECEIPT_REQUIRED");const prepared=JSON.parse(last),run=String(prepared.run);
    if(!/^[a-f0-9]{12}$/.test(run)||prepared.origin!=="http://127.0.0.1:3104")throw new Error("BROWSER_PREPARATION_SCOPE_MISMATCH");
    await command(`browser-${project}-start`,[process.execPath,"scripts/local/run-server-script.ts","scripts/local/browser-simulation.ts","start",run],{...simulationEnvironment,ERPX_BROWSER_SIMULATION_RELEASE_DIR:release});
    await ready();
    await command(`legal-worker-${project}-pause`,[process.execPath,"scripts/local/run-server-script.ts","scripts/local/browser-simulation.ts","pause-jobs",run],simulationEnvironment);
    try {
      await command(`legal-worker-${project}`,[process.execPath,"scripts/local/run-server-script.ts","scripts/testing/fixtures/legal-production-worker-uat.ts",run,join(root,".runtime/browser",run,"release")],{...simulationEnvironment,ERPX_ENGINEERING_EVIDENCE_DIR:directory});
      await command(`business-worker-${project}`,[process.execPath,"scripts/local/run-server-script.ts","scripts/testing/fixtures/business-production-worker-uat.ts",run,join(root,".runtime/browser",run,"release")],{...simulationEnvironment,ERPX_ENGINEERING_EVIDENCE_DIR:directory});
    }finally{
      await command(`legal-worker-${project}-resume`,[process.execPath,"scripts/local/run-server-script.ts","scripts/local/browser-simulation.ts","resume-jobs",run],simulationEnvironment);
    }
    await ready();
    await command(`stock-${project}-baseline`,[process.execPath,"scripts/local/run-server-script.ts","scripts/testing/engineering/engineering-stock-continuity.ts",run,project,"baseline",directory]);
    await copyFile(join(root,".runtime/browser",run,"legal-production-worker-evidence.json"),join(directory,`legal-worker-${project}.json`));
    await copyFile(join(root,".runtime/browser",run,"legal-production-worker-control-fixture.json"),join(directory,`legal-worker-${project}-control.json`));
    await copyFile(join(root,".runtime/browser",run,"business-production-worker-evidence.json"),join(directory,`business-worker-${project}.json`));
    await copyFile(join(root,".runtime/browser",run,"business-production-worker-control-fixture.json"),join(directory,`business-worker-${project}-control.json`));
    cohorts.push({run,project,sourceFingerprint,fileManifestSha256:build.fileManifestSha256});
    await copyFile(join(root,".runtime/browser",run,"attachment-final.json"),join(directory,`attachments-${project}.json`));attachmentReports.push(`attachments-${project}.json`);
    const reportPath=join(directory,`playwright-${project}.json`),artifacts=join(directory,"browser-artifacts",project);await mkdir(artifacts,{recursive:true});
    await command(`browser-${project}`,[node,"node_modules/@playwright/test/cli.js","test","--config","tests/config/playwright.config.ts",...browserFiles,`--project=${project}`,"--reporter=line,json",`--output=${artifacts}`,"--workers=1","--retries=0"],{
      ERPX_DEMO_RUN:run,ERPX_E2E_BASE_URL:"http://127.0.0.1:3104",PLAYWRIGHT_JSON_OUTPUT_NAME:reportPath,ERPX_ENGINEERING_BROWSER_EVIDENCE:"1",ERPX_ENGINEERING_EVIDENCE_DIR:directory,
    });
    reports.push(JSON.parse(await readFile(reportPath,"utf8")));await chmod(reportPath,0o600);
    await command(`stock-${project}-final`,[process.execPath,"scripts/local/run-server-script.ts","scripts/testing/engineering/engineering-stock-continuity.ts",run,project,"final",directory]);
    if(project==="desktop") {
      await command("browser-desktop-stop",[process.execPath,"scripts/local/run-server-script.ts","scripts/local/browser-simulation.ts","destroy",run],simulationEnvironment);
      await Bun.sleep(1_000);
      await command("browser-desktop-destroy",[process.execPath,"scripts/local/run-server-script.ts","scripts/local/browser-simulation.ts","destroy",run],simulationEnvironment);
    }else performanceRun=run;
  }
  const businessWorkerReports=await Promise.all(["desktop","mobile"].map(async project=>({artifact:`business-worker-${project}.json`,sha256:hash(await readFile(join(directory,`business-worker-${project}.json`))),controlArtifact:`business-worker-${project}-control.json`,controlSha256:hash(await readFile(join(directory,`business-worker-${project}-control.json`)))})));
  const compiledFiles=JSON.parse(buildFiles.toString()) as Record<string,string>;
  const businessWorkersPassed=(await Promise.all(businessWorkerReports.map(async item=>businessWorkerEvidenceValid(JSON.parse(await readFile(join(directory,item.artifact),"utf8")),JSON.parse(await readFile(join(directory,item.controlArtifact),"utf8")),build,compiledFiles["tools/jobs.mjs"])))).every(Boolean);
  await privateJson(join(directory,"browser.json"),{version:1,reports,cohorts,businessWorkerReports,collectionArtifact,collectionSha256:hash(await readFile(join(directory,collectionArtifact)))});
  const observations=reports.flatMap(report=>playwrightObservations(root,report as Parameters<typeof playwrightObservations>[1]));
  await add("browser","browser.json",businessWorkersPassed&&requiredBrowserExecutionsValid(collected,observations)&&["desktop","mobile"].every(project=>observations.some(item=>item.project===project)),observations);
  const integration=await context.waitForIntegration();
  await privateJson(join(directory,"calendar-delivery.json"),{sourceFingerprint,verificationFingerprint,requiredFiles:["tests/integration/shared.calendar/calendar.test.ts","tests/unit/shared.calendar/calendar-recurrence.test.ts","tests/integration/shared.foundation/web-push.test.ts"],integrationReportSha256:hash(await readFile(join(directory,"vitest.json")))});
  const calendar=integration.filter(item=>["tests/integration/shared.calendar/calendar.test.ts","tests/unit/shared.calendar/calendar-recurrence.test.ts","tests/integration/shared.foundation/web-push.test.ts"].includes(item.file));
  await add("calendar-delivery","calendar-delivery.json",calendar.length>=30&&calendar.every(item=>item.status==="passed"));
  const legalPath=join(root,"tests/catalogue/legal-uat-engineering-cases.json");
  const legalCases=existsSync(legalPath)?JSON.parse(await readFile(legalPath,"utf8")):[];
  const workerReports=await Promise.all(["desktop","mobile"].map(async project=>({artifact:`legal-worker-${project}.json`,sha256:hash(await readFile(join(directory,`legal-worker-${project}.json`)))})));
  await privateJson(join(directory,"legal-uat.json"),{sourceFingerprint,verificationFingerprint,cases:legalCases,workerReports,browserReportSha256:hash(await readFile(join(directory,"browser.json"))),recoveryReportSha256:hash(await readFile(join(directory,"encrypted-recovery.json")))});
  const casePassed=(required:{file:string;title:string;runner?:string;project?:string})=>{
    const matches=[...integration,...observations].filter(item=>item.file===required.file&&item.title===required.title&&(!required.runner||item.runner===required.runner)&&item.project===required.project);
    return matches.length===1&&matches[0].status==="passed";
  };
  await add("legal-uat","legal-uat.json",Array.isArray(legalCases)&&legalCases.length===26&&new Set(legalCases.map(item=>item.id)).size===26&&Array.from({length:26},(_,index)=>`UAT-${String(index+1).padStart(2,"0")}`).every(id=>legalCases.some(item=>item.id===id&&item.review==="reviewed"&&item.proof?.length>=20&&["desktop","mobile"].every(project=>item.projects?.includes(project)&&casePassed({...item,project})))));
  const gateCasesPath=join(root,"tests/catalogue/engineering-gate-scenarios.json"),gateCases=existsSync(gateCasesPath)?JSON.parse(await readFile(gateCasesPath,"utf8")):{};
  const financial=gateCases["financial-continuity"]??[];
  const continuity=JSON.parse(await readFile(join(directory,"financial-continuity.json"),"utf8"));
  const stockReports=await Promise.all(["desktop","mobile"].map(async project=>({project,artifact:`stock-${project}.json`,sha256:hash(await readFile(join(directory,`stock-${project}.json`)))})));
  const stockPassed=(await Promise.all(stockReports.map(async item=>stockContinuityEvidenceValid(JSON.parse(await readFile(join(directory,item.artifact),"utf8")),build,item.project as "desktop"|"mobile")))).every(Boolean);
  await privateJson(join(directory,"financial-continuity-gate.json"),{sourceFingerprint,verificationFingerprint,required:financial,stockReports,continuityReportSha256:hash(await readFile(join(directory,"financial-continuity.json"))),candidateReportSha256:hash(await readFile(join(directory,"candidate-recovery-vitest.json"))),integrationReportSha256:hash(await readFile(join(directory,"vitest.json"))),browserReportSha256:hash(await readFile(join(directory,"browser.json")))});
  await add("financial-continuity","financial-continuity-gate.json",stockPassed&&financialContinuityEvidenceValid(continuity,build)&&financial.length>=3&&financial.every((item:{review:string;proof:string})=>item.review==="reviewed"&&item.proof.length>=20&&casePassed(item as unknown as Parameters<typeof casePassed>[0])));

  const developmentFinal=join(root,".scratch/dev/engineering-completion/development-attachments-final.json");
  if(existsSync(developmentFinal)){await copyFile(developmentFinal,join(directory,"attachments-development.json"));attachmentReports.push("attachments-development.json");}
  const ownerMatrixPath=join(root,"tests/catalogue/engineering-attachment-owner-cases.json"),ownerMatrix=existsSync(ownerMatrixPath)?JSON.parse(await readFile(ownerMatrixPath,"utf8")):[];
  await privateJson(join(directory,"attachments.json"),{sourceFingerprint,verificationFingerprint,reports:await Promise.all(attachmentReports.map(async artifact=>({artifact,sha256:hash(await readFile(join(directory,artifact)))}))),ownerMatrix,integrationReportSha256:hash(await readFile(join(directory,"vitest.json")))});
  const receipts=await Promise.all(attachmentReports.map(async path=>JSON.parse(await readFile(join(directory,path),"utf8"))));
  await command("attachment-owner-inventory",[process.execPath,"scripts/local/run-server-script.ts","scripts/testing/engineering/engineering-attachment-matrix.ts"]);
  await add("attachments","attachments.json",attachmentReports.includes("attachments-development.json")&&receipts.every(receipt=>attachmentReconciliationEvidenceValid(receipt,sourceFingerprint))&&ownerMatrix.length>=216&&new Set(ownerMatrix.map((item:{id:string})=>item.id)).size===ownerMatrix.length&&ownerMatrix.every((item:{review:string;proof:string;cases:Parameters<typeof casePassed>[0][]})=>item.review==="reviewed"&&item.proof?.length>=20&&item.cases.length&&item.cases.every(casePassed)));
  if(!performanceRun)throw new Error("NORMAL_LOGIN_PERFORMANCE_COHORT_REQUIRED");
  const performanceDirectory=join(directory,"performance-runtime");
  await mkdir(performanceDirectory,{recursive:true,mode:0o700});
  const performanceBuild=await Bun.build({entrypoints:[join(root,"scripts/testing/engineering/engineering-performance.ts")],outdir:performanceDirectory,target:"node",format:"esm",packages:"external",naming:"engineering-performance.mjs"});
  if(!performanceBuild.success)throw new Error("PERFORMANCE_NODE_COMPILATION_FAILED");
  const performanceEntry=join(performanceDirectory,"engineering-performance.mjs");
  await chmod(performanceEntry,0o600);
  if(!await stable())throw new Error("PERFORMANCE_NODE_COMPILATION_SOURCE_CHANGED");
  await privateJson(join(directory,"performance-runtime.json"),{sourceFingerprint,verificationFingerprint,node,entryArtifact:relative(directory,performanceEntry),entrySha256:hash(await readFile(performanceEntry))});
  await command("performance",[node,performanceEntry,performanceRun,directory],{ERPX_E2E_BASE_URL:"http://127.0.0.1:3104"});
  const performance=JSON.parse(await readFile(join(directory,"performance.json"),"utf8")),samplesBytes=await readFile(join(directory,performance.samplesArtifact));
  if(hash(samplesBytes)!==performance.samplesSha256)throw new Error("PERFORMANCE_RAW_SAMPLES_MISMATCH");
  const samples=samplesBytes.toString().trim().split("\n").map(line=>JSON.parse(line) as PerformanceSample);
  const evaluated=evaluateEngineeringPerformance({samples,durationMs:performance.durationMs,accountIds:performance.accountIds,sameProcesses:performance.processes.sameProcesses,runtimeErrors:performance.runtimeErrors});
  await add("performance","performance.json",evaluated.passed&&performance.sourceFingerprint===sourceFingerprint&&performance.verificationFingerprint===verificationFingerprint&&performance.release.fileManifestSha256===build.fileManifestSha256);
}
