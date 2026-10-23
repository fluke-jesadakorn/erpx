import { resolve } from "node:path";
import { readFileSync } from "node:fs";
import { consoleWorkspaceCutoverBlockers } from "./console-workspace-cutover";

const args = process.argv.slice(2);
if (args.length) {
  if (args.length !== 1 || args[0] !== "--development-workspaces") throw new Error("UNKNOWN_CONSOLE_RETIREMENT_OPTION");
  const root = resolve(import.meta.dirname, "../..");
  const settings = JSON.parse(readFileSync(resolve(root, "scripts/local/workspace-settings.json"), "utf8"));
  if (settings.version !== 1 || !Array.isArray(settings.activeApps) || settings.activeApps.some((id: unknown) => typeof id !== "string")) throw new Error("INVALID_WORKSPACE_SETTINGS");
  const blockers = consoleWorkspaceCutoverBlockers(settings.activeApps, process.env);
  console.log(JSON.stringify({ acceptance: false, scope: "development-console-mapping-preflight", ready: blockers.length === 0, blockers,
    note: "Read-only mapping preflight. Scoped app/browser verification and explicit activation remain required." }));
  process.exit(blockers.length ? 1 : 0);
}

// Necessary activation preflight, not a substitute for the final UI/provider,
// performance and restore/rollback receipts in docs/production-plan.md.
const required = ["WORK", "SUPPORT", "SERVICE", "POS", "PMS", "FINANCE", "PEOPLE", "COMMERCE", "PROJECTS", "ASSETS", "LEARNING", "MANUFACTURING", "TALENT", "LEGAL"];
const blockers: string[] = [];
if (process.env.ERPX_ENVIRONMENT !== "local" || process.env.ERPX_PRODUCT_ENVIRONMENT !== "development") blockers.push("Development environment required");
if (!/^https:\/\/console-dev-erpx\.jesadakorn\.com\/?$/.test(process.env.ERPX_PUBLIC_ORIGIN ?? "")) blockers.push("Exact legacy development Console origin required");
for (const key of ["ERPX_PRODUCT_ROUTING", "ERPX_PRODUCT_SSO"])
  if (process.env[key] !== "enabled") blockers.push(`${key} is not enabled`);
for (const key of required.map(owner => `ERPX_${owner}_PACKAGE_ENFORCEMENT`))
  if (process.env[key] !== "enforced") blockers.push(`${key} is not enforced`);

const check = Bun.spawn([process.execPath, resolve("scripts/docs/generate-product-acceptance.ts"), "--acceptance"], {
  cwd: process.cwd(), env: process.env, stdout: "pipe", stderr: "pipe",
});
// Consume both streams so a long list of pending workflow receipts cannot block.
const [status, stdout, stderr] = await Promise.all([check.exited, new Response(check.stdout).text(), new Response(check.stderr).text()]);
if (status !== 0) {
  const pending = (stderr.match(/Pending current-source (?:implemented|verified|enabled): [A-Za-z0-9_/-]+/g) ?? []).length;
  blockers.push(`Current-source product acceptance failed${pending ? ` (${pending} missing workflow phase receipts)` : ""}; run bun run products:acceptance for details`);
}
if (blockers.length) {
  console.error("CONSOLE_RETIREMENT_NOT_READY\n" + blockers.map(value => `- ${value}`).join("\n"));
  process.exitCode = 1;
} else {
  console.log(stdout.trim());
  console.log("CONSOLE_RETIREMENT_PREFLIGHT_PASSED: reconcile final provider/UI/performance/recovery receipts before deliberate activation.");
}
