import { mkdir, readFile, readdir, realpath, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { aggregateCiRegression, parseBrowserShard, requiredNativePhases, type BrowserCaseExclusion, type BrowserCaseReceipt, type BrowserCiReceipt, type NativeCiReceipt } from "./browser-sharding";
import { loadCatalogue, repositoryRoot, validateCatalogue } from "./catalogue";
import { runBrowserIsolation } from "./browser-isolation";
import { runtimeSourceFingerprint, verificationSourceFingerprint } from "../local/source-fingerprint";
import { internalRequiredCases, loadRequiredCases, readNativeCaseReceipt } from "./required-case-inventory";
import { preparePdfAssets } from "../assets/prepare-pdf-assets";

const root = repositoryRoot, catalogue = loadCatalogue();
validateCatalogue(catalogue);
const args = process.argv.slice(2), mode = args.shift();
if (!["native", "browser", "aggregate"].includes(mode ?? "")) throw new Error("CI_REGRESSION_MODE_REQUIRED");
const option = (name: string) => args.find(argument => argument.startsWith(`${name}=`))?.slice(name.length + 1);
if (args.some(argument => !(mode === "browser" && argument === "--build") && !argument.startsWith(mode === "browser" ? "--shard=" : mode === "aggregate" ? "--directory=" : "<no-options>")) || args.length > (mode === "native" ? 0 : mode === "browser" ? 2 : 1) || new Set(args.map(argument => argument.split("=")[0])).size !== args.length) throw new Error("UNKNOWN_CI_REGRESSION_ARGUMENT");
const candidate = Bun.spawnSync(["git", "rev-parse", "HEAD"], { cwd: root }).stdout.toString().trim();
if (process.env.ERPX_TEST_CANDIDATE_SHA && process.env.ERPX_TEST_CANDIDATE_SHA !== candidate) throw new Error("CI_CANDIDATE_SHA_MISMATCH");
const browserSuites = catalogue.suites.filter(suite => ["e2e", "journeys", "uat"].includes(suite.lane));
const requiredCases = internalRequiredCases(loadRequiredCases(root), catalogue.suites);
const directory = join(root, ".scratch/dev/test-suites/ci");
await mkdir(directory, { recursive: true, mode: 0o700 });
const children = new Set<ReturnType<typeof Bun.spawn>>();
let interrupted: string | undefined;
const signal = (name: "SIGINT" | "SIGTERM") => { interrupted = name; for (const child of children) child.kill(name); };
process.on("SIGINT", () => signal("SIGINT")); process.on("SIGTERM", () => signal("SIGTERM"));

if (mode === "aggregate") {
  const source = option("--directory");
  if (!source) throw new Error("CI_EVIDENCE_DIRECTORY_REQUIRED");
  async function files(path: string): Promise<string[]> {
    return (await Promise.all((await readdir(path, { withFileTypes: true })).map(entry => entry.isDirectory() ? files(join(path, entry.name)) : /^(?:native|browser-\d+-\d+)\.json$/.test(entry.name) ? [join(path, entry.name)] : []))).flat();
  }
  const receipts = await Promise.all((await files(resolve(root, source))).map(async path => JSON.parse(await readFile(path, "utf8")) as NativeCiReceipt | BrowserCiReceipt));
  const natives = receipts.filter(receipt => receipt.kind === "native") as NativeCiReceipt[];
  const browsers = receipts.filter(receipt => receipt.kind === "browser") as BrowserCiReceipt[];
  if (natives.length !== 1 || !browsers.length) throw new Error("FULL_CI_RECEIPTS_REQUIRED");
  const receipt = aggregateCiRegression({ native: natives[0], browsers, candidate, total: browsers[0].shard.total, requiredSuiteIds: browserSuites.map(suite => suite.id), requiredCases });
  await writeFile(join(directory, "aggregate.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 });
  console.log(JSON.stringify(receipt));
} else {
  await preparePdfAssets();
  const runtimeFingerprint = await runtimeSourceFingerprint(root), verificationFingerprint = await verificationSourceFingerprint(root, runtimeFingerprint);
  async function stable() {
    if (await runtimeSourceFingerprint(root) !== runtimeFingerprint || await verificationSourceFingerprint(root, runtimeFingerprint) !== verificationFingerprint) throw new Error("CI_REGRESSION_SOURCE_CHANGED");
  }
  async function execute(command: string[], environment: Record<string, string> = {}) {
    await stable();
    if (interrupted) throw new Error(`CI_REGRESSION_INTERRUPTED:${interrupted}`);
    const child = Bun.spawn([process.execPath, ...command], { cwd: root, env: { ...process.env, ...environment }, stdin: "inherit", stdout: "inherit", stderr: "inherit" });
    children.add(child);
    try { if (await child.exited !== 0) throw new Error(`CI_REGRESSION_COMMAND_FAILED:${command.join(" ")}`); }
    finally { children.delete(child); }
    await stable();
  }
  async function verifiedRelease(path: string) {
    const releaseDirectory = await realpath(path), release = JSON.parse(await readFile(join(releaseDirectory, "release.json"), "utf8"));
    if (release.sourceFingerprint !== runtimeFingerprint) throw new Error("CI_RELEASE_CANDIDATE_MISMATCH");
    const bytes = await readFile(join(releaseDirectory, "release-files.json"));
    if (createHash("sha256").update(bytes).digest("hex") !== release.fileManifestSha256) throw new Error("CI_RELEASE_FILE_MANIFEST_MISMATCH");
    for (const [name, digest] of Object.entries(JSON.parse(bytes.toString()) as Record<string, string>)) {
      const file = resolve(releaseDirectory, name);
      if (!file.startsWith(`${releaseDirectory}/`) || createHash("sha256").update(await readFile(file)).digest("hex") !== digest) throw new Error(`CI_RELEASE_FILE_MISMATCH:${name}`);
    }
    return { releaseDirectory, identity: { id: String(release.id), fileManifestSha256: String(release.fileManifestSha256) } };
  }
  if (mode === "native") {
    const receipt: NativeCiReceipt = { version: 1, kind: "native", candidate, runtimeFingerprint, verificationFingerprint, passed: false, phases: [], cases: [] };
    const commands: Record<typeof requiredNativePhases[number], string[]> = { architecture: ["scripts/architecture/acceptance.ts"], unit: ["scripts/testing/test.ts", "unit"], integration: ["scripts/testing/test.ts", "integration"], lint: ["run", "lint"], typecheck: ["run", "typecheck"], "isolated-release": ["run", "deploy:build"] };
    try {
      for (const phase of requiredNativePhases) {
        const native = phase === "unit" || phase === "integration", caseReceipt = join(directory, `${phase}-cases.json`);
        await execute(commands[phase], native ? { ERPX_TEST_CASE_RECEIPT: caseReceipt } : {});
        if (native) receipt.cases.push(...readNativeCaseReceipt(caseReceipt, catalogue.suites.filter(suite => suite.lane === phase).flatMap(suite => suite.files), requiredCases).cases);
        receipt.phases.push({ phase, passed: true });
      }
      const release = await verifiedRelease(join(root, ".runtime/releases/latest")); receipt.release = release.identity;
      const archive = Bun.spawn(["tar", "-cf", join(directory, "candidate-release.tar"), "-C", release.releaseDirectory, "."], { cwd: root, stdout: "inherit", stderr: "inherit" });
      children.add(archive);
      try { if (await archive.exited !== 0) throw new Error("CI_RELEASE_ARCHIVE_FAILED"); } finally { children.delete(archive); }
      await stable(); receipt.passed = true;
    }
    finally { await writeFile(join(directory, "native.json"), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 }); }
  } else {
    const shard = parseBrowserShard(option("--shard") ?? "");
    const receipt: BrowserCiReceipt = { version: 1, kind: "browser", candidate, runtimeFingerprint, verificationFingerprint, passed: false, shard, inventory: [], receipts: [], exclusions: [], isolation: null };
    try {
      if (args.includes("--build")) await execute(["run", "deploy:build"]);
      if (!process.env.ERPX_TEST_RELEASE_DIR && !args.includes("--build")) throw new Error("CI_ATTESTED_RELEASE_DIRECTORY_REQUIRED");
      const { releaseDirectory, identity } = await verifiedRelease(process.env.ERPX_TEST_RELEASE_DIR ?? join(root, ".runtime/releases/latest"));
      receipt.release = identity;
      const isolated = await runBrowserIsolation(browserSuites, { releaseDirectory, shard });
      const browser = isolated.forward.receipt;
      if (!browser.passed || browser.runtimeFingerprint !== runtimeFingerprint || browser.verificationFingerprint !== verificationFingerprint) throw new Error("CI_BROWSER_RECEIPT_MISMATCH");
      receipt.inventory = browser.inventory;
      receipt.receipts = browser.receipts.filter((test): test is BrowserCaseReceipt => test.required !== false && typeof test.file === "string" && typeof test.title === "string" && typeof test.passed === "boolean");
      receipt.exclusions = browser.receipts.filter((test): test is BrowserCaseExclusion => test.required === false && typeof test.file === "string" && typeof test.title === "string" && typeof test.reason === "string");
      receipt.isolation = isolated.proof;
      await stable(); receipt.passed = true;
    } finally { await writeFile(join(directory, `browser-${shard.index}-${shard.total}.json`), JSON.stringify(receipt, null, 2) + "\n", { mode: 0o600 }); }
  }
}
