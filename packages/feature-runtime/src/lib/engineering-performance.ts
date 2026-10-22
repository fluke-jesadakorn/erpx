export type PerformanceSample = { kind: "work"|"people"|"pms"|"page"|"public"; actor: string; elapsedMs: number; atMs: number; status: number; validated: boolean; pageKind?: "work"|"people"|"pms" };
export function engineeringPerformanceWorkspacePath(holding: string, kind: "work"|"people"|"pms") {
  const section = kind === "people" ? "people-organization/people" : kind;
  return `/holdings/${encodeURIComponent(holding)}/${section}`;
}
export function percentile(values: number[], fraction: number) {
  const sorted=[...values].sort((a,b)=>a-b);
  return sorted.length ? sorted[Math.max(0,Math.ceil(sorted.length*fraction)-1)]! : null;
}
export function evaluateEngineeringPerformance(input:{samples:PerformanceSample[];durationMs:number;accountIds:string[];sameProcesses:boolean;runtimeErrors:string[]}) {
  const errors:string[]=[],accounts=[...new Set(input.accountIds)];
  if (accounts.length!==10) errors.push("Exactly10 distinct normal-login accounts are required.");
  if (input.durationMs<30*60_000) errors.push("The continuous run must cover30minutes.");
  if (!input.sameProcesses) errors.push("A web/jobs process exited or restarted.");
  if (input.runtimeErrors.length) errors.push("Runtime logs contain pool/memory failures.");
  if (input.samples.some(sample=>!Number.isFinite(sample.elapsedMs)||sample.elapsedMs<0||sample.atMs<0||sample.atMs>input.durationMs||sample.status>=500||sample.status<200||sample.status>=400||!sample.validated))
    errors.push("A request failed or lacked response-content assertions.");
  for (const actor of accounts) {
    const samples=input.samples.filter(sample=>sample.actor===actor&&sample.kind!=="page"&&sample.kind!=="public");
    if (samples.length<170 || !samples.some(sample=>sample.atMs<=60_000) || !samples.some(sample=>sample.atMs>=input.durationMs-60_000))
      errors.push(`Account${actor} did not remain active throughout the run.`);
    const times=samples.map(sample=>sample.atMs).sort((a,b)=>a-b);
    if (times.some((at,index)=>index>0&&at-times[index-1]!>30_000)) errors.push(`Account${actor} has a gap over30seconds.`);
  }
  const api=Object.fromEntries(["work","people","pms"].map(kind=>{
    const samples=input.samples.filter(sample=>sample.kind===kind),p95=percentile(samples.map(sample=>sample.elapsedMs),0.95);
    // API latency is reported, without a fixed acceptance ceiling (user decision).
    if (samples.length<170||p95===null) errors.push(`${kind} read API requires continuous measured samples.`);
    return [kind,{count:samples.length,p95Ms:p95}];
  }));
  const pages=Object.fromEntries(["work","people","pms"].map(kind=>{
    const samples=input.samples.filter(sample=>sample.kind==="page"&&sample.pageKind===kind),median=percentile(samples.map(sample=>sample.elapsedMs),0.5);
    if (samples.length<10||median===null||median>1_000) errors.push(`${kind} warm page requires median≤1000ms.`);
    return [kind,{count:samples.length,medianMs:median}];
  }));
  const publicSamples=input.samples.filter(sample=>sample.kind==="public"),publicMedian=percentile(publicSamples.map(sample=>sample.elapsedMs),0.5);
  if (publicSamples.length<30||publicMedian===null||publicMedian>1_500) errors.push("Public navigation requires median≤1500ms.");
  return {passed:errors.length===0,errors,metrics:{api,pages,public:{count:publicSamples.length,medianMs:publicMedian}}};
}
