import {createHash} from "node:crypto";
import {readFile,realpath} from "node:fs/promises";
import {join,resolve,sep} from "node:path";
import type {EngineeringGate,EngineeringObservation} from "../../../src/lib/runtime/engineering-completion";
import {attachmentReconciliationEvidenceValid,businessWorkerEvidenceValid,financialContinuityEvidenceValid,stockContinuityEvidenceValid} from "../../../src/lib/runtime/engineering-gate-evidence";
import {reconcileStockSnapshots,type StockSnapshot} from "../../../fixtures/inventory/builders/engineering-stock-continuity";
import {evaluateEngineeringPerformance,type PerformanceSample} from "../../../src/lib/runtime/engineering-performance";
import {playwrightObservations,vitestObservations,requiredBrowserExecutionsValid,syntheticRollbackEvidenceValid} from "./engineering-test-results";

const sha=(bytes:string|Buffer)=>createHash("sha256").update(bytes).digest("hex");
type Input={root:string;directory:string;sourceFingerprint:string;verificationFingerprint:string;gates:EngineeringGate[];observations:EngineeringObservation[]};
/** Re-derive every gate from raw, checksum-bound receipts instead of cached pass flags. */
export async function reassessEngineeringGates(input:Input):Promise<EngineeringGate[]> {
  const {root,directory,sourceFingerprint,verificationFingerprint}=input;
  const file=async(name:string)=>{const path=resolve(directory,name);if(!path.startsWith(directory+sep))throw new Error("EVIDENCE_CHILD_OUTSIDE_RUN");return readFile(path);};
  const json=async(name:string)=>JSON.parse((await file(name)).toString());
  const child=async(name:string,digest:unknown)=>typeof digest==="string"&&/^[a-f0-9]{64}$/.test(digest)&&sha(await file(name))===digest;
  const same=(item:{sourceFingerprint?:string;verificationFingerprint?:string})=>item.sourceFingerprint===sourceFingerprint&&item.verificationFingerprint===verificationFingerprint;
  const phase=async(name:string)=>{const receipt=await json(`${name}.phase.json`);return same(receipt)&&receipt.exitCode===0&&receipt.sourceStable===true&&await child(`${name}.log`,receipt.logSha256);};
  const candidateCase=async(file:string,title:string,gateReceipt:{candidateReportSha256?:unknown})=>{
    if(!await child("candidate-recovery-vitest.json",gateReceipt.candidateReportSha256))return false;
    const report=await json("candidate-recovery-vitest.json"),cases=vitestObservations(root,report),matching=cases.filter(item=>item.file===file&&item.title===title);
    return report.success===true&&!cases.some(item=>item.status==="failed")&&matching.length===1&&matching[0].status==="passed";
  };
  const casePassed=(required:{runner?:string;file:string;title:string;project?:string})=>{
    const matches=input.observations.filter(item=>item.runner===required.runner&&item.file===required.file&&item.title===required.title&&item.project===required.project);
    return matches.length===1&&matches[0].status==="passed";
  };
  let build:Awaited<ReturnType<typeof json>>|undefined;
  const compiled=async()=>{
    if(build)return build;
    const candidate=await json("compiled-candidate.json"),path=await realpath(resolve(root,candidate.releaseDirectory));
    if(!path.startsWith(join(root,".runtime/releases")+sep)||candidate.node!=="26.11.1"||candidate.sourceFingerprint!==sourceFingerprint||candidate.softwareVersion!==`release-${sourceFingerprint}`)throw new Error("EXACT_COMPILED_EVIDENCE_REQUIRED");
    const actual=JSON.parse(await readFile(join(path,"release.json"),"utf8")),manifest=await readFile(join(path,"release-files.json"));
    if(actual.sourceFingerprint!==candidate.sourceFingerprint||actual.softwareVersion!==candidate.softwareVersion||actual.fileManifestSha256!==candidate.fileManifestSha256||sha(manifest)!==candidate.fileManifestSha256)throw new Error("COMPILED_EVIDENCE_MANIFEST_CHANGED");
    for(const [name,digest] of Object.entries(JSON.parse(manifest.toString()))){const target=resolve(path,name);if(!target.startsWith(path+sep)||sha(await readFile(target))!==digest)throw new Error("COMPILED_EVIDENCE_BYTES_CHANGED");}
    return build=candidate;
  };
  const gates:EngineeringGate[]=[];
  for(const gate of input.gates) {
    let passed=false;
    try {
      if(!same(gate)||!await child(gate.artifact,gate.sha256))throw new Error("EVIDENCE_IDENTITY_MISMATCH");
      const value=await json(gate.artifact);
      switch(gate.id) {
        case "typecheck":case "lint":passed=await phase(gate.id);break;
        case "integration":{const observations=vitestObservations(root,value);passed=await phase("integration")&&value.success===true&&observations.length>0&&!observations.some(item=>item.status==="failed");break;}
        case "release-build":await compiled();passed=await phase("release-build");break;
        case "migration-upgrade":{const release=await compiled();passed=await candidateCase("tests/integration/shared.evidence/engineering-migration-upgrade.test.ts","upgrades populated pre-completion databases without changing posted money or files and replays canonical backfill once",value)&&await phase("candidate-upgrade-recovery")&&value.compiledCandidate===true&&value.sourceFingerprint===sourceFingerprint&&value.fileManifestSha256===release.fileManifestSha256&&value.tracks===8&&value.beforeSha256===value.afterSha256&&/^[a-f0-9]{64}$/.test(value.beforeSha256)&&value.priorReceiptsPreserved===true&&value.backfillCount===1&&value.replayBackfillCount===0&&value.postedEvidenceImmutable===true&&value.checksumsVerified===true&&value.quotaDeltaBytes===0&&value.pmsPriceBackfill?.count===1&&value.pmsPriceBackfill?.replayCount===0&&["originalQuoteRetained","unquotedNotInvented","ownerHistoryRetained"].every(key=>value.pmsPriceBackfill[key]===true);break;}
        case "recovery":{const release=await compiled();passed=await candidateCase("tests/integration/shared.evidence/engineering-recovery.test.ts","encrypts a complete isolated snapshot, restores immutable money and files, and rolls back only compatible releases",value)&&await phase("candidate-upgrade-recovery")&&value.compiledCandidate===true&&value.sourceFingerprint===sourceFingerprint&&value.fileManifestSha256===release.fileManifestSha256&&value.databaseCount===8&&value.tableCount>=555&&[value.tableDigestSha256,value.ciphertextSha256,value.sourceBytesSha256].every(item=>/^[a-f0-9]{64}$/.test(item))&&value.restored===true&&value.compatibleRollback===true&&value.incompatibleRollbackRejected===true&&syntheticRollbackEvidenceValid(value,sourceFingerprint,release.fileManifestSha256)&&value.ciphertextTamperingRejected===true&&["originalBytesRetained","historyRetained","aclRetained","unassignedReaderRefused","overlappingHoldsRetained","queuedJobRetained","commandReplayWithoutDuplicateEffects"].every(key=>value.legal?.[key]===true);break;}
        case "browser":{
          const release=await compiled(),observations=playwrightObservations(root,value),compiledFiles=JSON.parse(await readFile(join(resolve(root,release.releaseDirectory),"release-files.json"),"utf8"));
          passed=value.version===1&&value.reports.length===2&&value.cohorts.length===2&&new Set(value.cohorts.map((item:{run:string})=>item.run)).size===2&&value.cohorts.every((item:{sourceFingerprint:string;fileManifestSha256:string})=>item.sourceFingerprint===sourceFingerprint&&item.fileManifestSha256===release.fileManifestSha256)&&observations.length>=60&&["desktop","mobile"].every(project=>observations.some(item=>item.project===project))&&observations.every(item=>item.status==="passed")&&await phase("browser-desktop")&&await phase("browser-mobile")&&value.businessWorkerReports?.length===2;
          passed=passed&&value.collectionArtifact==='browser-required-collection.json'&&await child(value.collectionArtifact,value.collectionSha256)&&await phase('browser-required-collection')&&requiredBrowserExecutionsValid(playwrightObservations(root,await json(value.collectionArtifact)),observations);
          for(const project of ["desktop","mobile"]){const artifact=`business-worker-${project}.json`,controlArtifact=`business-worker-${project}-control.json`,receipt=value.businessWorkerReports?.find((item:{artifact:string})=>item.artifact===artifact);passed=passed&&Boolean(receipt)&&receipt.controlArtifact===controlArtifact&&await child(artifact,receipt.sha256)&&await child(controlArtifact,receipt.controlSha256)&&await phase(`business-worker-${project}`)&&(await json(artifact)).run===value.cohorts.find((item:{project:string})=>item.project===project)?.run&&businessWorkerEvidenceValid(await json(artifact),await json(controlArtifact),release,compiledFiles["tools/jobs.mjs"]);}
          break;
        }
        case "calendar-delivery":{const required=["tests/integration/shared.calendar/calendar.test.ts","tests/unit/shared.calendar/calendar-recurrence.test.ts","tests/integration/shared.foundation/web-push.test.ts"],cases=input.observations.filter(item=>item.runner==="vitest"&&required.includes(item.file));passed=same(value)&&JSON.stringify(value.requiredFiles)===JSON.stringify(required)&&await child("vitest.json",value.integrationReportSha256)&&cases.length>=30&&required.every(file=>cases.some(item=>item.file===file))&&cases.every(item=>item.status==="passed");break;}
        case "financial-continuity":{
          const release=await compiled(),register=JSON.parse(await readFile(join(root,"tests/catalogue/engineering-gate-scenarios.json"),"utf8")),required=register["financial-continuity"];
          const native=vitestObservations(root,await json("candidate-recovery-vitest.json")),receipt=await json("financial-continuity.json");
          passed=same(value)&&await child("vitest.json",value.integrationReportSha256)&&await child("browser.json",value.browserReportSha256)&&await child("financial-continuity.json",value.continuityReportSha256)&&await child("candidate-recovery-vitest.json",value.candidateReportSha256)&&await phase("candidate-upgrade-recovery")&&financialContinuityEvidenceValid(receipt,release)&&JSON.stringify(value.required)===JSON.stringify(required)&&Array.isArray(required)&&required.length>=3&&required.every(item=>item.review==="reviewed"&&item.proof?.length>=20&&casePassed(item))&&native.some(item=>item.file==="tests/integration/finance/finance-reporting.test.ts"&&item.title===receipt.case&&item.status==="passed")&&value.stockReports?.length===2;
          const browser=await json('browser.json');
          for(const project of ['desktop','mobile'] as const){
            const artifact=`stock-${project}.json`,reference=value.stockReports?.find((item:{project:string})=>item.project===project);
            if(!reference||reference.artifact!==artifact||!await child(artifact,reference.sha256)){passed=false;continue;}
            const stock=await json(artifact);passed=passed&&same(stock)&&stock.run===browser.cohorts.find((item:{project:string})=>item.project===project)?.run&&stockContinuityEvidenceValid(stock,release,project)&&await phase(`stock-${project}-baseline`)&&await phase(`stock-${project}-final`)&&await child(stock.baselineArtifact,stock.baselineArtifactSha256)&&await child(stock.finalSnapshotArtifact,stock.finalSnapshotArtifactSha256);
            const baseline=await json(stock.baselineArtifact),final=await json(stock.finalSnapshotArtifact);
            if(!same(baseline)||!same(final)||baseline.run!==stock.run||final.run!==stock.run||baseline.project!==project||final.project!==project||baseline.snapshots?.length!==3||final.snapshots?.length!==3){passed=false;continue;}
            const reconstructed=final.snapshots.map((item:{holdingId:string;snapshot:StockSnapshot})=>{const before=baseline.snapshots.find((row:{holdingId:string})=>row.holdingId===item.holdingId);if(!before)throw new Error('STOCK_BASELINE_HOLDING_MISSING');return {holdingId:item.holdingId,...reconcileStockSnapshots(before.snapshot,item.snapshot)};});
            passed=passed&&JSON.stringify(reconstructed)===JSON.stringify(stock.results)&&reconstructed.every((row:{passed:boolean})=>row.passed);
          }
          break;
        }
        case "legal-uat":{
          const release=await compiled(),register=JSON.parse(await readFile(join(root,"tests/catalogue/legal-uat-engineering-cases.json"),"utf8"));
          passed=same(value)&&await child("browser.json",value.browserReportSha256)&&await child("encrypted-recovery.json",value.recoveryReportSha256)&&JSON.stringify(value.cases)===JSON.stringify(register)&&Array.isArray(register)&&register.length===26&&new Set(register.map(item=>item.id)).size===26&&Array.from({length:26},(_,index)=>`UAT-${String(index+1).padStart(2,"0")}`).every(id=>register.some(item=>item.id===id&&item.review==="reviewed"&&item.proof?.length>=20&&["desktop","mobile"].every(project=>item.projects?.includes(project)&&casePassed({...item,runner:"playwright",project}))));
          for(const project of ["desktop","mobile"]) {
            const worker=await json(`legal-worker-${project}.json`),childReceipt=value.workerReports.find((item:{artifact:string})=>item.artifact===`legal-worker-${project}.json`);
            passed=passed&&Boolean(childReceipt)&&await child(`legal-worker-${project}.json`,childReceipt?.sha256)&&await child(`legal-worker-${project}-control.json`,worker.controlFixture?.sha256)&&await phase(`legal-worker-${project}`)&&await phase(`legal-worker-${project}-pause`)&&await phase(`legal-worker-${project}-resume`)&&worker.scope==="isolated-engineering"&&worker.sourceFingerprint===sourceFingerprint&&worker.softwareVersion===release.softwareVersion&&worker.releaseManifestSha256===release.fileManifestSha256&&["compiled_reminder","compiled_ocr_original_checksum","paused_company","restart_deduplication","holding_failure_heartbeat","certification_unchanged"].every(name=>worker.passed?.includes(name))&&Number.isInteger(worker.restart?.firstWorkerPid)&&Number.isInteger(worker.restart?.restartedWorkerPid)&&worker.restart?.firstWorkerPid!==worker.restart?.restartedWorkerPid&&new Set(worker.restart?.workerPids).size>=2&&worker.restart?.postRestartHeartbeat>worker.restart?.preRestartHeartbeat;
          }
          break;
        }
        case "attachments":{
          const matrix=JSON.parse(await readFile(join(root,"tests/catalogue/engineering-attachment-owner-cases.json"),"utf8"));
          passed=same(value)&&await child("vitest.json",value.integrationReportSha256)&&JSON.stringify(value.ownerMatrix)===JSON.stringify(matrix)&&await phase("attachment-owner-inventory")&&matrix.length>=216&&new Set(matrix.map((item:{id:string})=>item.id)).size===matrix.length&&matrix.every((item:{review:string;proof:string;cases:Parameters<typeof casePassed>[0][]})=>item.review==="reviewed"&&item.proof?.length>=20&&item.cases.length>0&&item.cases.every(casePassed))&&value.reports.length===3&&["attachments-development.json","attachments-desktop.json","attachments-mobile.json"].every(name=>value.reports.some((item:{artifact:string})=>item.artifact===name));
          for(const receipt of value.reports)passed=passed&&await child(receipt.artifact,receipt.sha256)&&attachmentReconciliationEvidenceValid(await json(receipt.artifact),sourceFingerprint);
          break;
        }
        case "performance":{
          const release=await compiled();if(!await child(value.samplesArtifact,value.samplesSha256))throw new Error("PERFORMANCE_SAMPLES_CHANGED");
          const samples=(await file(value.samplesArtifact)).toString().trim().split("\n").map(line=>JSON.parse(line) as PerformanceSample),assessment=evaluateEngineeringPerformance({samples,durationMs:value.durationMs,accountIds:value.accountIds,sameProcesses:value.processes.sameProcesses,runtimeErrors:value.runtimeErrors});
          passed=same(value)&&await phase("performance")&&assessment.passed&&value.release.fileManifestSha256===release.fileManifestSha256;break;
        }
      }
    }catch{/* Missing, altered or malformed underlying evidence is a failed gate. */}
    gates.push({...gate,status:passed?"passed":"failed"});
  }
  return gates;
}
