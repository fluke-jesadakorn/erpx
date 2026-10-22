import { relative, resolve } from "node:path";
import type { EngineeringObservation } from "@erpx/feature-runtime/lib/engineering-completion";

const record=(value:unknown):value is Record<string,unknown>=>value!==null&&typeof value==="object"&&!Array.isArray(value);
const digest=(value:unknown):value is string=>typeof value==="string"&&/^[a-f0-9]{64}$/.test(value);
/** Accept only the isolated synthetic-prior switch evidence; this says nothing about booting an older deployed release. */
export function syntheticRollbackEvidenceValid(value:unknown,sourceFingerprint:string,fileManifestSha256:string) {
  if(!record(value)||value.compiledCandidate!==true||value.sourceFingerprint!==sourceFingerprint||value.fileManifestSha256!==fileManifestSha256||
    typeof value.candidateReleaseId!=="string"||!record(value.rollback))return false;
  const rollback=value.rollback;
  return rollback.scope==="isolated-synthetic-prior-release"&&rollback.candidateReleaseId===value.candidateReleaseId&&
    typeof rollback.previousReleaseId==="string"&&rollback.previousReleaseId!==rollback.candidateReleaseId&&
    digest(rollback.candidateArtifactSha256)&&digest(rollback.previousArtifactSha256)&&rollback.candidateArtifactSha256!==rollback.previousArtifactSha256&&
    digest(rollback.candidateServerSha256)&&digest(rollback.previousServerSha256)&&rollback.candidateServerSha256!==rollback.previousServerSha256&&
    rollback.switchedToPrevious===true&&rollback.databaseAndFilesUnchanged===true&&
    digest(rollback.databaseStateBeforeSha256)&&rollback.databaseStateBeforeSha256===rollback.databaseStateAfterSha256&&
    digest(rollback.privateFilesStateBeforeSha256)&&rollback.privateFilesStateBeforeSha256===rollback.privateFilesStateAfterSha256;
}
type VitestReport = { success: boolean; testResults: Array<{ name: string; assertionResults: Array<{ fullName: string; status: string; duration?: number }> }> };
export function vitestObservations(root: string, report: VitestReport): EngineeringObservation[] {
  if (!Array.isArray(report.testResults)) throw new Error("VITEST_EXECUTION_RECEIPT_REQUIRED");
  return report.testResults.flatMap(file => file.assertionResults.map(test => ({ runner: "vitest" as const, file: relative(root, file.name), title: test.fullName,
    status: test.status === "passed" ? "passed" as const : test.status === "failed" ? "failed" as const : "skipped" as const, durationMs: test.duration })));
}
type BrowserSuite = { title: string; suites?: BrowserSuite[]; specs?: Array<{ title: string; file: string; tests: Array<{
  projectName: string; expectedStatus: string; status: string; results: Array<{ status: string; duration: number; attachments?: Array<{ path?: string }> }>
}> }> };
type PlaywrightReport = { config: { rootDir: string }; suites: BrowserSuite[] };
export function playwrightObservations(root: string, report: PlaywrightReport | {version:1;reports:PlaywrightReport[]}): EngineeringObservation[] {
  if("reports" in report) {
    if(report.version!==1||!Array.isArray(report.reports))throw new Error("PLAYWRIGHT_COHORT_RECEIPTS_REQUIRED");
    return report.reports.flatMap(item=>playwrightObservations(root,item));
  }
  if (!report.config?.rootDir || !Array.isArray(report.suites)) throw new Error("PLAYWRIGHT_EXECUTION_RECEIPT_REQUIRED");
  const reportRoot=report.config.rootDir;
  const observations: EngineeringObservation[] = [];
  function walk(suite: BrowserSuite, titles: string[]) {
    const path = suite.title && !/\.spec\.ts$/.test(suite.title) ? [...titles, suite.title] : titles;
    for (const spec of suite.specs ?? []) for (const test of spec.tests) {
      if (!["desktop", "mobile"].includes(test.projectName)) continue;
      const last = test.results.at(-1);
      const passed = test.expectedStatus === "passed" && test.status === "expected" && test.results.length > 0 && test.results.every(result => result.status === "passed");
      observations.push({ runner: "playwright", file: relative(root, resolve(reportRoot, spec.file)), title: [...path, spec.title].join(" "),
        project: test.projectName as "desktop" | "mobile", status: passed ? "passed" : !last || last.status === "skipped" ? "skipped" : "failed",
        durationMs: last?.duration, artifacts: test.results.flatMap(result => result.attachments?.flatMap(attachment => attachment.path ? [attachment.path] : []) ?? []) });
    }
    for (const nested of suite.suites ?? []) walk(nested, path);
  }
  for (const suite of report.suites) walk(suite, []);
  return observations;
}

/** Every collected required case must run exactly once and pass without a retry. */
export function requiredBrowserExecutionsValid(required:EngineeringObservation[],executed:EngineeringObservation[]){
 const key=(item:EngineeringObservation)=>JSON.stringify([item.file,item.title,item.project]);
 const wanted=new Set(required.map(key)),actual=new Set(executed.map(key));
 return required.length>0&&required.every(item=>item.runner==='playwright'&&['desktop','mobile'].includes(item.project??''))&&wanted.size===required.length
  &&executed.length===required.length&&actual.size===executed.length&&executed.every(item=>item.runner==='playwright'&&item.status==='passed'&&wanted.has(key(item)));
}
