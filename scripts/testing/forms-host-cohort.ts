/** Explicit one-origin HTTPS transport for the public Forms workflow only. */
import { createHash, createPrivateKey, createPublicKey, X509Certificate } from "node:crypto";
import { chmod, lstat, open, readFile, realpath, rename, writeFile } from "node:fs/promises";
import { createServer as createHttpServer, request as httpRequest, type IncomingHttpHeaders, type IncomingMessage } from "node:http";
import { createServer as createHttpsServer } from "node:https";
import { createServer as createNetServer, Socket } from "node:net";
import { execFileSync, fork, type ChildProcess } from "node:child_process";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { homedir } from "node:os";

export const formsHostOrigin = "https://forms-dev-erpx.jesadakorn.com" as const;
export const formsHostWorkflow = "forms.versioned-consent-submit-replay" as const;
export const formsHostBrowserFile = "tests/e2e/marketing/forms-host.spec.ts" as const;
export const formsHostCaseTitle = "isolated anonymous Forms host submits the published version and replays exact consent once" as const;
const formsHostname = new URL(formsHostOrigin).host;
const schema = "erpx-owned-forms-host-browser/v1" as const;
const maxBodyBytes = 32 * 1024;
const maxRefusals = 32;
const databaseNames = ["CONTROL_DB", "CONTROL_READ_DB", "H001_DB", "H001_READ_DB", "H002_DB", "H002_READ_DB", "H003_DB", "H003_READ_DB"] as const;

type Binding = { path: string; sha256: string };
type FileBinding = Binding & { bytes: number };
type Release = { id: string; sourceFingerprint: string; node: "26.11.1"; fileManifestSha256: string };
type FormsTls = { certificate: Binding; key: Binding; ca: Binding; spki: string };
type FormsTools = { node: Binding; chromium: Binding };

/** The plan deliberately has no Product Host, Commercial, MFA, Apply or package-enforcement switches. */
export type FormsHostPlan = {
  schema: "erpx-forms-host-arm/v1";
  version: 1;
  enabled: true;
  expiresAt: number;
  workflow: typeof formsHostWorkflow;
  runtimeFingerprint: string;
  verificationFingerprint: string;
  release: Binding;
  releaseFiles: Binding;
  tls: FormsTls;
  tools: FormsTools;
};

export type FormsHostFixture = {
  slug: "simulation";
  formId: string;
  formVersion: number;
  mediaPublicationId?: string;
};

export type FormsHostRequestState = { acceptedPosts: number; firstBody?: string; firstIdempotencyKey?: string };
export type FormsHostRequestKind = "forms-page" | "public-form-read" | "approved-media-read" | "static-read" | "submission" | "replay";
export type FormsHostPolicyInput = {
  method: string;
  origin: string;
  path: string;
  search?: string;
  headers: Record<string, string | undefined>;
  body: string;
  fixture: FormsHostFixture;
  run: string;
  prior: FormsHostRequestState;
};

export type FormsHostLease = {
  schema: typeof schema;
  run: string;
  pid: number;
  port: 3102;
  upstream: "http://127.0.0.1:3104";
  sourceFingerprint: string;
  verificationFingerprint: string;
  release: { id: string; fileManifestSha256: string };
  workflow: typeof formsHostWorkflow;
  origin: typeof formsHostOrigin;
  formsOnly: true;
  productHosts: false;
  executiveWorkHosts: false;
  commercial: false;
  mfa: false;
  apply: false;
  packageEnforcement: false;
  wholeProductAcceptance: false;
  fixture: FormsHostFixture;
  plan: Binding;
  certificate: Binding;
  key: Binding;
  certificateSha256: string;
  ca: Binding;
  chromium: Binding;
  spki: string;
  databases: FileBinding;
  fixtureGuide: FileBinding;
  requestEvidence: {
    acceptedPosts: 0 | 1 | 2;
    replayCount: 0 | 1;
    bodySha256?: string;
    idempotencyKeySha256?: string;
    refusalCount: number;
    refusalLog: { method: string; pathSha256: string }[];
  };
  ui: {
    version: 1;
    observedAt: string;
    listener: { address: "127.0.0.1"; port: 3102; pid: number };
    node: Binding;
    source: Binding;
    backend: {
      pid: number;
      jobsPid: number;
      webEntrypoint: string;
      jobsEntrypoint: string;
      state: FileBinding;
    };
  };
};

export type FormsHostHistoricalTlsEvidence = {
  certificateHostMatches: boolean;
  certificateIssuerMatchesCa: boolean;
  certificateValidFrom: number;
  certificateValidTo: number;
  certificateSpki: string;
  certificateSignatureValid: boolean;
  caIsAuthority: boolean;
  privateKeySpki: string;
};

export type FormsHostPlanInput = {
  plan: FormsHostPlan;
  binding: Binding;
  release: Release;
  directory: string;
  stable(): Promise<void>;
};

const hash = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const sha = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const safeId = (value: unknown): value is string => typeof value === "string" && /^[A-Za-z0-9_-]{1,180}$/.test(value);
const assert: (condition: unknown, code: string) => asserts condition = (condition, code) => { if (!condition) throw new Error(code); };
const exactKeys = (value: Record<string, unknown>, keys: readonly string[]) => JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...keys].sort());

function pathWithin(path: string, parent: string) { return path.startsWith(parent + "/"); }
async function boundFile(root: string, value: Binding, options: { privateFile?: boolean; chromium?: boolean; maxBytes?: number } = {}) {
  assert(record(value) && typeof value.path === "string" && sha(value.sha256), "FORMS_HOST_BINDING_REQUIRED");
  const path = resolve(root, value.path), chromiumCache = join(homedir(), "Library/Caches/ms-playwright") + "/";
  const chromiumAllowed = options.chromium === true && isAbsolute(value.path) && path.startsWith(chromiumCache) &&
    /^chromium(?:_headless_shell)?-\d+\//.test(path.slice(chromiumCache.length));
  assert((path === root || pathWithin(path, root) || chromiumAllowed) && await realpath(path) === path, "FORMS_HOST_REAL_INTERNAL_FILE_REQUIRED");
  const stat = await lstat(path);
  assert(stat.isFile() && !stat.isSymbolicLink() && (!options.privateFile || (stat.mode & 0o077) === 0 && stat.uid === (process.getuid?.() ?? stat.uid)) &&
    (options.maxBytes === undefined || stat.size <= options.maxBytes), "FORMS_HOST_PRIVATE_REGULAR_FILE_REQUIRED");
  const bytes = await readFile(path);
  assert(hash(bytes) === value.sha256, "FORMS_HOST_BOUND_BYTES_CHANGED");
  return { path, bytes };
}

async function readArm(root: string, pathInput: string) {
  const path = resolve(root, pathInput), scratch = join(root, ".scratch/dev");
  assert(pathWithin(path, scratch) && await realpath(path) === path, "PRIVATE_FORMS_HOST_ARM_REQUIRED");
  const stat = await lstat(path);
  assert(stat.isFile() && !stat.isSymbolicLink() && stat.size <= 64 * 1024 && (stat.mode & 0o077) === 0 && stat.uid === (process.getuid?.() ?? stat.uid), "PRIVATE_FORMS_HOST_ARM_REQUIRED");
  const bytes = await readFile(path), value: unknown = JSON.parse(bytes.toString("utf8"));
  assert(record(value) && exactKeys(value, ["schema", "version", "enabled", "expiresAt", "workflow", "runtimeFingerprint", "verificationFingerprint", "release", "releaseFiles", "tls", "tools"]), "EXACT_FORMS_HOST_ARM_SCHEMA_REQUIRED");
  assert(value.schema === "erpx-forms-host-arm/v1" && value.version === 1 && value.enabled === true && value.workflow === formsHostWorkflow &&
    Number.isSafeInteger(value.expiresAt) && Number(value.expiresAt) > Date.now() && Number(value.expiresAt) <= Date.now() + 12 * 3_600_000 &&
    sha(value.runtimeFingerprint) && sha(value.verificationFingerprint) && record(value.release) && exactKeys(value.release, ["path", "sha256"]) &&
    record(value.releaseFiles) && exactKeys(value.releaseFiles, ["path", "sha256"]) && record(value.tls) && exactKeys(value.tls, ["certificate", "key", "ca", "spki"]) &&
    record(value.tls.certificate) && exactKeys(value.tls.certificate, ["path", "sha256"]) && record(value.tls.key) && exactKeys(value.tls.key, ["path", "sha256"]) &&
    record(value.tls.ca) && exactKeys(value.tls.ca, ["path", "sha256"]) && sha(value.tls.spki) && record(value.tools) && exactKeys(value.tools, ["node", "chromium"]) &&
    record(value.tools.node) && exactKeys(value.tools.node, ["path", "sha256"]) && record(value.tools.chromium) && exactKeys(value.tools.chromium, ["path", "sha256"]),
    "EXACT_FORMS_HOST_ARM_REQUIRED");
  return { plan: value as unknown as FormsHostPlan, path, bytes, binding: { path, sha256: hash(bytes) } };
}

/** Validate a current private Forms-only arm and the exact isolated release it names. */
export async function formsHostPlan(rootInput: string, armPath: string, runtimeFingerprint: string, verificationFingerprint: string, releaseDirectory: string): Promise<FormsHostPlanInput> {
  const root = resolve(rootInput);
  assert(await realpath(root) === root && sha(runtimeFingerprint) && sha(verificationFingerprint), "FORMS_HOST_CURRENT_FINGERPRINTS_REQUIRED");
  const arm = await readArm(root, armPath), plan = arm.plan;
  assert(plan.runtimeFingerprint === runtimeFingerprint && plan.verificationFingerprint === verificationFingerprint, "CURRENT_FORMS_HOST_ARM_REQUIRED");
  const directory = await realpath(releaseDirectory), releaseRoot = join(root, ".runtime/releases");
  assert(pathWithin(directory, releaseRoot) && resolve(root, plan.release.path) === join(directory, "release.json") &&
    resolve(root, plan.releaseFiles.path) === join(directory, "release-files.json"), "EXACT_FORMS_HOST_COMPILED_RELEASE_REQUIRED");
  const releaseFile = await boundFile(root, plan.release), manifestFile = await boundFile(root, plan.releaseFiles);
  const releaseValue: unknown = JSON.parse(releaseFile.bytes.toString("utf8"));
  assert(record(releaseValue) && safeId(releaseValue.id) && releaseValue.sourceFingerprint === runtimeFingerprint && releaseValue.node === "26.11.1" &&
    sha(releaseValue.fileManifestSha256) && hash(manifestFile.bytes) === releaseValue.fileManifestSha256, "FORMS_HOST_RELEASE_CANDIDATE_MISMATCH");
  const manifest: unknown = JSON.parse(manifestFile.bytes.toString("utf8"));
  assert(record(manifest) && Object.keys(manifest).length > 0, "FORMS_HOST_RELEASE_FILES_REQUIRED");
  for (const [name, digest] of Object.entries(manifest)) {
    assert(name && !isAbsolute(name) && !name.includes("\\") && name.split("/").every(part => part && part !== "." && part !== "..") && sha(digest), "FORMS_HOST_RELEASE_ARTIFACT_PATH_REQUIRED");
    const path = resolve(directory, name), actual = await realpath(path);
    assert(pathWithin(actual, directory) && (await lstat(actual)).isFile() && hash(await readFile(actual)) === digest, "FORMS_HOST_RELEASE_ARTIFACT_CHANGED");
  }
  assert(resolve(root, plan.tools.node.path) === join(root, ".runtime/node/node-v26.11.1-darwin-arm64/bin/node"), "PINNED_FORMS_HOST_NODE_REQUIRED");
  const { chromium } = await import("playwright");
  assert(resolve(root, plan.tools.chromium.path) === chromium.executablePath(), "PINNED_FORMS_HOST_CHROMIUM_REQUIRED");
  for (const value of [plan.tls.certificate, plan.tls.ca]) await boundFile(root, value);
  await boundFile(root, plan.tls.key, { privateFile: true });
  await boundFile(root, plan.tools.node);
  await boundFile(root, plan.tools.chromium, { chromium: true });
  const release: Release = { id: releaseValue.id, sourceFingerprint: runtimeFingerprint, node: "26.11.1", fileManifestSha256: releaseValue.fileManifestSha256 };
  return {
    plan,
    binding: arm.binding,
    release,
    directory,
    async stable() {
      const current = await readFile(arm.path);
      assert(current.equals(arm.bytes) && plan.expiresAt > Date.now(), "FORMS_HOST_ARM_CHANGED_OR_EXPIRED");
      for (const value of [plan.release, plan.releaseFiles, plan.tls.certificate, plan.tls.key, plan.tls.ca, plan.tools.node, plan.tools.chromium])
        await boundFile(root, value, { privateFile: value === plan.tls.key, chromium: value === plan.tools.chromium });
    },
  };
}

function fixtureShape(value: unknown, run: string): FormsHostFixture {
  assert(record(value) && value.run === run && record(value.marketing), "FORMS_HOST_WORKPLACE_GUIDE_REQUIRED");
  const marketing = value.marketing;
  assert(safeId(marketing.formId) && Number.isSafeInteger(marketing.formVersion) && Number(marketing.formVersion) > 0 &&
    (marketing.publicationId === undefined || marketing.publicationId === null || safeId(marketing.publicationId)), "FORMS_HOST_SEEDED_PUBLISHED_FORM_REQUIRED");
  return { slug: "simulation", formId: marketing.formId, formVersion: Number(marketing.formVersion),
    ...(typeof marketing.publicationId === "string" ? { mediaPublicationId: marketing.publicationId } : {}) };
}

function safeStaticPath(path: string, search: string) {
  const exactAssets = new Set([
    "/fonts/NotoSansThai.woff2", "/fonts/NotoSansThaiOther.woff2", "/favicon.ico", "/icon.svg",
    "/icon-192.png", "/icon-512.png", "/icon-maskable-512.png",
  ]);
  if (exactAssets.has(path)) return search === "";
  const segments = path.split("/");
  return path.startsWith("/_next/static/") && !/%(?:2e|2f|5c)/i.test(path) && segments.length >= 5 && segments[0] === "" && segments.slice(1).every(part => part && part !== "." && part !== "..") &&
    /^[A-Za-z0-9%._~!$&'()*+,;=:@/-]+$/.test(path) && search.length <= 512 && (search === "" || search.startsWith("?")) && /^[?A-Za-z0-9%._~!$&'()*+,;=:@/-]*$/.test(search);
}

function bodyShape(bodyText: string, fixture: FormsHostFixture, run: string) {
  let body: unknown;
  try { body = JSON.parse(bodyText); } catch { return false; }
  if (!record(body) || !exactKeys(body, ["formVersion", "contactName", "contactDetail", "brief", "consentAccepted", "source"]) ||
      bodyText !== JSON.stringify(body)) return false;
  return body.formVersion === fixture.formVersion && body.consentAccepted === true && body.contactName === `Fictional Forms visitor ${run}` &&
    body.contactDetail === `forms-visitor-${run}@simulation.test` && body.brief === `Fictional Forms-host inquiry for isolated run ${run}; human review only.` &&
    body.source === `Declared fictional Forms visitor ${run}`;
}

/** Return the only request classes the Forms edge can forward. */
export function formsHostRequestKind(scope: string | null, input: FormsHostPolicyInput): FormsHostRequestKind | null {
  if (scope !== formsHostWorkflow || input.origin !== formsHostOrigin || input.headers.host !== formsHostname ||
      !/^[a-f0-9]{12}$/.test(input.run) || input.fixture.slug !== "simulation" || !safeId(input.fixture.formId) ||
      input.fixture.mediaPublicationId !== undefined && !safeId(input.fixture.mediaPublicationId) ||
      !Number.isSafeInteger(input.fixture.formVersion) || input.fixture.formVersion < 1 || input.headers.cookie !== undefined ||
      input.headers.authorization !== undefined || input.headers["proxy-authorization"] !== undefined ||
      Object.keys(input.headers).some(name => name === "forwarded" || name.startsWith("x-forwarded-"))) return null;
  const search = input.search ?? "";
  if (input.headers.origin !== undefined && input.headers.origin !== formsHostOrigin) return null;
  const sourceSearch = `?utm_source=${encodeURIComponent(`Declared fictional Forms visitor ${input.run}`)}`;
  if (input.method === "GET" && input.body === "" && (input.headers["content-length"] === undefined || input.headers["content-length"] === "0") &&
      input.headers["transfer-encoding"] === undefined) {
    if (input.path === `/forms/simulation/${input.fixture.formId}` && search === sourceSearch) return "forms-page";
    if (search === "" && input.path === `/api/public/marketing/simulation/${input.fixture.formId}`) return "public-form-read";
    if (search === "" && input.fixture.mediaPublicationId && input.path === `/api/public/holdings/simulation/media-publications/${input.fixture.mediaPublicationId}/content`) return "approved-media-read";
    if (search === "" && (input.path === "/icon-192.png" || input.path === "/icon-512.png" || input.path === "/icon-maskable-512.png")) return "static-read";
  }
  if (input.method === "GET" && input.body === "" &&
      (input.headers["content-length"] === undefined || input.headers["content-length"] === "0") &&
      input.headers["transfer-encoding"] === undefined && safeStaticPath(input.path, search)) return "static-read";
  if (input.method !== "POST" || input.path !== `/api/public/marketing/simulation/${input.fixture.formId}` || search !== "" || input.body.length > maxBodyBytes ||
      Buffer.byteLength(input.body) > maxBodyBytes || input.headers["content-type"]?.toLowerCase() !== "application/json" ||
      input.headers["sec-fetch-site"] !== "same-origin" || input.headers["sec-fetch-mode"] !== "cors" || input.headers["sec-fetch-dest"] !== "empty" ||
      input.headers.origin !== formsHostOrigin || input.headers["expect"] !== undefined || input.headers["content-encoding"] !== undefined ||
      input.headers["transfer-encoding"] !== undefined || input.headers["content-length"] !== undefined &&
        (!/^\d+$/.test(input.headers["content-length"]) || Number(input.headers["content-length"]) !== Buffer.byteLength(input.body)) ||
      !/^[A-Za-z0-9:_.-]{8,180}$/.test(input.headers["idempotency-key"] ?? "") || !bodyShape(input.body, input.fixture, input.run)) return null;
  const key = input.headers["idempotency-key"]!;
  if (input.prior.acceptedPosts === 0) return "submission";
  if (input.prior.acceptedPosts === 1 && input.prior.firstBody === input.body && input.prior.firstIdempotencyKey === key) return "replay";
  return null;
}

export function permitsFormsHostRequest(scope: string | null, input: FormsHostPolicyInput): boolean {
  return formsHostRequestKind(scope, input) !== null;
}

export function formsHostRedirectAllowed(location: string, fixture: FormsHostFixture, run: string) {
  try {
    const target = new URL(location, formsHostOrigin);
    if (target.origin !== formsHostOrigin || target.username || target.password || target.hash || !/^[a-f0-9]{12}$/.test(run)) return false;
    return ["forms-page", "public-form-read", "approved-media-read", "static-read"].includes(formsHostRequestKind(formsHostWorkflow, {
      method: "GET", origin: target.origin, path: target.pathname, search: target.search,
      headers: { host: formsHostname }, body: "", fixture, run, prior: { acceptedPosts: 0 },
    }) ?? "");
  } catch { return false; }
}

async function readPrivateJson(path: string) {
  const stat = await lstat(path);
  assert(await realpath(path) === path && stat.isFile() && !stat.isSymbolicLink() && (stat.mode & 0o077) === 0 && stat.uid === (process.getuid?.() ?? stat.uid), "FORMS_HOST_PRIVATE_FIXTURE_REQUIRED");
  return JSON.parse(await readFile(path, "utf8")) as unknown;
}

function validateDatabaseManifest(value: unknown, run: string) {
  assert(record(value) && record(value.databases) && JSON.stringify(Object.keys(value.databases).sort()) === JSON.stringify([...databaseNames].sort()), "EXACT_FORMS_HOST_EIGHT_DATABASES_REQUIRED");
  for (const name of databaseNames) {
    const raw = value.databases[name];
    assert(typeof raw === "string", "EXACT_FORMS_HOST_DATABASE_REQUIRED");
    const url = new URL(raw);
    assert(["postgres:", "postgresql:"].includes(url.protocol) && url.hostname === "127.0.0.1" && url.port === "5432" &&
      url.username === "erpx_test_admin" && url.pathname === `/erpx_browser_${run}_${name.toLowerCase()}` && !url.password && !url.search && !url.hash,
      "EXACT_FORMS_HOST_DATABASE_IDENTITY_REQUIRED");
  }
}

async function readDatabaseManifest(root: string, run: string) {
  const path = join(root, ".runtime/browser", run, "databases.json"), value: unknown = await readPrivateJson(path), bytes = await readFile(path);
  validateDatabaseManifest(value, run);
  return { path, sha256: hash(bytes), bytes: bytes.length };
}

async function requestBody(request: IncomingMessage) {
  const rawLength = request.headers["content-length"];
  const length = rawLength === undefined ? 0 : Number(rawLength);
  assert((rawLength === undefined || /^\d+$/.test(rawLength)) && Number.isSafeInteger(length) && length <= maxBodyBytes, "FORMS_HOST_BODY_TOO_LARGE");
  const chunks: Buffer[] = []; let bytes = 0;
  for await (const part of request) {
    const chunk = Buffer.isBuffer(part) ? part : Buffer.from(part); bytes += chunk.length;
    assert(bytes <= maxBodyBytes, "FORMS_HOST_BODY_TOO_LARGE"); chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function freePort() {
  const server = createNetServer();
  await new Promise<void>((yes, no) => { server.once("error", no); server.listen(3102, "127.0.0.1", () => server.close(error => error ? no(error) : yes())); });
}

function stateRelease(value: unknown): value is { id: string; fileManifestSha256: string } {
  return record(value) && typeof value.id === "string" && sha(value.fileManifestSha256);
}

function historicalArmShape(value: unknown): value is FormsHostPlan {
  return record(value) && exactKeys(value, ["schema", "version", "enabled", "expiresAt", "workflow", "runtimeFingerprint", "verificationFingerprint", "release", "releaseFiles", "tls", "tools"]) &&
    value.schema === "erpx-forms-host-arm/v1" && value.version === 1 && value.enabled === true &&
    Number.isSafeInteger(value.expiresAt) && Number(value.expiresAt) > 0 && typeof value.workflow === "string" &&
    sha(value.runtimeFingerprint) && sha(value.verificationFingerprint) && record(value.release) && exactKeys(value.release, ["path", "sha256"]) &&
    typeof value.release.path === "string" && sha(value.release.sha256) && record(value.releaseFiles) && exactKeys(value.releaseFiles, ["path", "sha256"]) &&
    typeof value.releaseFiles.path === "string" && sha(value.releaseFiles.sha256) && record(value.tls) && exactKeys(value.tls, ["certificate", "key", "ca", "spki"]) &&
    record(value.tls.certificate) && exactKeys(value.tls.certificate, ["path", "sha256"]) && typeof value.tls.certificate.path === "string" && sha(value.tls.certificate.sha256) &&
    record(value.tls.key) && exactKeys(value.tls.key, ["path", "sha256"]) && typeof value.tls.key.path === "string" && sha(value.tls.key.sha256) &&
    record(value.tls.ca) && exactKeys(value.tls.ca, ["path", "sha256"]) && typeof value.tls.ca.path === "string" && sha(value.tls.ca.sha256) &&
    sha(value.tls.spki) && record(value.tools) && exactKeys(value.tools, ["node", "chromium"]) &&
    record(value.tools.node) && exactKeys(value.tools.node, ["path", "sha256"]) && typeof value.tools.node.path === "string" && sha(value.tools.node.sha256) &&
    record(value.tools.chromium) && exactKeys(value.tools.chromium, ["path", "sha256"]) && typeof value.tools.chromium.path === "string" && sha(value.tools.chromium.sha256);
}

/** Check retained arm, release and TLS metadata at the original browser observation time. */
export function assertFormsHostHistoricalBindings(input: {
  root: string;
  lease: Pick<FormsHostLease, "sourceFingerprint" | "verificationFingerprint" | "release" | "workflow" | "certificate" | "key" | "ca" | "spki" | "ui">;
  arm: unknown;
  release: unknown;
  releaseManifest: Uint8Array;
  tls: FormsHostHistoricalTlsEvidence;
}) {
  const { root, lease, arm, release, releaseManifest, tls } = input;
  assert(historicalArmShape(arm), "EXACT_FORMS_HOST_HISTORICAL_ARM_REQUIRED");
  const observedAt = Date.parse(lease.ui.observedAt);
  assert(Number.isFinite(observedAt) && arm.workflow === lease.workflow && arm.runtimeFingerprint === lease.sourceFingerprint &&
    arm.verificationFingerprint === lease.verificationFingerprint && arm.expiresAt > observedAt && arm.expiresAt <= observedAt + 12 * 3_600_000,
    "FORMS_HOST_HISTORICAL_ARM_IDENTITY_REQUIRED");
  const sameBinding = (left: Binding, right: Binding) => left.path === right.path && left.sha256 === right.sha256;
  assert(sameBinding(arm.tls.certificate, lease.certificate) && sameBinding(arm.tls.key, lease.key) && sameBinding(arm.tls.ca, lease.ca) && arm.tls.spki === lease.spki,
    "FORMS_HOST_HISTORICAL_TLS_BINDING_REQUIRED");
  const releasePath = resolve(root, arm.release.path), releaseFilesPath = resolve(root, arm.releaseFiles.path);
  assert(pathWithin(dirname(releasePath), join(root, ".runtime/releases")) && releasePath === join(dirname(releasePath), "release.json") &&
    releaseFilesPath === join(dirname(releasePath), "release-files.json"), "FORMS_HOST_HISTORICAL_RELEASE_PATH_REQUIRED");
  assert(record(release) && safeId(release.id) && release.node === "26.11.1" && release.sourceFingerprint === lease.sourceFingerprint &&
    sha(release.fileManifestSha256) && release.fileManifestSha256 === arm.releaseFiles.sha256 && hash(releaseManifest) === release.fileManifestSha256 &&
    release.id === lease.release.id && release.fileManifestSha256 === lease.release.fileManifestSha256 &&
    dirname(releasePath) === join(root, ".runtime/releases", release.id),
    "FORMS_HOST_HISTORICAL_RELEASE_BINDING_REQUIRED");
  assert(tls.caIsAuthority && tls.certificateIssuerMatchesCa && tls.certificateSignatureValid && tls.certificateHostMatches &&
    Number.isFinite(tls.certificateValidFrom) && Number.isFinite(tls.certificateValidTo) && tls.certificateValidFrom <= observedAt && observedAt < tls.certificateValidTo &&
    tls.certificateSpki === lease.spki && tls.privateKeySpki === lease.spki,
    "FORMS_HOST_HISTORICAL_TLS_IDENTITY_REQUIRED");
}

function processCommand(pid: number) {
  try { return execFileSync("ps", ["-p", String(pid), "-o", "command="], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim(); }
  catch { return ""; }
}

function processWorkingDirectory(pid: number) {
  try {
    const value = execFileSync("lsof", ["-a", "-p", String(pid), "-d", "cwd", "-Fn"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    return value.split(/\r?\n/).find(line => line.startsWith("n"))?.slice(1) ?? "";
  } catch { return ""; }
}

async function listenFormsHostEdge(root: string, input: FormsHostPlanInput, run: string, output: string) {
  assert(/^[a-f0-9]{12}$/.test(run) && resolve(output).startsWith(join(root, ".scratch/dev/test-suites/browser/")) && resolve(output).endsWith("/" + run), "EXACT_FORMS_HOST_CASE_OUTPUT_REQUIRED");
  const outputStat = await lstat(output);
  assert(outputStat.isDirectory() && !outputStat.isSymbolicLink() && (outputStat.mode & 0o077) === 0 && outputStat.uid === (process.getuid?.() ?? outputStat.uid), "PRIVATE_FORMS_HOST_CASE_OUTPUT_REQUIRED");
  const cohort = join(root, ".runtime/browser", run), statePath = join(cohort, "state.json");
  await readPrivateJson(statePath);
  const stateBytes = await readFile(statePath), state: unknown = JSON.parse(stateBytes.toString("utf8"));
  assert(record(state) && state.run === run && state.prefix === `erpx_browser_${run}` && state.profile === "engineering" && state.port === 3104 &&
    state.origin === "http://127.0.0.1:3104" && state.ready === true && state.diagnostic !== true && state.formsHosts === true &&
    state.productHosts === false && state.executiveWorkHosts === false && state.sourceFingerprint === input.plan.runtimeFingerprint && stateRelease(state.release) &&
    state.release.id === input.release.id && state.release.fileManifestSha256 === input.release.fileManifestSha256 &&
    Number.isSafeInteger(state.pid) && Number(state.pid) > 0 && Number.isSafeInteger(state.jobsPid) && Number(state.jobsPid) > 0 &&
    state.pid !== state.jobsPid &&
    state.webEntrypoint === join(cohort, "release/server.js") && state.jobsEntrypoint === join(cohort, "release/tools/jobs.mjs"), "CURRENT_OWNED_FORMS_HOST_COHORT_REQUIRED");
  process.kill(Number(state.pid), 0);
  process.kill(Number(state.jobsPid), 0);
  const releaseWorkingDirectory = join(cohort, "release"), webEntrypoint = join(releaseWorkingDirectory, "server.js"), jobsEntrypoint = join(releaseWorkingDirectory, "tools/jobs.mjs");
  for (const [path, candidatePath] of [[webEntrypoint, join(input.directory, "server.js")], [jobsEntrypoint, join(input.directory, "tools/jobs.mjs")]]) {
    const stat = await lstat(path);
    assert(await realpath(path) === path && stat.isFile() && !stat.isSymbolicLink() && hash(await readFile(path)) === hash(await readFile(candidatePath)), "FORMS_HOST_COHORT_RELEASE_ENTRYPOINT_CHANGED");
  }
  assert(processCommand(Number(state.pid)).includes(webEntrypoint) && processWorkingDirectory(Number(state.pid)) === releaseWorkingDirectory &&
    processCommand(Number(state.jobsPid)).includes(jobsEntrypoint) && processWorkingDirectory(Number(state.jobsPid)) === releaseWorkingDirectory, "FORMS_HOST_LIVE_RELEASE_PROCESSES_REQUIRED");
  const sourceDatabaseBinding = await readDatabaseManifest(root, run), guidePath = join(cohort, "workplace-platform-guide.json");
  await readPrivateJson(guidePath);
  const guideBytes = await readFile(guidePath), fixture = fixtureShape(JSON.parse(guideBytes.toString("utf8")), run);
  const databaseBytes = await readFile(sourceDatabaseBinding.path), fixtureGuideBytes = guideBytes;
  const databasePath = join(output, "forms-host-databases.json"), fixtureGuidePath = join(output, "forms-host-workplace-platform-guide.json");
  await writeFile(databasePath, databaseBytes, { mode: 0o600, flag: "wx" }); await chmod(databasePath, 0o600);
  await writeFile(fixtureGuidePath, fixtureGuideBytes, { mode: 0o600, flag: "wx" }); await chmod(fixtureGuidePath, 0o600);
  const databaseBinding = { path: databasePath, sha256: hash(databaseBytes), bytes: databaseBytes.length };
  const guideBinding = { path: fixtureGuidePath, sha256: hash(fixtureGuideBytes), bytes: fixtureGuideBytes.length };
  const sourcePath = join(root, "scripts/testing/forms-host-cohort.ts"), sourceBinding = { path: sourcePath, sha256: hash(await readFile(sourcePath)) };
  await freePort();
  const certFile = await boundFile(root, input.plan.tls.certificate), keyFile = await boundFile(root, input.plan.tls.key, { privateFile: true }), caFile = await boundFile(root, input.plan.tls.ca);
  const cert = new X509Certificate(certFile.bytes), ca = new X509Certificate(caFile.bytes), spki = hash(cert.publicKey.export({ format: "der", type: "spki" }));
  assert(ca.ca && cert.verify(ca.publicKey) && Date.parse(cert.validFrom) <= Date.now() && Date.parse(cert.validTo) > Date.now() &&
    cert.subjectAltName === `DNS:${formsHostname}` && cert.checkHost(formsHostname) === formsHostname && spki === input.plan.tls.spki &&
    hash(createPublicKey(createPrivateKey(keyFile.bytes)).export({ format: "der", type: "spki" })) === spki,
    "EXACT_FORMS_HOST_SINGLE_ORIGIN_TLS_REQUIRED");

  const sockets = new Set<Socket>(), refusals: { method: string; pathSha256: string }[] = [], requestState: FormsHostRequestState = { acceptedPosts: 0 };
  let closed = false, leaseReady = false, writes = Promise.resolve();
  const requestEvidence = (): FormsHostLease["requestEvidence"] => ({
    acceptedPosts: requestState.acceptedPosts as 0 | 1 | 2,
    replayCount: Math.max(0, requestState.acceptedPosts - 1) as 0 | 1,
    ...(requestState.firstBody ? { bodySha256: hash(requestState.firstBody) } : {}),
    ...(requestState.firstIdempotencyKey ? { idempotencyKeySha256: hash(requestState.firstIdempotencyKey) } : {}),
    refusalCount: refusals.length, refusalLog: refusals.slice(0, maxRefusals),
  });
  const leasePath = join(output, "forms-host-lease.json"), tempPath = () => `${leasePath}.${process.pid}.${Date.now()}.${Math.random().toString(16).slice(2)}.tmp`;
  const lease = {} as FormsHostLease;
  async function persistLease() {
    if (!leaseReady) return;
    lease.requestEvidence = requestEvidence();
    const bytes = JSON.stringify(lease, null, 2) + "\n";
    writes = writes.then(async () => {
      const temp = tempPath(); await writeFile(temp, bytes, { mode: 0o600, flag: "wx" });
      await chmod(temp, 0o600); await rename(temp, leasePath);
    });
    await writes;
  }
  function noteRefusal(method: string, value: string) {
    if (refusals.length < maxRefusals) refusals.push({ method: /^[A-Z]{1,12}$/.test(method) ? method : "INVALID", pathSha256: hash(value) });
  }
  const reject = async (req: IncomingMessage, res: import("node:http").ServerResponse, value = req.url ?? "") => {
    noteRefusal(String(req.method ?? "INVALID").toUpperCase(), value); req.resume(); await persistLease();
    res.writeHead(403, { "Cache-Control": "no-store" }); res.end("Forms host edge refused this request.");
  };
  const tls = createHttpsServer({ key: keyFile.bytes, cert: certFile.bytes, minVersion: "TLSv1.3" }, (req, res) => {
    void (async () => {
      let url: URL;
      try {
        assert(req.socket.remoteAddress === "127.0.0.1" && (req.socket as import("node:tls").TLSSocket).servername === formsHostname, "FORMS_HOST_LOCAL_TLS_CLIENT_REQUIRED");
        assert(String(req.headers.host ?? "") === formsHostname, "FORMS_HOST_EXACT_HOST_HEADER_REQUIRED");
        assert(!Object.keys(req.headers).some(name => name === "forwarded" || name.startsWith("x-forwarded-")), "FORMS_HOST_CALLER_FORWARDING_REFUSED");
        assert(typeof req.url === "string" && req.url.startsWith("/") && !req.url.startsWith("//"), "FORMS_HOST_ORIGIN_FORM_REQUIRED");
        url = new URL(req.url, formsHostOrigin); assert(url.origin === formsHostOrigin && !url.username && !url.password && !url.hash, "FORMS_HOST_TARGET_REFUSED");
      } catch { await reject(req, res); return; }
      const method = String(req.method ?? "").toUpperCase(), body = method === "POST" ? await requestBody(req).catch(async () => { await reject(req, res); return null; }) : "";
      if (body === null) return;
      const headers = Object.fromEntries(Object.entries(req.headers).map(([name, value]) => [name.toLowerCase(), Array.isArray(value) ? value.join(",") : value]));
      const kind = formsHostRequestKind(formsHostWorkflow, { method, origin: url.origin, path: url.pathname, search: url.search, headers, body, fixture, run, prior: requestState });
      if (!kind) { await reject(req, res); return; }
      if (kind === "submission" || kind === "replay") {
        if (kind === "submission") { requestState.acceptedPosts = 1; requestState.firstBody = body; requestState.firstIdempotencyKey = String(headers["idempotency-key"]); }
        else requestState.acceptedPosts = 2;
        await persistLease();
      }
      const upstreamHeaders: IncomingHttpHeaders = { ...req.headers, host: formsHostname, "x-forwarded-host": formsHostname, "x-forwarded-proto": "https", "x-forwarded-port": "443" };
      delete upstreamHeaders["proxy-authorization"];
      const upstream = httpRequest({ hostname: "127.0.0.1", port: 3104, path: url.pathname + url.search, method, headers: upstreamHeaders, agent: false }, reply => {
        const locations = Array.isArray(reply.headers.location) ? reply.headers.location : reply.headers.location ? [reply.headers.location] : [];
        if (reply.headers.refresh !== undefined || locations.some(location => !formsHostRedirectAllowed(location, fixture, run))) {
          noteRefusal("REDIRECT", [...locations, String(reply.headers.refresh ?? "")].join("\n")); void persistLease().then(() => { reply.destroy(); if (!res.headersSent) res.writeHead(403, { "Cache-Control": "no-store" }); res.end("Forms host redirect refused."); }); return;
        }
        delete reply.headers["set-cookie"]; delete reply.headers["set-cookie2"];
        res.writeHead(reply.statusCode ?? 502, reply.headers); reply.pipe(res);
      });
      upstream.on("error", () => { if (!res.headersSent) res.writeHead(502, { "Cache-Control": "no-store" }); res.end(); });
      if (body) upstream.end(body); else req.pipe(upstream);
    })().catch(async () => { if (!res.headersSent) await reject(req, res); else res.destroy(); });
  });
  tls.on("upgrade", (req, socket) => {
    noteRefusal("UPGRADE", req.url ?? ""); void persistLease().finally(() => socket.end("HTTP/1.1 403 Forbidden\r\n\r\n"));
  });
  const proxy = createHttpServer((req, res) => {
    noteRefusal("HTTP", req.url ?? ""); req.resume();
    void persistLease().finally(() => { res.writeHead(403, { "Cache-Control": "no-store" }); res.end(); });
  });
  proxy.on("upgrade", (req, socket) => {
    noteRefusal("UPGRADE", req.url ?? ""); void persistLease().finally(() => socket.end("HTTP/1.1 403 Forbidden\r\n\r\n"));
  });
  proxy.on("connection", socket => { sockets.add(socket); socket.once("close", () => sockets.delete(socket)); });
  proxy.on("connect", (req, socket, head) => {
    const match = /^([a-z0-9.-]+):443$/.exec(req.url ?? "");
    if (!(socket instanceof Socket) || match?.[1] !== formsHostname || socket.remoteAddress !== "127.0.0.1") {
      noteRefusal("CONNECT", req.url ?? ""); void persistLease().finally(() => socket.end("HTTP/1.1 403 Forbidden\r\n\r\n")); return;
    }
    socket.write("HTTP/1.1 200 Connection Established\r\n\r\n"); if (head.length) socket.unshift(head); tls.emit("connection", socket);
  });
  await new Promise<void>((yes, no) => { proxy.once("error", no); proxy.listen(3102, "127.0.0.1", () => yes()); });
  const address = proxy.address(); assert(address && typeof address === "object" && address.address === "127.0.0.1" && address.port === 3102, "FORMS_HOST_ACTUAL_LISTENER_REQUIRED");
  assert((await readFile(statePath)).equals(stateBytes), "FORMS_HOST_ORIGINAL_STATE_CHANGED");
  Object.assign(lease, {
    schema, run, pid: process.pid, port: 3102, upstream: "http://127.0.0.1:3104", sourceFingerprint: input.plan.runtimeFingerprint,
    verificationFingerprint: input.plan.verificationFingerprint, release: { id: input.release.id, fileManifestSha256: input.release.fileManifestSha256 },
    workflow: formsHostWorkflow, origin: formsHostOrigin, formsOnly: true, productHosts: false, executiveWorkHosts: false,
    commercial: false, mfa: false, apply: false, packageEnforcement: false, wholeProductAcceptance: false, fixture,
    plan: input.binding, certificate: { path: certFile.path, sha256: hash(certFile.bytes) }, key: { path: keyFile.path, sha256: hash(keyFile.bytes) }, certificateSha256: hash(certFile.bytes),
    ca: { path: caFile.path, sha256: hash(caFile.bytes) }, chromium: { path: resolve(root, input.plan.tools.chromium.path), sha256: input.plan.tools.chromium.sha256 }, spki,
    databases: databaseBinding, fixtureGuide: guideBinding,
    requestEvidence: requestEvidence(),
    ui: {
      version: 1, observedAt: new Date().toISOString(), listener: { address: address.address, port: address.port, pid: process.pid },
      node: { path: process.execPath, sha256: input.plan.tools.node.sha256 }, source: sourceBinding,
      backend: { pid: Number(state.pid), jobsPid: Number(state.jobsPid), webEntrypoint: String(state.webEntrypoint), jobsEntrypoint: String(state.jobsEntrypoint),
        state: { path: statePath, sha256: hash(stateBytes), bytes: stateBytes.length } },
    },
  });
  await writeFile(leasePath, JSON.stringify(lease, null, 2) + "\n", { mode: 0o600, flag: "wx" }); await chmod(leasePath, 0o600);
  leaseReady = true;
  return {
    path: leasePath, lease,
    async stable() {
      assert(!closed && proxy.listening, "FORMS_HOST_EDGE_CLOSED"); await input.stable(); await persistLease();
      assert((await readFile(statePath)).equals(stateBytes), "FORMS_HOST_ORIGINAL_STATE_CHANGED");
      assert((await readFile(guidePath)).equals(guideBytes), "FORMS_HOST_ORIGINAL_GUIDE_CHANGED");
      const currentDatabases = await readDatabaseManifest(root, run);
      assert(currentDatabases.path === sourceDatabaseBinding.path && currentDatabases.sha256 === sourceDatabaseBinding.sha256 && currentDatabases.bytes === sourceDatabaseBinding.bytes &&
        (await boundFile(root, databaseBinding, { privateFile: true })).bytes.equals(databaseBytes) &&
        (await boundFile(root, guideBinding, { privateFile: true })).bytes.equals(fixtureGuideBytes), "FORMS_HOST_DATABASE_OR_FIXTURE_BINDING_CHANGED");
      assert(refusals.length === 0, "FORMS_HOST_REQUEST_REFUSED");
      assert(requestState.acceptedPosts === 2 && requestState.firstBody !== undefined && requestState.firstIdempotencyKey !== undefined, "FORMS_HOST_SUBMISSION_AND_EXACT_REPLAY_REQUIRED");
      assert((await readFile(leasePath, "utf8")) === JSON.stringify(lease, null, 2) + "\n", "FORMS_HOST_LEASE_CHANGED");
      await boundFile(root, sourceBinding);
    },
    async close() {
      if (closed) return;
      closed = true;
      for (const socket of sockets) socket.destroy();
      await new Promise<void>((yes, no) => proxy.close(error => error ? no(error) : yes()));
      await writes;
      assert(sockets.size === 0, "FORMS_HOST_SOCKETS_REMAIN");
    },
  };
}

/** Validate the retained exact-case Forms lease. This does not probe live state after teardown. */
export async function readFormsHostLease(rootInput: string, pathInput: string, options: { requireCompleted?: boolean } = { requireCompleted: true }): Promise<FormsHostLease> {
  const requireCompleted = options.requireCompleted ?? true;
  const root = resolve(rootInput), path = resolve(pathInput), outputRoot = join(root, ".scratch/dev/test-suites/browser");
  assert(isAbsolute(pathInput) && pathWithin(path, outputRoot) && await realpath(path) === path && path.endsWith("/forms-host-lease.json"), "FORMS_HOST_LEASE_REQUIRED");
  const stat = await lstat(path); assert(stat.isFile() && !stat.isSymbolicLink() && (stat.mode & 0o077) === 0 && stat.uid === (process.getuid?.() ?? stat.uid), "PRIVATE_FORMS_HOST_LEASE_REQUIRED");
  const value: unknown = JSON.parse(await readFile(path, "utf8"));
  assert(record(value) && exactKeys(value, ["schema", "run", "pid", "port", "upstream", "sourceFingerprint", "verificationFingerprint", "release", "workflow", "origin", "formsOnly", "productHosts", "executiveWorkHosts", "commercial", "mfa", "apply", "packageEnforcement", "wholeProductAcceptance", "fixture", "plan", "certificate", "key", "certificateSha256", "ca", "chromium", "spki", "databases", "fixtureGuide", "requestEvidence", "ui"]), "EXACT_FORMS_HOST_LEASE_SCHEMA_REQUIRED");
  assert(value.schema === schema && /^[a-f0-9]{12}$/.test(String(value.run)) && dirname(path).endsWith("/" + value.run) && Number.isSafeInteger(value.pid) && Number(value.pid) > 0 &&
    value.port === 3102 && value.upstream === "http://127.0.0.1:3104" && value.workflow === formsHostWorkflow && value.origin === formsHostOrigin &&
    value.formsOnly === true && value.productHosts === false && value.executiveWorkHosts === false && value.commercial === false && value.mfa === false && value.apply === false &&
    value.packageEnforcement === false && value.wholeProductAcceptance === false && sha(value.sourceFingerprint) && sha(value.verificationFingerprint) &&
    stateRelease(value.release) && record(value.fixture) && value.fixture.slug === "simulation" && safeId(value.fixture.formId) &&
    Number.isSafeInteger(value.fixture.formVersion) && Number(value.fixture.formVersion) > 0 &&
    (value.fixture.mediaPublicationId === undefined || safeId(value.fixture.mediaPublicationId)) && record(value.plan) &&
    record(value.certificate) && typeof value.certificate.path === "string" && sha(value.certificate.sha256) &&
    record(value.key) && typeof value.key.path === "string" && sha(value.key.sha256) && value.certificateSha256 === value.certificate.sha256 &&
    record(value.ca) && typeof value.ca.path === "string" && sha(value.ca.sha256) && record(value.chromium) && typeof value.chromium.path === "string" && sha(value.chromium.sha256) &&
    record(value.databases) && typeof value.databases.path === "string" && sha(value.databases.sha256) && Number.isSafeInteger(value.databases.bytes) &&
    record(value.fixtureGuide) && typeof value.fixtureGuide.path === "string" && sha(value.fixtureGuide.sha256) && Number.isSafeInteger(value.fixtureGuide.bytes) &&
    sha(value.spki) && record(value.requestEvidence) &&
    exactKeys(value.requestEvidence, ["acceptedPosts", "replayCount", "refusalCount", "refusalLog", ...(Object.hasOwn(value.requestEvidence, "bodySha256") ? ["bodySha256"] : []), ...(Object.hasOwn(value.requestEvidence, "idempotencyKeySha256") ? ["idempotencyKeySha256"] : [])]) &&
    [0, 1, 2].includes(Number(value.requestEvidence.acceptedPosts)) && [0, 1].includes(Number(value.requestEvidence.replayCount)) &&
    Number.isSafeInteger(value.requestEvidence.refusalCount) && Number(value.requestEvidence.refusalCount) >= 0 && Array.isArray(value.requestEvidence.refusalLog) &&
    value.requestEvidence.refusalLog.every(item => record(item) && exactKeys(item, ["method", "pathSha256"]) && typeof item.method === "string" && sha(item.pathSha256)) && record(value.ui), "EXACT_FORMS_HOST_LEASE_REQUIRED");
  const lease = value as unknown as FormsHostLease;
  assert(exactKeys(lease.release, ["id", "fileManifestSha256"]) && safeId(lease.release.id) &&
    exactKeys(lease.fixture, lease.fixture.mediaPublicationId === undefined ? ["slug", "formId", "formVersion"] : ["slug", "formId", "formVersion", "mediaPublicationId"]) &&
    [lease.plan, lease.certificate, lease.key, lease.ca, lease.chromium, lease.ui.node, lease.ui.source].every(binding => record(binding) && exactKeys(binding, ["path", "sha256"]) && typeof binding.path === "string" && sha(binding.sha256)) &&
    exactKeys(lease.databases, ["path", "sha256", "bytes"]) && lease.databases.bytes > 0 &&
    exactKeys(lease.fixtureGuide, ["path", "sha256", "bytes"]) && lease.fixtureGuide.bytes > 0 &&
    exactKeys(lease.ui, ["version", "observedAt", "listener", "node", "source", "backend"]) &&
    exactKeys(lease.ui.listener, ["address", "port", "pid"]) &&
    exactKeys(lease.ui.backend, ["pid", "jobsPid", "webEntrypoint", "jobsEntrypoint", "state"]) &&
    exactKeys(lease.ui.backend.state, ["path", "sha256", "bytes"]) &&
    Number.isFinite(Date.parse(lease.ui.observedAt)) && lease.ui.version === 1 &&
    lease.ui.listener.address === "127.0.0.1" && lease.ui.listener.port === 3102 && lease.ui.listener.pid === lease.pid &&
    Number.isSafeInteger(lease.ui.backend.pid) && lease.ui.backend.pid > 0 && Number.isSafeInteger(lease.ui.backend.jobsPid) && lease.ui.backend.jobsPid > 0 &&
    lease.ui.backend.pid !== lease.ui.backend.jobsPid && lease.ui.backend.pid !== lease.pid && lease.ui.backend.jobsPid !== lease.pid &&
    typeof lease.ui.backend.state.path === "string" && sha(lease.ui.backend.state.sha256) && Number.isSafeInteger(lease.ui.backend.state.bytes) && lease.ui.backend.state.bytes > 0 &&
    lease.ui.backend.webEntrypoint === join(root, ".runtime/browser", lease.run, "release/server.js") && lease.ui.backend.jobsEntrypoint === join(root, ".runtime/browser", lease.run, "release/tools/jobs.mjs") &&
    lease.requestEvidence.replayCount === lease.requestEvidence.acceptedPosts - 1 &&
    lease.requestEvidence.refusalCount === lease.requestEvidence.refusalLog.length &&
    (lease.requestEvidence.acceptedPosts === 0
      ? lease.requestEvidence.bodySha256 === undefined && lease.requestEvidence.idempotencyKeySha256 === undefined
      : sha(lease.requestEvidence.bodySha256) && sha(lease.requestEvidence.idempotencyKeySha256)) &&
    (!requireCompleted || lease.requestEvidence.acceptedPosts === 2 && lease.requestEvidence.replayCount === 1 && lease.requestEvidence.refusalCount === 0 && lease.requestEvidence.refusalLog.length === 0) &&
    lease.ui.source.path === join(root, "scripts/testing/forms-host-cohort.ts") && lease.ui.backend.state.path === join(root, ".runtime/browser", lease.run, "state.json") &&
    lease.databases.path === join(dirname(path), "forms-host-databases.json") && lease.fixtureGuide.path === join(dirname(path), "forms-host-workplace-platform-guide.json") &&
    resolve(root, lease.certificate.path) === lease.certificate.path && resolve(root, lease.key.path) === lease.key.path &&
    resolve(root, lease.ca.path) === lease.ca.path && resolve(root, lease.plan.path) === lease.plan.path, "FORMS_HOST_LEASE_PROCESS_BINDING_REQUIRED");
  const bindings: { value: Binding; privateFile?: boolean; chromium?: boolean }[] = [
    { value: lease.plan, privateFile: true }, { value: lease.certificate }, { value: lease.key, privateFile: true }, { value: lease.ca },
    { value: lease.chromium, chromium: true }, { value: lease.ui.source }, { value: lease.ui.node },
  ];
  const retainedBindingBytes = new Map<string, Buffer>();
  for (const binding of bindings) {
    const current = await boundFile(root, binding.value, { privateFile: binding.privateFile, chromium: binding.chromium });
    retainedBindingBytes.set(binding.value.path, current.bytes);
  }
  for (const binding of [lease.databases, lease.fixtureGuide]) {
    const current = await boundFile(root, binding, { privateFile: true, maxBytes: 4 * 1024 * 1024 });
    assert(current.bytes.length === binding.bytes, "FORMS_HOST_RETAINED_FIXTURE_LENGTH_CHANGED");
    if (binding === lease.databases) validateDatabaseManifest(JSON.parse(current.bytes.toString("utf8")), lease.run);
    else {
      const fixture = fixtureShape(JSON.parse(current.bytes.toString("utf8")), lease.run);
      assert(fixture.slug === lease.fixture.slug && fixture.formId === lease.fixture.formId && fixture.formVersion === lease.fixture.formVersion &&
        fixture.mediaPublicationId === lease.fixture.mediaPublicationId, "FORMS_HOST_RETAINED_FIXTURE_CHANGED");
    }
  }
  const armBytes = retainedBindingBytes.get(lease.plan.path), certificateBytes = retainedBindingBytes.get(lease.certificate.path),
    privateKeyBytes = retainedBindingBytes.get(lease.key.path), caBytes = retainedBindingBytes.get(lease.ca.path);
  assert(armBytes && certificateBytes && privateKeyBytes && caBytes, "FORMS_HOST_HISTORICAL_TLS_FILES_REQUIRED");
  const arm: unknown = JSON.parse(armBytes.toString("utf8"));
  assert(historicalArmShape(arm), "EXACT_FORMS_HOST_HISTORICAL_ARM_REQUIRED");
  const releasePath = resolve(root, arm.release.path), releaseFilesPath = resolve(root, arm.releaseFiles.path), releaseDirectory = dirname(releasePath);
  assert(releasePath === join(releaseDirectory, "release.json") && releaseFilesPath === join(releaseDirectory, "release-files.json") &&
    pathWithin(releaseDirectory, join(root, ".runtime/releases")) && await realpath(releaseDirectory) === releaseDirectory,
    "FORMS_HOST_HISTORICAL_RELEASE_PATH_REQUIRED");
  const releaseFile = await boundFile(root, arm.release), releaseManifestFile = await boundFile(root, arm.releaseFiles), release: unknown = JSON.parse(releaseFile.bytes.toString("utf8"));
  const certificate = new X509Certificate(certificateBytes), ca = new X509Certificate(caBytes);
  const certificateSpki = hash(certificate.publicKey.export({ format: "der", type: "spki" }));
  const privateKeySpki = hash(createPublicKey(createPrivateKey(privateKeyBytes)).export({ format: "der", type: "spki" }));
  assertFormsHostHistoricalBindings({
    root,
    lease,
    arm,
    release,
    releaseManifest: releaseManifestFile.bytes,
    tls: {
      certificateHostMatches: certificate.subjectAltName === `DNS:${formsHostname}` && certificate.checkHost(formsHostname) === formsHostname,
      certificateIssuerMatchesCa: certificate.issuer === ca.subject,
      certificateValidFrom: Date.parse(certificate.validFrom),
      certificateValidTo: Date.parse(certificate.validTo),
      certificateSpki,
      certificateSignatureValid: certificate.verify(ca.publicKey),
      caIsAuthority: ca.ca,
      privateKeySpki,
    },
  });
  return lease;
}

const readyTimeoutMs = 60_000;
const protocolTimeoutMs = 10_000;
function waitMessage(child: ChildProcess, wanted: string, id?: number) {
  return new Promise<Record<string, unknown>>((yes, no) => {
    const timer = setTimeout(() => finish(Error("FORMS_HOST_EDGE_PROTOCOL_TIMEOUT")), wanted === "ready" ? readyTimeoutMs : protocolTimeoutMs);
    const onMessage = (raw: unknown) => {
      if (!record(raw) || typeof raw.kind !== "string") return;
      if (raw.kind === "error") finish(Error(typeof raw.code === "string" ? raw.code : "FORMS_HOST_EDGE_FAILED"));
      else if (raw.kind === wanted && (id === undefined || raw.id === id)) { clearTimeout(timer); clean(); yes(raw); }
    };
    const onExit = () => finish(Error("FORMS_HOST_EDGE_EXITED"));
    const onError = (error: Error) => finish(error);
    function clean() { child.off("message", onMessage); child.off("exit", onExit); child.off("error", onError); }
    function finish(error: Error) { clearTimeout(timer); clean(); no(error); }
    child.on("message", onMessage); child.once("exit", onExit); child.once("error", onError);
  });
}

/** Start a separately owned pinned Node child for the one-origin Forms lease. */
export async function startFormsHostEdge(rootInput: string, input: FormsHostPlanInput, run: string, output: string) {
  const root = resolve(rootInput);
  assert(input.plan.workflow === formsHostWorkflow && input.plan.runtimeFingerprint === input.release.sourceFingerprint && /^[a-f0-9]{12}$/.test(run), "EXACT_FORMS_HOST_WORKFLOW_ARM_REQUIRED");
  const descriptor = { root, run, output, planPath: input.binding.path, planSha256: input.binding.sha256, runtimeFingerprint: input.plan.runtimeFingerprint,
    verificationFingerprint: input.plan.verificationFingerprint, releaseDirectory: input.directory };
  const descriptorPath = join(output, "forms-host-edge-input.json"), descriptorBytes = Buffer.from(JSON.stringify(descriptor) + "\n");
  await writeFile(descriptorPath, descriptorBytes, { mode: 0o600, flag: "wx" });
  const node = resolve(root, input.plan.tools.node.path); await boundFile(root, input.plan.tools.node);
  const log = await open(join(output, "forms-host-edge.log"), "wx", 0o600);
  const child = fork(join(root, "scripts/testing/forms-host-cohort.ts"), ["--owned-forms-host-edge", descriptorPath, hash(descriptorBytes)], {
    execPath: node, execArgv: [], cwd: root, env: { NODE_ENV: "production", PATH: dirname(node) + ":/usr/bin:/bin", LANG: "en_US.UTF-8", TZ: "Asia/Bangkok" },
    stdio: ["ignore", log.fd, log.fd, "ipc"],
  });
  await log.close();
  const exit = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>(yes => child.once("exit", (code, signal) => yes({ code, signal })));
  try {
    await waitMessage(child, "ready");
    const path = join(output, "forms-host-lease.json"), lease = await readFormsHostLease(root, path, { requireCompleted: false });
    assert(child.pid && lease.pid === child.pid && lease.run === run && lease.sourceFingerprint === input.plan.runtimeFingerprint &&
      lease.verificationFingerprint === input.plan.verificationFingerprint && lease.release.id === input.release.id, "EXACT_FORMS_HOST_EDGE_CHILD_REQUIRED");
    let serial = 0, closed = false;
    const message = (wanted: string, id?: number) => waitMessage(child, wanted, id);
    return {
      path, lease,
      async stable() {
        assert(!closed && child.exitCode === null && child.signalCode === null, "FORMS_HOST_EDGE_CHILD_CLOSED");
        const id = ++serial, pending = message("stable", id); child.send({ kind: "check", id }); await pending; await input.stable();
      },
      async close() {
        if (closed) return; closed = true;
        const stopping = child.exitCode === null && child.signalCode === null ? message("stopped") : undefined;
        if (stopping) child.send({ kind: "stop" });
        const timer = setTimeout(() => child.kill("SIGKILL"), 15_000);
        try { if (stopping) await stopping; const result = await exit; assert(result.code === 0 && result.signal === null, "FORMS_HOST_EDGE_CLEANUP_FAILED"); }
        finally { clearTimeout(timer); }
      },
    };
  } catch (error) {
    if (child.pid && child.exitCode === null && child.signalCode === null) { child.kill("SIGTERM"); const timer = setTimeout(() => child.kill("SIGKILL"), 1_000); try { await exit; } finally { clearTimeout(timer); } }
    throw error;
  }
}

async function runOwnedEdge() {
  let edge: Awaited<ReturnType<typeof listenFormsHostEdge>> | undefined;
  try {
    assert(process.version === "v26.11.1" && process.env.NODE_ENV === "production" && process.argv.length === 5 && process.send, "PINNED_FORMS_HOST_EDGE_REQUIRED");
    const scriptPath = process.argv[1], descriptorPath = process.argv[3], descriptorSha256 = process.argv[4];
    assert(typeof scriptPath === "string" && typeof descriptorPath === "string" && typeof descriptorSha256 === "string", "FORMS_HOST_EDGE_DESCRIPTOR_REQUIRED");
    const descriptorBytes = await readFile(descriptorPath);
    assert(hash(descriptorBytes) === descriptorSha256 && await realpath(descriptorPath) === descriptorPath && (await lstat(descriptorPath)).mode % 0o1000 === 0o600, "FORMS_HOST_EDGE_DESCRIPTOR_REQUIRED");
    const value: unknown = JSON.parse(descriptorBytes.toString("utf8"));
    assert(record(value) && typeof value.root === "string" && typeof value.output === "string" && typeof value.run === "string" &&
      typeof value.planPath === "string" && typeof value.planSha256 === "string" && typeof value.runtimeFingerprint === "string" &&
      typeof value.verificationFingerprint === "string" && typeof value.releaseDirectory === "string" &&
      resolve(scriptPath) === join(value.root, "scripts/testing/forms-host-cohort.ts") && descriptorPath === join(value.output, "forms-host-edge-input.json"), "FORMS_HOST_EDGE_ENTRY_REQUIRED");
    const input = await formsHostPlan(String(value.root), String(value.planPath), String(value.runtimeFingerprint), String(value.verificationFingerprint), String(value.releaseDirectory));
    assert(input.binding.sha256 === value.planSha256 && process.execPath === resolve(String(value.root), input.plan.tools.node.path), "PINNED_FORMS_HOST_EDGE_EXECUTABLE_REQUIRED");
    edge = await listenFormsHostEdge(String(value.root), input, String(value.run), String(value.output));
    const finish = (message: Record<string, unknown>, code: number) => {
      if (!process.connected || !process.send) { process.exit(code); return; }
      process.send(message, error => { process.disconnect?.(); process.exit(error ? 1 : code); });
    };
    let stopping = false;
    const stop = async () => { if (stopping) return; stopping = true; try { await edge!.close(); finish({ kind: "stopped" }, 0); } catch { finish({ kind: "error", code: "FORMS_HOST_EDGE_CLEANUP_FAILED" }, 1); } };
    process.on("disconnect", () => { void stop(); }); process.on("SIGTERM", () => { void stop(); });
    process.on("message", (raw: unknown) => {
      if (!record(raw)) return;
      if (raw.kind === "stop") { void stop(); return; }
      if (raw.kind === "check") void edge!.stable().then(() => process.send?.({ kind: "stable", id: raw.id }), () => process.send?.({ kind: "error", code: "FORMS_HOST_EDGE_CHECK_FAILED" }));
    });
    process.send?.({ kind: "ready" });
  } catch (error) {
    process.exitCode = 1; try { if (edge) await edge.close(); } catch { /* Preserve the startup failure. */ }
    process.send?.({ kind: "error", code: error instanceof Error && /^[A-Z0-9_]+$/.test(error.message) ? error.message : "FORMS_HOST_EDGE_START_FAILED" });
    if (process.connected) process.disconnect?.();
  }
}

if (process.argv[2] === "--owned-forms-host-edge") void runOwnedEdge();
