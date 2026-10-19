import { createHash } from "node:crypto";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join, relative, resolve, sep } from "node:path";
import registry from "../../../src/data/module-registry.json";
import { engineeringManifestSchema, evaluateEngineeringCompletion, type EngineeringGate, type EngineeringGateId, type EngineeringObservation } from "../../../src/lib/runtime/engineering-completion";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../../local/source-fingerprint";
import { vitestObservations, playwrightObservations } from "./engineering-test-results";
import { drainEngineeringProcess, runConcurrentEngineeringVerification, runReleaseVerification, serializeEngineeringEvidenceWrites } from "./engineering-release-verification";
import {reassessEngineeringGates} from "./engineering-gate-validation";

const root = resolve(import.meta.dir, "../../.."), register = join(root, "tests/catalogue/engineering-scenarios.json");
const registrySha256 = createHash("sha256").update(await readFile(join(root, "src/data/module-registry.json"))).digest("hex");
if (registry.length !== 33 || registry.flatMap(module => module.sections.filter(section => section.capability)).length !== 187)
  throw new Error("CATALOGUE_SCOPE_CHANGED_REVIEW_REQUIRED");
const mode = process.argv[2] ?? "check";
const node = join(root, ".runtime/node/node-v26.11.1-darwin-arm64/bin/node");
if (Bun.version !== "1.4.3" || Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v26.11.1")
  throw new Error("PINNED_TOOLCHAIN_REQUIRED");
async function privateJson(path: string, value: unknown) {
  await writeFile(path, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
  await chmod(path, 0o600);
}
async function fingerprints() {
  const sourceFingerprint = await runtimeSourceFingerprint(root);
  return { sourceFingerprint, verificationFingerprint: await verificationSourceFingerprint(root, sourceFingerprint) };
}
if (mode === "inventory") {
  if (existsSync(register)) {
    engineeringManifestSchema.parse(JSON.parse(await readFile(register, "utf8")));
    console.log("Existing register preserved; review mappings instead of overwriting evidence.");
  } else {
    await writeFile(register, JSON.stringify({ version: 1, scope: "catalogue-engineering-completion-v1", registrySha256,
      modules: registry.map(module => module.id), capabilities: registry.flatMap(module => module.sections.filter(section => section.capability).map(section => ({
        id: `${module.id}/${section.id}`, moduleId: module.id, capabilityId: section.id,
        workflow: [section.description], expectedResults: [`Persisted ${section.label} lifecycle, authorized handoff and history remain correct after reload.`],
        review: "pending", cases: [],
      }))) }, null, 2) + "\n");
    console.log("Created 33-module / 187-capability register. All mappings await actual assertion review; no capability is passed.");
  }
  process.exit(0);
}
const manifest = engineeringManifestSchema.parse(JSON.parse(await readFile(register, "utf8")));
type Run = { version: 1; runId: string; createdAt: string; sourceFingerprint: string; verificationFingerprint: string;
  registrySha256: string; observations: EngineeringObservation[]; gates: EngineeringGate[] };
let directory: string, run: Run;
if (mode === "check") {
  const supplied = process.argv[3];
  if (!supplied) throw new Error("EXACT_ENGINEERING_RUN_DIRECTORY_REQUIRED");
  directory = resolve(root, supplied);
  const allowed = join(root, ".scratch/dev/engineering-completion/runs") + sep;
  if (!directory.startsWith(allowed)) throw new Error("ENGINEERING_RUN_OUTSIDE_PRIVATE_EVIDENCE_DIRECTORY");
  run = JSON.parse(await readFile(join(directory, "run.json"), "utf8")) as Run;
  for (const gate of run.gates) {
    const path = resolve(directory, gate.artifact);
    if (!path.startsWith(directory + sep) || createHash("sha256").update(await readFile(path)).digest("hex") !== gate.sha256)
      throw new Error(`ENGINEERING_EVIDENCE_CHECKSUM_MISMATCH:${gate.id}`);
  }
  // Derive case outcomes from checksum-verified runner reports, never editable
  // aggregate counters or the convenient cached observations in run.json.
  run.observations = [];
  for (const gate of run.gates) {
    if (gate.id === "integration" && gate.artifact.endsWith(".json"))
      run.observations.push(...vitestObservations(root, JSON.parse(await readFile(join(directory, gate.artifact), "utf8"))));
    if (gate.id === "browser" && gate.artifact.endsWith(".json"))
      run.observations.push(...playwrightObservations(root, JSON.parse(await readFile(join(directory, gate.artifact), "utf8"))));
  }
} else if (["integration", "verify"].includes(mode)) {
  const runId = new Date().toISOString().replace(/[:.]/g, "-");
  directory = join(root, ".scratch/dev/engineering-completion/runs", runId);
  await mkdir(directory, { recursive: true, mode: 0o700 });
  run = { version: 1, runId, createdAt: new Date().toISOString(), registrySha256, ...await fingerprints(), observations: [], gates: [] };
  await privateJson(join(directory, "run.json"), run);
  const persistEvidence=serializeEngineeringEvidenceWrites();
  const phases: Array<{ id: EngineeringGateId; args: string[] }> = mode === "verify" ? [
    { id: "typecheck", args: ["run", "typecheck"] }, { id: "lint", args: ["run", "lint"] },
    { id: "integration", args: ["scripts/testing/test.ts", "integration", "--reporter=json", `--outputFile=${join(directory, "vitest.json")}`] },
    { id: "release-build", args: ["run", "deploy:build"] },
  ] : [{ id: "integration", args: ["scripts/testing/test.ts", "integration", "--reporter=json", `--outputFile=${join(directory, "vitest.json")}`] }];
  const executePhase = async (phase: typeof phases[number]) => {
    console.log(`${phase.id}: running; evidence ${relative(root, directory)}`);
    const before = await fingerprints(), startedAt = new Date().toISOString();
    const logPath = join(directory, `${phase.id}.log`), log = Bun.file(logPath).writer();
    let exitCode = -1, childExitCode: number | undefined;
    try {
      const child = Bun.spawn([globalThis.process.execPath, ...phase.args], {
        cwd: root, env: { ...globalThis.process.env, ERPX_NODE_BIN: node }, stdout: "pipe", stderr: "pipe",
      });
      ({exitCode,childExitCode}=await drainEngineeringProcess(child,bytes=>log.write(bytes)));
    } catch { /* A spawn failure is a failed, still checksum-bound phase. */ }
    const endedAt = new Date().toISOString();
    await log.end(); await chmod(logPath, 0o600);
    const after = await fingerprints();
    const stable = [before, after].every(value => value.sourceFingerprint === run.sourceFingerprint && value.verificationFingerprint === run.verificationFingerprint);
    const phaseArtifact=`${phase.id}.phase.json`;
    await privateJson(join(directory,phaseArtifact),{id:phase.id,exitCode,childExitCode,startedAt,endedAt,before,after,sourceStable:stable,sourceFingerprint:run.sourceFingerprint,verificationFingerprint:run.verificationFingerprint,log:`${phase.id}.log`,logSha256:createHash("sha256").update(await readFile(logPath)).digest("hex")});
    const artifact = phase.id === "integration" && existsSync(join(directory, "vitest.json")) ? "vitest.json" : phaseArtifact;
    const gate: EngineeringGate = { id: phase.id, status: exitCode === 0 && stable ? "passed" : "failed",
      sourceFingerprint: run.sourceFingerprint, verificationFingerprint: run.verificationFingerprint, artifact,
      sha256: createHash("sha256").update(await readFile(join(directory, artifact))).digest("hex") };
    const observations: EngineeringObservation[] = [];
    if (phase.id === "integration") {
      if(artifact!=="vitest.json")gate.status="failed";
      else {
        await chmod(join(directory, artifact), 0o600);
        try {
          const report=JSON.parse(await readFile(join(directory, artifact), "utf8"));
          observations.push(...vitestObservations(root, report));
          if(report.success!==true||!observations.length||observations.some(item=>item.status==="failed"))gate.status="failed";
        }catch { gate.status = "failed"; }
      }
    }
    console.log(`${phase.id}: ${gate.status}${stable ? "" : " (source changed during verification)"}`);
    return {gate, observations};
  };
  const recordPhase = (result: Awaited<ReturnType<typeof executePhase>>) => persistEvidence(async()=>{
    run.gates.push(result.gate); run.observations.push(...result.observations);
    await privateJson(join(directory, "run.json"), run);
  });
  for (const phase of mode === "verify" ? phases.slice(0, 2) : phases) {
    const result = await executePhase(phase); await recordPhase(result);
    if (result.gate.status !== "passed") break;
  }
  if (mode === "verify" && run.gates.length === 2 && run.gates.every(gate => gate.status === "passed")) {
    const results=await runConcurrentEngineeringVerification({
      integration:()=>executePhase(phases[2]),recordIntegration:recordPhase,
      release:async waitForIntegration=>{
        const build=await executePhase(phases[3]);await recordPhase(build);
        if(build.gate.status!=="passed")return;
        await runReleaseVerification({root,directory,sourceFingerprint:run.sourceFingerprint,verificationFingerprint:run.verificationFingerprint,manifest,
          waitForIntegration,add:(gate,observations=[])=>persistEvidence(async()=>{
            run.gates=run.gates.filter(prior=>prior.id!==gate.id);run.gates.push(gate);run.observations.push(...observations);
            await privateJson(join(directory,"run.json"),run);console.log(`${gate.id}: ${gate.status}`);
          })});
      },
    });
    if(results[0].status==="rejected")
      await privateJson(join(directory,"phase-execution-failure.json"),{message:"PHASE_EVIDENCE_WRITE_FAILED"});
    if(results[1].status==="rejected") {
      const error=results[1].reason,message=error instanceof Error&&/^[A-Z0-9_]+$/.test(error.message)?error.message:"RELEASE_VERIFICATION_FAILED_SEE_PRIVATE_LOG";
      await privateJson(join(directory,"verification-failure.json"),{message});console.log(message);
    }
  }
} else throw new Error("Use inventory, integration, verify, or check <private-run-directory>.");

const current = await fingerprints();
run.gates=await reassessEngineeringGates({root,directory,sourceFingerprint:run.sourceFingerprint,verificationFingerprint:run.verificationFingerprint,gates:run.gates,observations:run.observations});
const report = evaluateEngineeringCompletion({ manifest, registry, registrySha256, ...current, observations: run.observations, gates: run.gates });
if(existsSync(join(directory,"phase-execution-failure.json"))||existsSync(join(directory,"verification-failure.json")))
  report.issues.push("A phase execution or evidence write failed; this run is incomplete.");
if (current.sourceFingerprint !== run.sourceFingerprint || current.verificationFingerprint !== run.verificationFingerprint)
  report.issues.push("The current source/assertions differ from this run; rerun on a frozen release candidate.");
if (run.registrySha256 !== registrySha256) report.issues.push("Run catalogue differs from the current register.");
report.complete = report.complete && report.issues.length === 0;
await privateJson(join(directory, "completion.json"), { ...report, sourceFingerprint: current.sourceFingerprint, verificationFingerprint: current.verificationFingerprint,
  environment: "isolated local PostgreSQL; provider sandboxes/fixtures; engineering only", assessedAt: new Date().toISOString() });
const lines = ["# ERPX engineering completion evidence", "", `Run: ${run.runId}`, `Runtime source: ${run.sourceFingerprint}`, `Verification source: ${run.verificationFingerprint}`, "",
  `Modules: ${report.passedModules}/${report.moduleCount}; capabilities: ${report.passedCapabilities}/${report.capabilityCount}; complete: ${report.complete}`, "",
  "These results do not grant Company, legal, bank, device or provider production certification.", "", "| Capability | Result | Missing proof |", "| --- | --- | --- |",
  ...report.capabilities.map(item => `| ${item.id} | ${item.passed ? "passed" : "incomplete"} | ${item.errors.join("; ").replaceAll("|", "\\|")} |`), "", ...report.issues.map(issue => `- ${issue}`), ""];
await writeFile(join(directory, "completion.md"), lines.join("\n"), { mode: 0o600 });
console.log(JSON.stringify({ runDirectory: relative(root, directory), modules: `${report.passedModules}/${report.moduleCount}`, capabilities: `${report.passedCapabilities}/${report.capabilityCount}`,
  requiredGates: report.gates.filter(gate => gate.passed).length, complete: report.complete }));
process.exitCode = report.complete ? 0 : 1;
