import { expect, it } from "vitest";
import { evaluateEngineeringPerformance, engineeringPerformanceWorkspacePath, type PerformanceSample } from "../../../src/lib/runtime/engineering-performance";
function input() {
  const accountIds=Array.from({length:10},(_,i)=>`account-${i}`),samples:PerformanceSample[]=[];
  for (const actor of accountIds) for(let tick=0;tick<=180;tick++) {
    samples.push({actor,kind:"work",elapsedMs:100,atMs:tick*10_000,status:200,validated:true});
    if(actor==="account-0")samples.push({actor,kind:"people",elapsedMs:200,atMs:tick*10_000,status:200,validated:true});
    if(actor==="account-1")samples.push({actor,kind:"pms",elapsedMs:300,atMs:tick*10_000,status:200,validated:true});
    if(tick%3===0)samples.push({actor,kind:"page",pageKind:actor==="account-0"?"people":actor==="account-1"?"pms":"work",elapsedMs:700,atMs:tick*10_000,status:200,validated:true});
    if(actor==="account-0"&&tick%3===0)samples.push({actor:"public",kind:"public",elapsedMs:900,atMs:tick*10_000,status:200,validated:true});
  }
  return {accountIds,samples,durationMs:30*60_000,sameProcesses:true,runtimeErrors:[]};
}
it("uses current normal workspace routes for measured warm pages",()=>{
  expect(engineeringPerformanceWorkspacePath("simulation","work")).toBe("/holdings/simulation/work");
  expect(engineeringPerformanceWorkspacePath("simulation","people")).toBe("/holdings/simulation/people-organization/people");
  expect(engineeringPerformanceWorkspacePath("riverside-lantern","pms")).toBe("/holdings/riverside-lantern/pms");
});
it("reports API latency without a fixed ceiling while enforcing page latency budgets",()=>{
  expect(evaluateEngineeringPerformance(input()).passed).toBe(true);
  const slow=input();slow.samples.forEach(sample=>{if(sample.kind==="pms")sample.elapsedMs=600;});
  const result=evaluateEngineeringPerformance(slow);
  expect(result.passed).toBe(true);
  expect(result.metrics.api.pms).toEqual({count:181,p95Ms:600});
  const slowPage=input();slowPage.samples.forEach(sample=>{if(sample.kind==="page"&&sample.pageKind==="pms")sample.elapsedMs=1100;});
  expect(evaluateEngineeringPerformance(slowPage).passed).toBe(false);
});
it("still rejects an unmeasured API route without a latency ceiling",()=>{
  const missing=input();missing.samples=missing.samples.filter(sample=>sample.kind!=="pms");
  expect(evaluateEngineeringPerformance(missing).passed).toBe(false);
});
it("rejects shortened, idle, restarted, skipped and5xx runs despite fast aggregate medians",()=>{
  const shortened=input();shortened.durationMs-=1;expect(evaluateEngineeringPerformance(shortened).passed).toBe(false);
  const idle=input();idle.samples=idle.samples.filter(sample=>sample.actor!=="account-9"||sample.atMs===0);expect(evaluateEngineeringPerformance(idle).passed).toBe(false);
  const failed=input();failed.samples[0].status=500;expect(evaluateEngineeringPerformance(failed).passed).toBe(false);
  const skipped=input();skipped.samples[0].validated=false;expect(evaluateEngineeringPerformance(skipped).passed).toBe(false);
  expect(evaluateEngineeringPerformance({...input(),sameProcesses:false}).passed).toBe(false);
});
