/** Continuous normal-login load and browser navigation against one isolated RC. */
import { chromium, type BrowserContext, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { closeSync, openSync, writeSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { Pool } from "pg";
import { evaluateEngineeringPerformance, engineeringPerformanceWorkspacePath, type PerformanceSample } from "@erpx/feature-runtime/lib/engineering-performance";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../../local/source-fingerprint";
import { frontWorkspacePath, signInFrontRole } from "../../../tests/support/helpers/front-simulation-session";

const root = resolve(process.cwd()), run = process.argv[2], output = process.argv[3];
if (!run || !/^[a-f0-9]{12}$/.test(run) || !output) throw new Error("EXACT_ISOLATED_RUN_AND_EVIDENCE_DIRECTORY_REQUIRED");
const directory = resolve(root, output), allowed = join(root, ".scratch/dev/engineering-completion/runs") + sep;
if (!directory.startsWith(allowed)) throw new Error("PRIVATE_ENGINEERING_EVIDENCE_DIRECTORY_REQUIRED");
await mkdir(directory, { recursive: true, mode: 0o700 });
const browserDirectory = join(root, ".runtime/browser", run);
const state = JSON.parse(await readFile(join(browserDirectory, "state.json"), "utf8")) as {
  run: string; prefix: string; origin: string; port: number; pid: number; jobsPid: number;
  sourceFingerprint: string; softwareVersion: string; release?: { id: string; fileManifestSha256: string };
  fixture: { companyId: string; hospitality: { companyId: string; propertyId: string } };
};
if (state.run !== run || state.prefix !== `erpx_browser_${run}` || state.port !== 3104 || state.origin !== "http://127.0.0.1:3104" || !state.release)
  throw new Error("PERFORMANCE_REQUIRES_ISOLATED_3104_COMPILED_RELEASE");
const sourceFingerprint = await runtimeSourceFingerprint(root);
const verificationFingerprint = await verificationSourceFingerprint(root, sourceFingerprint);
if (sourceFingerprint !== state.sourceFingerprint) throw new Error("PERFORMANCE_SOURCE_RELEASE_MISMATCH");
const credentials = JSON.parse(await readFile(join(browserDirectory, "credentials.json"), "utf8")) as {
  password: string; users: Array<{ userId: string; email: string; role: string; roles?: string[]; scope?: string }>;
};
const general = credentials.users.filter(user => !user.scope && user.email.endsWith("@simulation.test")).slice(0, 7);
const hotel = ["pms_frontdesk", "pms_cashier", "pms_manager"].map(role => credentials.users.find(user => user.role === role));
if (general.length !== 7 || hotel.some(user => !user)) throw new Error("TEN_DISTINCT_DEMO_ACCOUNTS_REQUIRED");
const selected = [...general.map(user => ({ user, holding: "simulation", role: user.roles?.includes("hr_admin") ? "hr_admin" : user.role })),
  ...hotel.map(user => ({ user: user!, holding: "riverside-lantern", role: user!.role }))];
if (new Set(selected.map(item => item.user.userId)).size !== 10) throw new Error("TEN_DISTINCT_DEMO_ACCOUNTS_REQUIRED");
const requestedDuration = Number(process.env.ERPX_ENGINEERING_PERFORMANCE_DURATION_MS ?? 1_800_000);
if (!Number.isInteger(requestedDuration) || requestedDuration < 10_000 || requestedDuration > 3_600_000) throw new Error("INVALID_PERFORMANCE_DURATION");
const samples: PerformanceSample[] = [], runtimeErrors = new Set<string>();
const databaseManifest = JSON.parse(await readFile(join(browserDirectory, "databases.json"), "utf8"));
const databaseUrl = new URL(databaseManifest.databases.H001_DB);
if (databaseUrl.hostname !== "127.0.0.1" || databaseUrl.port !== "5432" || databaseUrl.username !== "erpx_test_admin" || databaseUrl.pathname !== `/erpx_browser_${run}_h001_db`)
  throw new Error("PERFORMANCE_FIXTURE_DATABASE_MISMATCH");
const fixtureDb = new Pool({ connectionString: databaseUrl.href, max: 1 });
let knownPeople: Set<string>;
try { knownPeople = new Set((await fixtureDb.query("SELECT id FROM hr_people WHERE company_id=$1 AND merged_into_id IS NULL", [state.fixture.companyId])).rows.map(row => String(row.id))); }
finally { await fixtureDb.end(); }
const rawPath = join(directory, "performance-samples.jsonl"), writer = openSync(rawPath, "w", 0o600);
const browser = await chromium.launch({ args: ["--host-resolver-rules=MAP dev-erpx.localhost 127.0.0.1"] });
const contexts: Array<{ context: BrowserContext; page: Page; actor: string; holding: string; kind: "work"|"people"|"pms"; company: string }> = [];
let started = 0, sameProcesses = true, peakRssKiB = 0, completedDuration = 0;
const logOffsets = new Map<string, number>();
for (const name of ["server.log", "jobs.log"]) logOffsets.set(name, (await readFile(join(browserDirectory, name))).length);
function alive(pid: number) { try { process.kill(pid, 0); return true; } catch { return false; } }
async function monitor() {
  const current = JSON.parse(await readFile(join(browserDirectory, "state.json"), "utf8"));
  sameProcesses &&= alive(state.pid) && alive(state.jobsPid) && current.pid === state.pid && current.jobsPid === state.jobsPid;
  const memory = (spawnSync("ps", ["-p", `${state.pid},${state.jobsPid}`, "-o", "rss="], { encoding: "utf8" }).stdout ?? "").trim().split(/\s+/).map(Number);
  peakRssKiB = Math.max(peakRssKiB, memory.filter(Number.isFinite).reduce((sum, value) => sum + value, 0));
  for (const name of logOffsets.keys()) {
    const bytes = await readFile(join(browserDirectory, name)), delta = bytes.subarray(logOffsets.get(name)).toString();
    logOffsets.set(name, bytes.length);
    for (const pattern of ["out of memory", "heap out of memory", "remaining connection slots", "too many clients", "pool exhaustion", "timeout exceeded when trying to connect", "connection acquisition timeout"])
      if (delta.toLowerCase().includes(pattern)) runtimeErrors.add(pattern);
  }
}
function record(sample: PerformanceSample) { samples.push(sample); writeSync(writer, JSON.stringify(sample) + "\n"); }
async function readApi(account: typeof contexts[number], measured = true) {
  const base = `/api/v1/holdings/${account.holding}`, path = account.kind === "pms"
    ? `${base}/pms?property=${encodeURIComponent(state.fixture.hospitality.propertyId)}`
    : account.kind === "people" ? `${base}/people` : `${base}/work/summary?paged=1&view=actions`;
  const begin = performance.now(), atMs = measured ? begin - started : 0;
  let status = 0, validated = false;
  try {
    const response = await account.context.request.get(path, { timeout: 20_000 });
    status = response.status();
    const body = await response.json(), data = body.data;
    if (status === 200 && !body.error && data) {
      validated = account.kind === "work" ? Array.isArray(data.items) && typeof data.generatedAt === "number" && Array.isArray(data.unavailableSources) && data.unavailableSources.length === 0
        : account.kind === "people" ? Array.isArray(data) && data.length > 0 && data.every((person: { id?: string }) => typeof person.id === "string" && knownPeople.has(person.id))
        : !!data.snapshot && data.snapshot.property?.id === state.fixture.hospitality.propertyId && Array.isArray(data.snapshot.records) && data.snapshot.records.length > 0;
    }
  } catch { /* A failed/invalid response becomes a failing raw sample. */ }
  const sample = { kind: account.kind, actor: account.actor, atMs, elapsedMs: performance.now() - begin, status, validated } satisfies PerformanceSample;
  if (measured) record(sample);
  else if (!validated) throw new Error(`PERFORMANCE_WARMUP_RESPONSE_INVALID:${account.kind}:${status}`);
}
async function navigate(page: Page, path: string, actor: string, pageKind?: "work"|"people"|"pms", measured = true) {
  const begin = performance.now(), atMs = measured ? begin - started : 0;
  let status = 0, validated = false;
  try {
    const response = await page.goto(path, { waitUntil: "domcontentloaded", timeout: 20_000 });
    status = response?.status() ?? 0;
    await page.locator("main").first().waitFor({ state: "visible", timeout: 10_000 });
    const stale = page.locator("[data-work-stale]").first();
    if (await stale.count()) await stale.evaluate(element => new Promise<void>((ok, no) => {
      if (element.getAttribute("data-work-stale") === "false") return ok();
      const timer = setTimeout(() => { observer.disconnect(); no(new Error("PAGE_STALE")); }, 10_000);
      const observer = new MutationObserver(() => { if (element.getAttribute("data-work-stale") === "false") { clearTimeout(timer); observer.disconnect(); ok(); } });
      observer.observe(element, { attributes: true });
    }));
    const text = await page.locator("main").first().innerText();
    validated = status === 200 && !/\/(?:login|sign-in)(?:\?|$)/.test(page.url()) && text.trim().length > 20 && !/application error|internal server error|something went wrong/i.test(text);
  } catch { /* Kept as a failed raw navigation sample. */ }
  if (measured) record({ kind: pageKind ? "page" : "public", pageKind, actor, atMs, elapsedMs: performance.now() - begin, status, validated });
  else if (!validated) throw new Error(`PERFORMANCE_PAGE_WARMUP_INVALID:${pageKind ?? "public"}:${status}`);
}
try {
  for (const item of selected) {
    const context = await browser.newContext({ baseURL: state.origin });
    const login = await context.request.post("/api/auth/sign-in/email", { headers: { Origin: state.origin }, data: { email: item.user.email, password: credentials.password } });
    if (login.status() !== 200) throw new Error(`PERFORMANCE_NORMAL_LOGIN_FAILED:${login.status()}`);
    const endpoint = `/api/v1/holdings/${item.holding}/assignments/active`, active = (await (await context.request.get(endpoint)).json()).data;
    const assignment = active.options.find((option: { role: string; scopeKind: string }) => option.role === item.role && option.scopeKind === "holding");
    if (!assignment) throw new Error("PERFORMANCE_CURRENT_ASSIGNMENT_REQUIRED");
    const select = await context.request.post(endpoint, { headers: { Origin: state.origin }, data: { assignmentId: assignment.id, revision: active.revision } });
    if (select.status() !== 200) throw new Error("PERFORMANCE_ASSIGNMENT_SELECTION_FAILED");
    const page = await context.newPage();
    if (item.holding === "riverside-lantern")
      await signInFrontRole(context, page, credentials, item.role, "pms", state.fixture.hospitality.companyId, frontWorkspacePath(state.fixture.hospitality.companyId));
    contexts.push({ context, page, actor: item.user.userId, holding: item.holding,
      kind: item.holding === "riverside-lantern" ? "pms" : item.role === "hr_admin" ? "people" : "work",
      company: item.holding === "riverside-lantern" ? state.fixture.hospitality.companyId : state.fixture.companyId });
  }
  credentials.password = "";
  const publicContext = await browser.newContext(), publicPage = await publicContext.newPage(), publicOrigin = "http://dev-erpx.localhost:3104/";
  const pageAccounts = (["work", "people", "pms"] as const).map(kind => contexts.find(account => account.kind === kind)!);
  await Promise.all(contexts.map(account => readApi(account, false)));
  for (const account of pageAccounts) await navigate(account.page, engineeringPerformanceWorkspacePath(account.holding, account.kind), account.actor, account.kind, false);
  await navigate(publicPage, publicOrigin, "anonymous", undefined, false);
  started = performance.now();
  console.log(JSON.stringify({ event: "performance_started", run, accounts: 10, requestedDurationMs: requestedDuration, sourceFingerprint }));
  let tick = 0, lastUpdate = 0;
  while (performance.now() - started < requestedDuration) {
    const tickBegin = performance.now();
    await Promise.all(contexts.map(account => readApi(account)));
    if (tick % 3 === 0) {
      await Promise.all(pageAccounts.map(account => navigate(account.page, engineeringPerformanceWorkspacePath(account.holding, account.kind), account.actor, account.kind)));
      await navigate(publicPage, publicOrigin, "anonymous");
      await monitor();
    }
    const elapsed = performance.now() - started;
    if (elapsed - lastUpdate >= 55_000) { console.log(JSON.stringify({ event: "performance_progress", elapsedSeconds: Math.floor(elapsed / 1000), samples: samples.length })); lastUpdate = elapsed; }
    tick++;
    const wait = Math.min(5_000 - (performance.now() - tickBegin), requestedDuration - (performance.now() - started));
    if (wait > 0) await sleep(wait);
  }
  completedDuration = performance.now() - started;
  await monitor();
} finally {
  closeSync(writer); await chmod(rawPath, 0o600);
  await browser.close();
}
const afterSource = await runtimeSourceFingerprint(root), afterVerification = await verificationSourceFingerprint(root, afterSource);
if (afterSource !== sourceFingerprint || afterVerification !== verificationFingerprint) runtimeErrors.add("Source/assertions changed during the continuous run.");
const result = evaluateEngineeringPerformance({ samples, durationMs: completedDuration, accountIds: contexts.map(account => account.actor), sameProcesses, runtimeErrors: [...runtimeErrors] });
const report = { version: 1, run, sourceFingerprint, verificationFingerprint, softwareVersion: state.softwareVersion, release: state.release,
  origin: state.origin, publicOrigin: "http://dev-erpx.localhost:3104/", environment: "isolated local standalone; normal Better Auth accounts; public host fixture",
  startedAt: new Date(Date.now() - completedDuration).toISOString(), durationMs: completedDuration, accountIds: contexts.map(account => account.actor),
  processes: { webPid: state.pid, jobsPid: state.jobsPid, sameProcesses, peakRssKiB }, runtimeErrors: [...runtimeErrors],
  samplesArtifact: "performance-samples.jsonl", samplesSha256: createHash("sha256").update(await readFile(rawPath)).digest("hex"), ...result };
await writeFile(join(directory, "performance.json"), JSON.stringify(report, null, 2) + "\n", { mode: 0o600 });
console.log(JSON.stringify({ event: "performance_finished", passed: result.passed, durationSeconds: Math.floor(completedDuration / 1000), metrics: result.metrics, errors: result.errors }));
process.exitCode = result.passed ? 0 : 1;
