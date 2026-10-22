/** Isolated read-only browser transport for the Company Executive Work proof. */
import { createHash, createPrivateKey, createPublicKey, X509Certificate } from "node:crypto";
import { chmod, lstat, readFile, realpath, rename, writeFile } from "node:fs/promises";
import { createServer as createHttpServer, request as httpRequest, type IncomingMessage, type IncomingHttpHeaders } from "node:http";
import { createServer as createHttpsServer } from "node:https";
import { createServer as createNetServer, Socket } from "node:net";
import type { TLSSocket } from "node:tls";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";

export const executiveWorkHostBrowserFile = "tests/e2e/shared.executive/executive-work-mcp.spec.ts";
export const executiveWorkHostCaseTitle = "isolated Work Executive UI reads a seeded connection through scoped MCP";
export const executiveWorkOrigin = "https://work-dev-erpx.jesadakorn.com";
export const executiveAccountOrigin = "https://account-dev-erpx.jesadakorn.com";
export const executiveWorkOrigins = [executiveWorkOrigin, executiveAccountOrigin] as const;
const hash = (value: Uint8Array | string) => createHash("sha256").update(value).digest("hex");
const assert: (condition: unknown, code: string) => asserts condition = (condition, code) => { if (!condition) throw new Error(code); };
const isRecord = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const sha = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const methods = ["initialize", "notifications/initialized", "tools/list", "tools/call"] as const;
const tools = ["list_metrics", "query_metric"] as const;
const knownChromiumBackgroundHosts = new Set(["accounts.google.com", "android.clients.google.com", "clients2.google.com", "content-autofill.googleapis.com", "update.googleapis.com", "www.google.com"]);
export type ExecutiveWorkBackgroundTraffic = { deniedConnectCount: number; deniedProxyRequestCount: number; connectHosts: Record<string, number>; proxyRequestHosts: Record<string, number> };
export function isKnownExecutiveWorkChromiumBackgroundHost(hostname: string) {
  return knownChromiumBackgroundHosts.has(hostname.toLowerCase());
}
export function executiveWorkBackgroundTrafficIsCurrent(persisted: ExecutiveWorkBackgroundTraffic, live: ExecutiveWorkBackgroundTraffic) {
  return JSON.stringify(persisted) === JSON.stringify(live);
}

export type ExecutiveWorkExpected = { run: string; companyId: string; assignmentId: string; assignmentVersion: number; email: string; password: string; token: string };
export type ExecutiveWorkPolicyInput = { method: string; origin: string; path: string; search?: string; headers: Record<string, string | undefined>; body: string };

function exactKeys(value: Record<string, unknown>, expected: readonly string[]) {
  return JSON.stringify(Object.keys(value).sort()) === JSON.stringify([...expected].sort());
}

function validBackgroundTraffic(value: unknown): value is ExecutiveWorkHostLease["backgroundTraffic"] {
  if (!isRecord(value) || !exactKeys(value, ["deniedConnectCount", "deniedProxyRequestCount", "connectHosts", "proxyRequestHosts"])) return false;
  const validCount = (count: unknown) => Number.isSafeInteger(count) && Number(count) >= 0;
  const validHosts = (hosts: unknown) => isRecord(hosts) && Object.entries(hosts).every(([hostname, count]) =>
    knownChromiumBackgroundHosts.has(hostname) && validCount(count));
  return validCount(value.deniedConnectCount) && validCount(value.deniedProxyRequestCount) &&
    validHosts(value.connectHosts) && validHosts(value.proxyRequestHosts);
}

function allowedMcpMessage(raw: unknown): boolean {
  if (!isRecord(raw) || raw.jsonrpc !== "2.0" || typeof raw.method !== "string") return false;
  if (raw.method === "notifications/initialized") return exactKeys(raw, ["jsonrpc", "method"]) || exactKeys(raw, ["jsonrpc", "method", "params"]) && isRecord(raw.params) && Object.keys(raw.params).length === 0;
  if (!Object.hasOwn({ initialize: 1, "tools/list": 1, "tools/call": 1 }, raw.method) || !Object.hasOwn(raw, "id") || !["string", "number"].includes(typeof raw.id)) return false;
  if (raw.method === "tools/list") return exactKeys(raw, ["jsonrpc", "id", "method"]) ||
    exactKeys(raw, ["jsonrpc", "id", "method", "params"]) && isRecord(raw.params) && exactKeys(raw.params, []);
  if (!exactKeys(raw, ["jsonrpc", "id", "method", "params"]) || !isRecord(raw.params)) return false;
  if (raw.method === "initialize") {
    const p = raw.params;
    return exactKeys(p, ["protocolVersion", "capabilities", "clientInfo"]) && typeof p.protocolVersion === "string" && /^202\d-\d{2}-\d{2}$/.test(p.protocolVersion) &&
      isRecord(p.capabilities) && Object.keys(p.capabilities).length === 0 && isRecord(p.clientInfo) &&
      exactKeys(p.clientInfo, ["name", "version"]) && typeof p.clientInfo.name === "string" && p.clientInfo.name.length <= 100 &&
      typeof p.clientInfo.version === "string" && p.clientInfo.version.length <= 80;
  }
  if (raw.method !== "tools/call" || !exactKeys(raw.params, ["name", "arguments"]) || typeof raw.params.name !== "string" || !isRecord(raw.params.arguments)) return false;
  if (raw.params.name === "list_metrics") return exactKeys(raw.params.arguments, []);
  if (raw.params.name !== "query_metric") return false;
  const args = raw.params.arguments;
  if (!exactKeys(args, ["metric", "filters", "breakdown"]) || args.metric !== "headcount" || args.breakdown !== "default" || !isRecord(args.filters) || !exactKeys(args.filters, ["from", "to", "currency"])) return false;
  const validDate = (value: unknown): value is string => {
    if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
    const parsed = Date.parse(`${value}T00:00:00.000Z`);
    return Number.isFinite(parsed) && new Date(parsed).toISOString().slice(0, 10) === value;
  };
  return validDate(args.filters.from) && validDate(args.filters.to) && args.filters.currency === "THB" && args.filters.from <= args.filters.to;
}

function ssoCallback(value: unknown): boolean {
  if (typeof value !== "string" || value.length > 2048) return false;
  try {
    const url = new URL(value, executiveAccountOrigin);
    return url.origin === executiveAccountOrigin && url.pathname === "/sso/authorize" && url.hash === "" &&
      [...url.searchParams.keys()].sort().join(",") === "challenge,state,transaction" &&
      [...url.searchParams.values()].every(part => /^[A-Za-z0-9_-]{20,160}$/.test(part));
  } catch { return false; }
}

function sameOriginBrowserDocumentNavigation(input: ExecutiveWorkPolicyInput) {
  return input.headers.origin === "null" && input.headers["sec-fetch-site"] === "same-origin" &&
    input.headers["sec-fetch-mode"] === "navigate" && input.headers["sec-fetch-dest"] === "document";
}

function allowedRoleSelection(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected) {
  const originAllowed = input.headers.origin === executiveWorkOrigin || sameOriginBrowserDocumentNavigation(input);
  if (input.method !== "POST" || input.origin !== executiveWorkOrigin || input.path !== "/launch" || !originAllowed ||
      !input.headers["content-type"]?.toLowerCase().startsWith("application/x-www-form-urlencoded")) return false;
  const form = new URLSearchParams(input.body), keys = [...form.keys()].sort();
  if (keys.join(",") !== "assignmentId,assignmentVersion,companyId,next,revision" || keys.some(key => form.getAll(key).length !== 1)) return false;
  const version = Number(form.get("assignmentVersion")), revision = Number(form.get("revision"));
  return form.get("companyId") === expected.companyId && form.get("next") === "/holdings/acme/insights/executive" &&
    form.get("assignmentId") === expected.assignmentId && version === expected.assignmentVersion && Number.isSafeInteger(revision) && revision >= 0;
}

function allowedWorkspacePreference(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected, selectedRevision?: number) {
  let body: unknown; try { body = JSON.parse(input.body); } catch { return false; }
  const revisionHeader = input.headers["x-erpx-context-revision"];
  return input.method === "POST" && input.origin === executiveWorkOrigin && input.headers.origin === executiveWorkOrigin &&
    input.path === "/api/v1/workspace-preference" && (input.search ?? "") === "" &&
    input.headers["sec-fetch-site"] === "same-origin" && input.headers["sec-fetch-mode"] === "cors" && input.headers["sec-fetch-dest"] === "empty" &&
    input.headers["content-type"]?.toLowerCase().startsWith("application/json") === true && input.body.length <= 1024 &&
    isRecord(body) && exactKeys(body, ["base"]) && body.base === "/holdings/acme" && input.body === JSON.stringify({ base: "/holdings/acme" }) &&
    input.headers["x-erpx-context-company"] === expected.companyId && input.headers["x-erpx-context-assignment"] === expected.assignmentId &&
    Number.isSafeInteger(selectedRevision) && selectedRevision! >= 1 && revisionHeader === String(selectedRevision);
}

function allowedLogin(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected) {
  if (input.method !== "POST" || input.origin !== executiveAccountOrigin || input.path !== "/api/auth/sign-in/email" || input.headers.origin !== executiveAccountOrigin ||
      !input.headers["content-type"]?.toLowerCase().startsWith("application/json")) return false;
  let body: unknown; try { body = JSON.parse(input.body); } catch { return false; }
  if (!isRecord(body) || !exactKeys(body, ["email", "password", "callbackURL"]) && !exactKeys(body, ["email", "password", "callbackURL", "rememberMe"])) return false;
  return body.email === expected.email && body.password === expected.password && ssoCallback(body.callbackURL) &&
    (body.rememberMe === undefined || typeof body.rememberMe === "boolean");
}

function allowedMcp(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected) {
  if (input.method !== "POST" || input.origin !== executiveWorkOrigin || input.path !== "/api/mcp/executive" ||
      input.headers.authorization !== `Bearer ${expected.token}` || !input.headers["content-type"]?.toLowerCase().startsWith("application/json") ||
      Number(input.headers["content-length"] ?? 0) > 32 * 1024) return false;
  let body: unknown; try { body = JSON.parse(input.body); } catch { return false; }
  return allowedMcpMessage(body);
}

function allowedMcpEventStreamRead(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected) {
  return input.method === "GET" && input.origin === executiveWorkOrigin && input.path === "/api/mcp/executive" &&
    (input.search ?? "") === "" && input.body === "" && input.headers.origin === executiveWorkOrigin &&
    input.headers.authorization === `Bearer ${expected.token}` && input.headers.accept === "text/event-stream" &&
    input.headers.cookie === undefined && input.headers["content-type"] === undefined &&
    (input.headers["content-length"] === undefined || input.headers["content-length"] === "0") &&
    input.headers["transfer-encoding"] === undefined;
}

/** The official Streamable HTTP client opens this token-bound SSE read after a 202 notification. */
export function executiveWorkGetKind(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected): "mcp-event-stream" | null {
  return allowedMcpEventStreamRead(input, expected) ? "mcp-event-stream" : null;
}

/** Only these exact role/auth/read requests cross the edge; business writes fail closed. */
export function executiveWorkPostKind(input: ExecutiveWorkPolicyInput, expected: ExecutiveWorkExpected, selectedRevision?: number): "login" | "role-selection" | "workspace-preference" | "mcp-read" | null {
  if (allowedLogin(input, expected)) return "login";
  if (allowedRoleSelection(input, expected)) return "role-selection";
  if (allowedWorkspacePreference(input, expected, selectedRevision)) return "workspace-preference";
  if (allowedMcp(input, expected)) return "mcp-read";
  return null;
}

export function executiveWorkDenialProbe(input: ExecutiveWorkPolicyInput) {
  return input.method === "POST" && input.origin === executiveWorkOrigin && input.path === "/api/v1/holdings/acme/executive/connections" &&
    input.headers["x-erpx-executive-denial-probe"] === "deny-only";
}

type Binding = { path: string; sha256: string; bytes: number };
type Release = { id: string; fileManifestSha256: string };
export type ExecutiveWorkHostLease = {
  schema: "erpx-owned-executive-work-host-browser/v1"; run: string; pid: number; port: 3102; upstream: "http://127.0.0.1:3104";
  sourceFingerprint: string; verificationFingerprint: string; release: Release; origins: string[]; workOnly: true; accountOnlyForSso: true;
  commercialMfa: false; commercialApply: false; packageEnforcement: false; installedWrites: false; businessWrites: false;
  mcp: { methods: readonly string[]; tools: readonly string[]; companyScope: true; credentialBound: true; businessReadOnly: true };
  backgroundTraffic: ExecutiveWorkBackgroundTraffic;
  denialProbe: { method: "POST"; pathSha256: string; status: 403; count: 1 };
  ca: Binding; certificate: Binding; spki: string; chromium: { path: string; sha256: string }; node: { path: string; sha256: string };
  ui: { listener: { address: "127.0.0.1"; port: 3102; pid: number }; backend: { pid: number; jobsPid: number; state: Binding }; source: { path: string; sha256: string } };
};

async function stablePrivateFile(root: string, path: string, within: string, max = 16 * 1024 * 1024) {
  const file = resolve(root, path);
  assert(file.startsWith(within + "/") && await realpath(file) === file, "EXECUTIVE_WORK_PRIVATE_FILE_BOUNDARY_REQUIRED");
  const stat = await lstat(file);
  assert(stat.isFile() && !stat.isSymbolicLink() && (stat.mode & 0o077) === 0 && stat.size <= max, "EXECUTIVE_WORK_PRIVATE_FILE_REQUIRED");
  const bytes = await readFile(file);
  return { bytes, binding: { path: file, sha256: hash(bytes), bytes: bytes.length } };
}

function credentialFrom(value: unknown): ExecutiveWorkExpected {
  assert(isRecord(value) && value.profile === "executive-work" && /^[a-f0-9]{12}$/.test(String(value.run)) &&
    typeof value.companyId === "string" && /^[A-Za-z0-9_-]{1,180}$/.test(value.companyId) &&
    typeof value.assignmentId === "string" && /^[A-Za-z0-9_-]{1,180}$/.test(value.assignmentId) && value.assignmentVersion === 1 &&
    typeof value.email === "string" && /^[^\s@]+@example\.test$/.test(value.email) && typeof value.password === "string" && value.password.length >= 20 &&
    typeof value.token === "string" && /^erpx_exec_[A-Za-z0-9_-]{43}$/.test(value.token), "EXECUTIVE_WORK_FIXTURE_CREDENTIALS_REQUIRED");
  return value as unknown as ExecutiveWorkExpected;
}

async function freePort(port: number) {
  const server = createNetServer();
  await new Promise<void>((yes, no) => { server.once("error", no); server.listen(port, "127.0.0.1", () => server.close(error => error ? no(error) : yes())); });
}

async function readDbManifest(root: string, run: string) {
  const bytes = await readFile(join(root, ".runtime/browser", run, "databases.json"));
  const value: unknown = JSON.parse(bytes.toString());
  assert(isRecord(value) && isRecord(value.databases) && JSON.stringify(Object.keys(value.databases).sort()) === JSON.stringify(["CONTROL_DB", "CONTROL_READ_DB", "H001_DB", "H001_READ_DB", "H002_DB", "H002_READ_DB", "H003_DB", "H003_READ_DB"].sort()), "EXECUTIVE_WORK_EXACT_TEST_DATABASES_REQUIRED");
  const databases = value.databases;
  for (const urlText of Object.values(databases)) {
    assert(typeof urlText === "string", "EXECUTIVE_WORK_TEST_DATABASE_REQUIRED");
    const url = new URL(urlText);
    const binding = Object.entries(databases).find(([, candidate]) => candidate === urlText)?.[0];
    assert(binding && url.hostname === "127.0.0.1" && url.port === "5432" && url.username === "erpx_test_admin" && url.pathname === `/erpx_browser_${run}_${binding.toLowerCase()}`, "EXECUTIVE_WORK_TEST_DATABASE_IDENTITY_REQUIRED");
  }
  return hash(bytes);
}

async function writeEphemeralCertificate(root: string, output: string) {
  const path = join(output, "executive-work-host.pem"), keyPath = join(output, "executive-work-host-key.pem");
  const binary = "/opt/homebrew/bin/openssl";
  const result = spawnSync(binary, ["req", "-x509", "-newkey", "rsa:2048", "-sha256", "-nodes", "-days", "1", "-keyout", keyPath, "-out", path,
    "-subj", "/CN=ERPX isolated Executive Work host", "-addext", `subjectAltName=DNS:${new URL(executiveWorkOrigin).hostname},DNS:${new URL(executiveAccountOrigin).hostname}`,
    "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "keyUsage=critical,digitalSignature,keyEncipherment,keyCertSign,cRLSign"],
    { cwd: root, env: { NODE_ENV: process.env.NODE_ENV ?? "production", PATH: "/opt/homebrew/bin:/usr/bin:/bin" }, encoding: "utf8" });
  assert(result.status === 0, "EXECUTIVE_WORK_TLS_CERTIFICATE_GENERATION_FAILED");
  await chmod(path, 0o600); await chmod(keyPath, 0o600);
  const [cert, key] = await Promise.all([readFile(path), readFile(keyPath)]), x509 = new X509Certificate(cert), spki = hash(x509.publicKey.export({ format: "der", type: "spki" }));
  assert(x509.ca && x509.verify(x509.publicKey) && Date.parse(x509.validFrom) <= Date.now() && Date.parse(x509.validTo) > Date.now() &&
    x509.checkHost(new URL(executiveWorkOrigin).hostname) === new URL(executiveWorkOrigin).hostname && x509.checkHost(new URL(executiveAccountOrigin).hostname) === new URL(executiveAccountOrigin).hostname &&
    JSON.stringify(x509.subjectAltName?.split(", ").sort()) === JSON.stringify(executiveWorkOrigins.map(origin => `DNS:${new URL(origin).hostname}`).sort()) &&
    spki === hash(createPublicKey(createPrivateKey(key)).export({ format: "der", type: "spki" })), "EXECUTIVE_WORK_TLS_CERTIFICATE_SCOPE_REQUIRED");
  return { path, keyPath, cert, spki, keySha256: hash(key) };
}

async function listen(root: string, descriptor: { run: string; output: string; runtimeFingerprint: string; verificationFingerprint: string; releaseDirectory: string; nodePath: string; chromiumPath: string }, credentials: ExecutiveWorkExpected) {
  const directory = join(root, ".runtime/browser", descriptor.run), statePath = join(directory, "state.json"), stateBytes = await readFile(statePath);
  const state = JSON.parse(stateBytes.toString()) as { run: string; prefix: string; port: number; origin: string; profile: string; executiveWorkHosts?: boolean; productHosts?: boolean; sourceFingerprint: string; release?: { id: string; fileManifestSha256: string }; pid?: number; jobsPid?: number; webEntrypoint?: string; jobsEntrypoint?: string };
  assert(state.run === descriptor.run && state.prefix === `erpx_browser_${descriptor.run}` && state.port === 3104 && state.origin === "http://127.0.0.1:3104" && state.profile === "executive-work" && state.executiveWorkHosts === true && state.productHosts !== true && state.sourceFingerprint === descriptor.runtimeFingerprint && state.release && state.pid && state.jobsPid, "EXECUTIVE_WORK_OWNED_COMPILED_COHORT_REQUIRED");
  const release = JSON.parse(await readFile(join(descriptor.releaseDirectory, "release.json"), "utf8")) as { id: string; sourceFingerprint: string; node: string; fileManifestSha256: string };
  assert(release.id === state.release.id && release.sourceFingerprint === descriptor.runtimeFingerprint && release.node === "26.11.1" && release.fileManifestSha256 === state.release.fileManifestSha256, "EXECUTIVE_WORK_RELEASE_IDENTITY_REQUIRED");
  const databaseManifestSha256 = await readDbManifest(root, descriptor.run);
  const stateBinding = { path: statePath, sha256: hash(stateBytes), bytes: stateBytes.length };
  const sourcePath = join(root, "scripts/testing/executive-work-host-cohort.ts"), sourceBinding = { path: sourcePath, sha256: hash(await readFile(sourcePath)) };
  const nodeBinding = { path: resolve(descriptor.nodePath), sha256: hash(await readFile(descriptor.nodePath)) };
  const chromiumBinding = { path: resolve(descriptor.chromiumPath), sha256: hash(await readFile(descriptor.chromiumPath)) };
  const output = resolve(descriptor.output), evidenceRoot = resolve(root, ".scratch/dev/test-suites/browser");
  assert(output.startsWith(evidenceRoot + "/") && output.endsWith(`/${descriptor.run}`), "EXECUTIVE_WORK_CASE_OUTPUT_REQUIRED");
  await freePort(3102);
  const tls = await writeEphemeralCertificate(root, output), certificateBinding = { path: tls.path, sha256: hash(tls.cert), bytes: tls.cert.length };
  const socketSet = new Set<Socket>(), refusedConnectHosts: Record<string, number> = {}, refusedProxyRequestHosts: Record<string, number> = {}, refusedTlsHosts: Record<string, number> = {};
  const backgroundTraffic: ExecutiveWorkHostLease["backgroundTraffic"] = { deniedConnectCount: 0, deniedProxyRequestCount: 0, connectHosts: {}, proxyRequestHosts: {} };
  const refusedTlsRoutes: Record<string, { method: string; pathnameSha256: string; count: number }> = {};
  let unexpectedRefusals = 0, unexpectedConnects = 0, denialProbes = 0, closed = false, selectedRevision: number | undefined;
  const hosts = executiveWorkOrigins.map(origin => new URL(origin).host);
  const countRefusedHostname = (counts: Record<string, number>, value: string | undefined, hostOnly = false) => {
    let hostname = "invalid";
    try {
      const parsed = new URL(hostOnly ? `http://${value ?? ""}` : value ?? "");
      if (parsed.username || parsed.password || !parsed.hostname || !/^[a-z0-9.-]{1,253}$/i.test(parsed.hostname)) throw Error();
      hostname = parsed.hostname.toLowerCase();
    } catch { /* Retain only an invalid aggregate for malformed host values. */ }
    if (Object.hasOwn(counts, hostname)) counts[hostname]++;
    else if (Object.keys(counts).length < 8) counts[hostname] = 1;
    else counts.other = (counts.other ?? 0) + 1;
    return hostname;
  };
  const countKnownBackground = (kind: "connect" | "proxyRequest", hostname: string) => {
    if (!isKnownExecutiveWorkChromiumBackgroundHost(hostname)) return false;
    if (kind === "connect") {
      backgroundTraffic.deniedConnectCount++;
      backgroundTraffic.connectHosts[hostname] = (backgroundTraffic.connectHosts[hostname] ?? 0) + 1;
    } else {
      backgroundTraffic.deniedProxyRequestCount++;
      backgroundTraffic.proxyRequestHosts[hostname] = (backgroundTraffic.proxyRequestHosts[hostname] ?? 0) + 1;
    }
    return true;
  };
  const countRefusedTlsRoute = (req: IncomingMessage) => {
    const rawMethod = String(req.method ?? "OTHER").toUpperCase(), method = /^[A-Z]{1,16}$/.test(rawMethod) ? rawMethod : "OTHER";
    let pathnameSha256 = "invalid";
    try {
      const host = String(req.headers.host ?? "");
      if (!hosts.includes(host)) throw Error();
      pathnameSha256 = hash(new URL(req.url ?? "/", `https://${host}`).pathname);
    } catch { /* Keep only a fixed invalid marker for malformed TLS targets. */ }
    const key = `${method}:${pathnameSha256}`;
    if (Object.hasOwn(refusedTlsRoutes, key)) refusedTlsRoutes[key].count++;
    else if (Object.keys(refusedTlsRoutes).length < 8) refusedTlsRoutes[key] = { method, pathnameSha256, count: 1 };
    else if (Object.hasOwn(refusedTlsRoutes, "other:invalid")) refusedTlsRoutes["other:invalid"].count++;
    else refusedTlsRoutes["other:invalid"] = { method: "OTHER", pathnameSha256: "invalid", count: 1 };
  };
  const refusal = (req: IncomingMessage, res: import("node:http").ServerResponse, probe: boolean) => {
    req.resume();
    if (probe) denialProbes++; else { unexpectedRefusals++; countRefusedHostname(refusedTlsHosts, req.headers.host, true); countRefusedTlsRoute(req); }
    res.writeHead(403, { "Cache-Control": "no-store", ...(probe ? { "X-ERPX-Test-Boundary": "executive-business-write-denied" } : {}) });
    res.end("Executive Work isolated test edge refused this request.");
  };
  const tlsServer = createHttpsServer({ key: await readFile(tls.keyPath), cert: tls.cert, minVersion: "TLSv1.2" }, (req, res) => { void (async () => {
    const host = String(req.headers.host ?? ""), origin = `https://${host}`;
    try {
      const tlsSocket = req.socket as TLSSocket;
      assert(hosts.includes(host) && tlsSocket.remoteAddress === "127.0.0.1" && tlsSocket.servername === host, "EXECUTIVE_WORK_EDGE_HOST_REFUSED");
      assert(!Object.keys(req.headers).some(name => name === "forwarded" || name === "proxy-authorization" || name.startsWith("x-forwarded-")), "EXECUTIVE_WORK_CALLER_FORWARDING_REFUSED");
      const target = new URL(req.url ?? "/", origin);
      assert(target.origin === origin && !target.username && !target.password && target.hash === "", "EXECUTIVE_WORK_EDGE_TARGET_REFUSED");
      const method = String(req.method ?? "").toUpperCase(), input: ExecutiveWorkPolicyInput = { method, origin, path: target.pathname, search: target.search, headers: Object.fromEntries(Object.entries(req.headers).map(([name, value]) => [name.toLowerCase(), Array.isArray(value) ? value.join(",") : value])), body: "" };
      if (input.headers.origin !== undefined) assert(input.headers.origin === origin ||
        method === "POST" && target.pathname === "/launch" && sameOriginBrowserDocumentNavigation(input), "EXECUTIVE_WORK_EDGE_ORIGIN_REFUSED");
      if (method === "POST") {
        if (executiveWorkDenialProbe(input)) { refusal(req, res, true); return; }
        const chunks: Buffer[] = []; let size = 0;
        for await (const part of req) { const chunk = Buffer.isBuffer(part) ? part : Buffer.from(part); size += chunk.length; assert(size <= 32 * 1024, "EXECUTIVE_WORK_POST_BODY_TOO_LARGE"); chunks.push(chunk); }
        input.body = Buffer.concat(chunks).toString("utf8");
        const postKind = executiveWorkPostKind(input, credentials, selectedRevision);
        if (!postKind) {
          refusal(req, res, false); return;
        }
        if (postKind === "role-selection") {
          const revision = Number(new URLSearchParams(input.body).get("revision"));
          assert(Number.isSafeInteger(revision) && revision >= 0 && revision < Number.MAX_SAFE_INTEGER, "EXECUTIVE_WORK_ROLE_REVISION_REQUIRED");
          selectedRevision = revision + 1;
        }
        assert(input.headers.origin === origin || postKind === "role-selection" && sameOriginBrowserDocumentNavigation(input), "EXECUTIVE_WORK_POST_ORIGIN_REQUIRED");
      } else if (method === "GET" && target.pathname === "/api/mcp/executive") {
        if (!executiveWorkGetKind(input, credentials)) { refusal(req, res, false); return; }
      } else if (!(["GET", "HEAD"].includes(method) && target.pathname !== "/api/mcp/executive")) { refusal(req, res, false); return; }
      const headers: IncomingHttpHeaders = { ...req.headers, host, "x-forwarded-host": host, "x-forwarded-proto": "https", "x-forwarded-port": "443" };
      delete headers["proxy-authorization"]; delete headers["connection"];
      const upstream = httpRequest({ hostname: "127.0.0.1", port: 3104, path: target.pathname + target.search, method, headers, agent: false }, reply => {
        try {
          for (const raw of Array.isArray(reply.headers.location) ? reply.headers.location : reply.headers.location ? [reply.headers.location] : []) {
            const next = new URL(raw, target); assert(executiveWorkOrigins.includes(next.origin as typeof executiveWorkOrigins[number]) && !next.username && !next.password, "EXECUTIVE_WORK_REDIRECT_ESCAPE");
          }
          res.writeHead(reply.statusCode ?? 502, reply.headers); reply.pipe(res);
        } catch { reply.destroy(); res.writeHead(502); res.end(); }
      });
      upstream.on("error", () => { if (!res.headersSent) res.writeHead(502); res.end(); });
      upstream.end(method === "POST" ? input.body : undefined);
    } catch { refusal(req, res, false); }
  })(); });
  const proxy = createHttpServer((_req, res) => { res.writeHead(403); res.end(); });
  // The default handler returns 403 and never proxies plain requests. Only exact
  // Chromium telemetry hosts are accounted separately; unknown hosts keep the gate red.
  proxy.on("request", req => {
    const hostname = countRefusedHostname(refusedProxyRequestHosts, req.url);
    if (req.method !== "GET" || req.socket.remoteAddress !== "127.0.0.1" || !countKnownBackground("proxyRequest", hostname)) unexpectedRefusals++;
  });
  proxy.on("connection", socket => { socketSet.add(socket); socket.once("close", () => socketSet.delete(socket)); });
  proxy.on("connect", (req, socket, head) => {
    const match = /^([a-z0-9.-]+):443$/.exec(req.url ?? "");
    if (!(socket instanceof Socket) || !match || !hosts.includes(match[1]) || socket.remoteAddress !== "127.0.0.1") {
      const hostname = countRefusedHostname(refusedConnectHosts, match?.[1] ?? req.url, Boolean(match));
      if (!(socket instanceof Socket && socket.remoteAddress === "127.0.0.1" && countKnownBackground("connect", hostname))) unexpectedConnects++;
      socket.end("HTTP/1.1 403 Forbidden\r\n\r\n"); return;
    }
    socket.write("HTTP/1.1 200 Connection Established\r\n\r\n"); if (head.length) socket.unshift(head); tlsServer.emit("connection", socket);
  });
  await new Promise<void>((yes, no) => { proxy.once("error", no); proxy.listen(3102, "127.0.0.1", () => yes()); });
  const address = proxy.address(); assert(address && typeof address === "object" && address.address === "127.0.0.1" && address.port === 3102, "EXECUTIVE_WORK_ACTUAL_LISTENER_REQUIRED");
  const ca = await stablePrivateFile(root, tls.path, output), lease: ExecutiveWorkHostLease = {
    schema: "erpx-owned-executive-work-host-browser/v1", run: descriptor.run, pid: process.pid, port: 3102, upstream: "http://127.0.0.1:3104",
    sourceFingerprint: descriptor.runtimeFingerprint, verificationFingerprint: descriptor.verificationFingerprint, release: { id: release.id, fileManifestSha256: release.fileManifestSha256 },
    origins: [...executiveWorkOrigins], workOnly: true, accountOnlyForSso: true, commercialMfa: false, commercialApply: false, packageEnforcement: false, installedWrites: false, businessWrites: false,
    mcp: { methods, tools, companyScope: true, credentialBound: true, businessReadOnly: true },
    backgroundTraffic: structuredClone(backgroundTraffic),
    denialProbe: { method: "POST", pathSha256: hash("/api/v1/holdings/acme/executive/connections"), status: 403, count: 1 },
    ca: ca.binding, certificate: certificateBinding, spki: tls.spki, chromium: chromiumBinding, node: nodeBinding,
    ui: { listener: { address: "127.0.0.1", port: 3102, pid: process.pid }, backend: { pid: Number(state.pid), jobsPid: Number(state.jobsPid), state: stateBinding }, source: sourceBinding },
  };
  const leasePath = join(output, "executive-work-host-lease.json"); let leaseText = JSON.stringify(lease, null, 2) + "\n";
  await writeFile(leasePath, leaseText, { flag: "wx", mode: 0o600 }); await chmod(leasePath, 0o600);
  const persistBackgroundTraffic = async () => {
    assert((await readFile(leasePath, "utf8")) === leaseText, "EXECUTIVE_WORK_HOST_LEASE_CHANGED");
    lease.backgroundTraffic = structuredClone(backgroundTraffic);
    const nextText = JSON.stringify(lease, null, 2) + "\n";
    if (nextText === leaseText) return;
    const nextPath = leasePath + ".next";
    await writeFile(nextPath, nextText, { flag: "wx", mode: 0o600 }); await chmod(nextPath, 0o600);
    await rename(nextPath, leasePath); leaseText = nextText;
  };
  const matches = async (check: () => Promise<boolean>) => { try { return await check(); } catch { return false; } };
  const stabilityFacts = async () => {
    const leaseBindingMatches = await matches(async () => (await readFile(leasePath, "utf8")) === leaseText);
    const originalStateMatches = await matches(async () => (await readFile(statePath)).equals(stateBytes));
    const tlsCertificateMatches = await matches(async () => hash(await readFile(tls.path)) === lease.certificate.sha256);
    const tlsKeyMatches = await matches(async () => hash(await readFile(tls.keyPath)) === tls.keySha256);
    const databaseManifestMatches = await matches(async () => await readDbManifest(root, descriptor.run) === databaseManifestSha256);
    const sourceBindingMatches = await matches(async () => hash(await readFile(sourcePath)) === sourceBinding.sha256);
    const nodeBindingMatches = await matches(async () => hash(await readFile(nodeBinding.path)) === nodeBinding.sha256);
    const chromiumBindingMatches = await matches(async () => hash(await readFile(chromiumBinding.path)) === chromiumBinding.sha256);
    // Snapshot all counters only after every awaited binding check, so traffic
    // received during file verification cannot be hidden by an earlier copy.
    const address = proxy.address(), liveBackgroundTraffic = structuredClone(backgroundTraffic);
    return {
      edgeOpen: !closed && proxy.listening,
      loopbackListener: address !== null && typeof address === "object" && address.address === "127.0.0.1" && address.port === 3102,
      edgeClosed: closed && !proxy.listening,
      loopbackClosed: address === null,
      denialProbeCount: denialProbes,
      unexpectedRefusalCount: unexpectedRefusals,
      unexpectedConnectCount: unexpectedConnects,
      refusedConnectHosts: { ...refusedConnectHosts },
      refusedProxyRequestHosts: { ...refusedProxyRequestHosts },
      refusedTlsHosts: { ...refusedTlsHosts },
      refusedTlsRoutes: { ...refusedTlsRoutes },
      openSocketCount: socketSet.size,
      leaseBindingMatches,
      originalStateMatches,
      tlsCertificateMatches,
      tlsKeyMatches,
      databaseManifestMatches,
      sourceBindingMatches,
      nodeBindingMatches,
      chromiumBindingMatches,
      backgroundTrafficBindingMatches: executiveWorkBackgroundTrafficIsCurrent(lease.backgroundTraffic, liveBackgroundTraffic),
    };
  };
  const trafficStable = (facts: Awaited<ReturnType<typeof stabilityFacts>>) => facts.denialProbeCount === 1 &&
    facts.unexpectedRefusalCount === 0 && facts.unexpectedConnectCount === 0 && facts.openSocketCount === 0 &&
    facts.leaseBindingMatches && facts.backgroundTrafficBindingMatches && facts.originalStateMatches && facts.tlsCertificateMatches &&
    facts.tlsKeyMatches && facts.databaseManifestMatches && facts.sourceBindingMatches && facts.nodeBindingMatches && facts.chromiumBindingMatches;
  return {
    path: leasePath, lease,
    async stable() {
      await persistBackgroundTraffic();
      const facts = await stabilityFacts();
      const stable = facts.edgeOpen && facts.loopbackListener && trafficStable(facts);
      assert(stable, `EXECUTIVE_WORK_EDGE_STABILITY_FAILED:${JSON.stringify(facts)}`);
    },
    async close() {
      if (closed) return;
      for (const socket of socketSet) socket.destroy();
      await new Promise<void>((yes, no) => proxy.close(error => error ? no(error) : yes()));
      closed = true; assert(socketSet.size === 0, "EXECUTIVE_WORK_EDGE_SOCKETS_REMAIN");
      await persistBackgroundTraffic();
      const facts = await stabilityFacts();
      assert(facts.edgeClosed && facts.loopbackClosed && trafficStable(facts), `EXECUTIVE_WORK_EDGE_CLOSE_STABILITY_FAILED:${JSON.stringify(facts)}`);
      const { unlink } = await import("node:fs/promises"); await unlink(tls.keyPath);
    },
  };
}

export async function readExecutiveWorkHostLease(rootInput: string, path: string): Promise<ExecutiveWorkHostLease> {
  const root = resolve(rootInput), outputRoot = resolve(root, ".scratch/dev/test-suites/browser"), file = resolve(path);
  assert(isAbsolute(path) && file.startsWith(outputRoot + "/") && await realpath(file) === file && file.endsWith("/executive-work-host-lease.json"), "EXECUTIVE_WORK_HOST_LEASE_REQUIRED");
  const stat = await lstat(file); assert(stat.isFile() && (stat.mode & 0o077) === 0, "EXECUTIVE_WORK_HOST_LEASE_PRIVATE_REQUIRED");
  const lease = JSON.parse(await readFile(file, "utf8")) as ExecutiveWorkHostLease;
  assert(lease.schema === "erpx-owned-executive-work-host-browser/v1" && /^[a-f0-9]{12}$/.test(lease.run) && dirname(file).endsWith(`/${lease.run}`) && lease.port === 3102 && lease.upstream === "http://127.0.0.1:3104" &&
    JSON.stringify(lease.origins) === JSON.stringify(executiveWorkOrigins) && lease.workOnly === true && lease.accountOnlyForSso === true && lease.commercialMfa === false && lease.commercialApply === false && lease.packageEnforcement === false && lease.installedWrites === false && lease.businessWrites === false &&
    JSON.stringify(lease.mcp.methods) === JSON.stringify(methods) && JSON.stringify(lease.mcp.tools) === JSON.stringify(tools) && lease.mcp.companyScope === true && lease.mcp.credentialBound === true && lease.mcp.businessReadOnly === true && validBackgroundTraffic(lease.backgroundTraffic) &&
    lease.denialProbe.method === "POST" && lease.denialProbe.pathSha256 === hash("/api/v1/holdings/acme/executive/connections") && lease.denialProbe.status === 403 && lease.denialProbe.count === 1 && sha(lease.sourceFingerprint) && sha(lease.verificationFingerprint) && sha(lease.release.fileManifestSha256) && sha(lease.ca.sha256) && sha(lease.certificate.sha256) && sha(lease.spki), "EXECUTIVE_WORK_HOST_LEASE_SCOPE_REQUIRED");
  process.kill(lease.pid, 0);
  for (const binding of [lease.ca, lease.certificate]) {
    assert(binding.path.startsWith(file.slice(0, file.lastIndexOf("/")) + "/"), "EXECUTIVE_WORK_HOST_TLS_PATH_REQUIRED");
    const actual = await stablePrivateFile(root, binding.path, dirname(file)); assert(actual.binding.sha256 === binding.sha256 && actual.binding.bytes === binding.bytes, "EXECUTIVE_WORK_HOST_TLS_BINDING_CHANGED");
  }
  const cert = new X509Certificate(await readFile(lease.certificate.path));
  assert(cert.ca && hash(cert.publicKey.export({ format: "der", type: "spki" })) === lease.spki && executiveWorkOrigins.every(origin => cert.checkHost(new URL(origin).hostname) === new URL(origin).hostname), "EXECUTIVE_WORK_HOST_TLS_IDENTITY_REQUIRED");
  return lease;
}

async function waitMessage(child: ChildProcess, kind: string) {
  return new Promise<Record<string, unknown>>((yes, no) => {
    const timer = setTimeout(() => finish(Error("EXECUTIVE_WORK_EDGE_PROTOCOL_TIMEOUT")), 60_000);
    const message = (value: unknown) => { if (!isRecord(value)) return; if (value.kind === "error") finish(Error(String(value.code ?? "EXECUTIVE_WORK_EDGE_FAILED"))); else if (value.kind === kind) { clearTimeout(timer); clean(); yes(value); } };
    const exit = () => finish(Error("EXECUTIVE_WORK_EDGE_EXITED")); const error = (reason: Error) => finish(reason);
    function clean() { child.off("message", message); child.off("exit", exit); child.off("error", error); }
    function finish(reason: Error) { clearTimeout(timer); clean(); no(reason); }
    child.on("message", message); child.once("exit", exit); child.once("error", error);
  });
}

export async function startExecutiveWorkHostEdge(root: string, input: { run: string; output: string; runtimeFingerprint: string; verificationFingerprint: string; releaseDirectory: string; nodePath: string; chromiumPath: string }) {
  const inputPath = join(input.output, "executive-work-edge-input.json"), bytes = JSON.stringify({ ...input, root }) + "\n";
  await writeFile(inputPath, bytes, { flag: "wx", mode: 0o600 });
  const child = spawn(input.nodePath, [join(root, "scripts/testing/executive-work-host-cohort.ts"), "--owned-executive-work-host-edge", inputPath, hash(bytes)], {
    cwd: root, env: { NODE_ENV: "production", PATH: `${dirname(input.nodePath)}:/opt/homebrew/bin:/usr/bin:/bin`, LANG: "en_US.UTF-8", TZ: "Asia/Bangkok" }, stdio: ["ignore", "ignore", "ignore", "ipc"],
  });
  const exit = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>(yes => child.once("exit", (code, signal) => yes({ code, signal })));
  try {
    await waitMessage(child, "ready");
    const path = join(input.output, "executive-work-host-lease.json"), lease = await readExecutiveWorkHostLease(root, path);
    assert(child.pid === lease.pid && lease.run === input.run && lease.sourceFingerprint === input.runtimeFingerprint && lease.verificationFingerprint === input.verificationFingerprint, "EXECUTIVE_WORK_EDGE_CHILD_IDENTITY_REQUIRED");
    let closed = false;
    return { path, lease, async stable() { assert(!closed && child.exitCode === null && child.signalCode === null, "EXECUTIVE_WORK_EDGE_CHILD_CLOSED"); const pending = waitMessage(child, "stable"); child.send({ kind: "check" }); await pending; await readExecutiveWorkHostLease(root, path); }, async close() {
      if (closed) return; closed = true; let failure: unknown;
      if (child.exitCode === null && child.signalCode === null) { const pending = waitMessage(child, "stopped"); child.send({ kind: "stop" }); try { await pending; } catch (error) { failure = error; } }
      const result = await exit; const { unlink } = await import("node:fs/promises"); await unlink(inputPath).catch(() => {});
      if (failure) throw failure; if (result.code !== 0 || result.signal !== null) throw Error("EXECUTIVE_WORK_EDGE_CLEANUP_FAILED");
    } };
  } catch (error) {
    if (child.pid && child.exitCode === null && child.signalCode === null) { child.kill("SIGTERM"); await exit; }
    const { unlink } = await import("node:fs/promises"); await unlink(inputPath).catch(() => {}); throw error;
  }
}

async function runOwnedEdge() {
  let edge: Awaited<ReturnType<typeof listen>> | undefined;
  try {
    assert(process.version === "v26.11.1" && process.env.NODE_ENV === "production" && process.argv.length === 5 && process.send, "PINNED_EXECUTIVE_WORK_EDGE_REQUIRED");
    const inputPath = process.argv[3], bytes = await readFile(inputPath); assert(hash(bytes) === process.argv[4], "EXECUTIVE_WORK_EDGE_INPUT_CHANGED");
    const descriptor = JSON.parse(bytes.toString()) as { root: string; run: string; output: string; runtimeFingerprint: string; verificationFingerprint: string; releaseDirectory: string; nodePath: string; chromiumPath: string };
    assert(resolve(process.argv[1]) === join(descriptor.root, "scripts/testing/executive-work-host-cohort.ts") && inputPath === join(descriptor.output, "executive-work-edge-input.json") && (await lstat(inputPath)).mode % 0o1000 === 0o600, "EXECUTIVE_WORK_EDGE_ENTRY_REQUIRED");
    const credentialFile = join(descriptor.root, ".runtime/browser", descriptor.run, "executive-work-credentials.json"), credential = JSON.parse((await stablePrivateFile(descriptor.root, credentialFile, join(descriptor.root, ".runtime/browser", descriptor.run))).bytes.toString());
    const expected = credentialFrom(credential); assert(expected.run === descriptor.run, "EXECUTIVE_WORK_EDGE_CREDENTIAL_RUN_REQUIRED");
    edge = await listen(descriptor.root, descriptor, expected);
    const finish = (message: Record<string, unknown>, code: number) => { if (!process.connected || !process.send) { process.exit(code); return; } process.send(message, error => { process.disconnect?.(); process.exit(error ? 1 : code); }); };
    process.on("disconnect", () => { void stop(); }); process.on("SIGTERM", () => { void stop(); });
    let stopping = false;
    const stop = async () => { if (stopping) return; stopping = true; try { await edge!.close(); finish({ kind: "stopped" }, 0); } catch { finish({ kind: "error", code: "EXECUTIVE_WORK_EDGE_CLEANUP_FAILED" }, 1); } };
    process.on("message", (value: unknown) => { if (!isRecord(value)) return; if (value.kind === "stop") { void stop(); return; } if (value.kind === "check") void edge!.stable().then(() => process.send?.({ kind: "stable" }), (error: unknown) => process.send?.({ kind: "error", code: error instanceof Error && error.message.startsWith("EXECUTIVE_WORK_EDGE_STABILITY_FAILED:{") ? error.message : "EXECUTIVE_WORK_EDGE_STABILITY_FAILED" })); });
    process.send?.({ kind: "ready" });
  } catch {
    process.exitCode = 1; try { if (edge) await edge.close(); } catch { /* keep original failure */ }
    process.send?.({ kind: "error", code: "EXECUTIVE_WORK_EDGE_START_FAILED" }); if (process.connected) process.disconnect?.();
  }
}

if (process.argv[2] === "--owned-executive-work-host-edge") void runOwnedEdge();
