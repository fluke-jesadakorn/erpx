import fs from "node:fs";
import path from "node:path";
import { loadWorkspaceGraph } from "../testing/workspace-graph";
import { evaluateBaseline, inspectArchitecture } from "./boundaries";
import { runtimeLeafViolations } from "./runtime-leaves";

const root = path.resolve(import.meta.dirname, "../..");
const baselineFile = path.join(root, "scripts/architecture/legacy-edges.json");
const baseline = JSON.parse(fs.readFileSync(baselineFile, "utf8")) as { version: number; edges: string[] };
// Resolves explicit exports and rejects app-to-app / package-to-app imports.
const runtimeViolations = runtimeLeafViolations(loadWorkspaceGraph(root));
for (const issue of runtimeViolations) console.error(`${issue.source}: runtime leaf reaches business command composition: ${issue.target}`);
const result = evaluateBaseline(inspectArchitecture(root), baseline.edges);
if (process.argv.includes("--prune")) {
  const stale = new Set(result.stale);
  fs.writeFileSync(baselineFile, JSON.stringify({ ...baseline, edges: baseline.edges.filter((id) => !stale.has(id)) }, null, 2) + "\n");
} else if (result.stale.length) {
  console.error(`${result.stale.length} resolved legacy edges remain. Run architecture:prune to reduce the allowlist.`);
}
for (const issue of result.violations) console.error(`${issue.source}:${issue.line} ${issue.rule}: ${issue.target}`);
if (runtimeViolations.length || result.violations.length || (!process.argv.includes("--prune") && result.stale.length)) process.exitCode = 1;
else console.log(`Architecture boundaries passed; ${baseline.edges.length - result.stale.length} exact legacy edges remain.`);
