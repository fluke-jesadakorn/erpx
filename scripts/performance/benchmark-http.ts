import { chmod, lstat, mkdir, open, readFile, realpath } from "node:fs/promises";
import { constants } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";

const publicOrigin = "https://console-dev-erpx.jesadakorn.com";
const allowedOrigins = new Set([publicOrigin, "http://127.0.0.1:3100", "http://localhost:3100"]);
const options = new Map<string, string>();
const validOptions = ["--origin", "--origins", "--credentials-file", "--cookie-file", "--save-cookie-file", "--rounds", "--label", "--warmup-rounds", "--timeout-ms"];
for (let index = 2; index < process.argv.length; index++) {
  const key = process.argv[index];
  if (key === "--help") {
    console.log("bun scripts/performance/benchmark-http.ts [--origin DEV_ORIGIN | --origins DEV_ORIGIN,DEV_ORIGIN] [--credentials-file .scratch/dev/private.json | --cookie-file .scratch/dev/session.cookie] [--save-cookie-file .scratch/dev/session.cookie] [--rounds 20] [--label warm] [--warmup-rounds 0] [--timeout-ms 30000]");
    process.exit(0);
  }
  if (!validOptions.includes(key) || !process.argv[index + 1] || process.argv[index + 1].startsWith("--")) throw new Error("INVALID_HTTP_BENCHMARK_OPTION");
  options.set(key, process.argv[++index]);
}
if (Bun.version !== "1.4.3") throw new Error("BUN_1_4_3_REQUIRED");
const node = resolve(process.env.ERPX_NODE_BIN ?? ".runtime/node/node-v26.11.1-darwin-arm64/bin/node");
if (Bun.spawnSync([node, "--version"]).stdout.toString().trim() !== "v26.11.1") throw new Error("NODE_26_11_1_REQUIRED");
if (process.env.NODE_ENV === "production" || process.env.ERPX_ENVIRONMENT === "staging") throw new Error("DEVELOPMENT_ENVIRONMENT_REQUIRED");
if (options.has("--origin") && options.has("--origins")) throw new Error("CHOOSE_ONE_ORIGIN_OPTION");
const origins = (options.get("--origin") ?? options.get("--origins") ?? `http://127.0.0.1:3100,${publicOrigin}`)
  .split(",").map(origin => origin.replace(/\/$/, ""));
if (origins.some(origin => !allowedOrigins.has(origin))) throw new Error("DEVELOPMENT_ORIGIN_REQUIRED");
function count(key: string, fallback: number, minimum = 1, maximum = 1000) {
  const value = Number(options.get(key) ?? fallback);
  if (!Number.isInteger(value) || value < minimum || value > maximum) throw new Error("INVALID_HTTP_BENCHMARK_COUNT");
  return value;
}
const rounds = count("--rounds", 20), warmupRounds = count("--warmup-rounds", 0, 0, 20);
const timeoutMs = count("--timeout-ms", 30_000, 100, 180_000);
const label = options.get("--label") ?? "warm";
if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,59}$/.test(label)) throw new Error("INVALID_HTTP_BENCHMARK_LABEL");
if (options.has("--credentials-file") && options.has("--cookie-file")) throw new Error("CHOOSE_ONE_AUTHENTICATION_SOURCE");
const privateRoot = resolve(".scratch/dev");
await mkdir(privateRoot, { recursive: true });
if (await realpath(privateRoot) !== privateRoot) throw new Error("PRIVATE_DIRECTORY_SYMLINK_FORBIDDEN");
async function privateFile(input: string, exists: boolean) {
  const path = resolve(input), inside = relative(privateRoot, path);
  if (!inside || inside.startsWith("..") || inside.startsWith("/")) throw new Error("PRIVATE_DEV_FILE_REQUIRED");
  if (exists) {
    const info = await lstat(path);
    if (!info.isFile() || info.isSymbolicLink() || await realpath(path) !== path) throw new Error("PRIVATE_DEV_FILE_REQUIRED");
  } else {
    await mkdir(dirname(path), { recursive: true, mode: 0o700 });
    if (await realpath(dirname(path)) !== dirname(path)) throw new Error("PRIVATE_DIRECTORY_SYMLINK_FORBIDDEN");
  }
  return path;
}
async function privateWrite(path: string, contents: string) {
  const file = await open(path, constants.O_WRONLY | constants.O_CREAT | constants.O_TRUNC | constants.O_NOFOLLOW, 0o600);
  try { await file.chmod(0o600); await file.writeFile(contents); } finally { await file.close(); }
}
const cookies = new Map<string, string>();
function receiveCookies(headers: Headers) {
  for (const header of headers.getSetCookie()) {
    const [pair, ...attributes] = header.split(";"), separator = pair.indexOf("=");
    if (separator <= 0) continue;
    const name = pair.slice(0, separator).trim(), value = pair.slice(separator + 1).trim();
    const expired = attributes.some(attribute => {
      const [key, ...parts] = attribute.trim().split("="), setting = parts.join("=");
      return key.toLowerCase() === "max-age" && Number(setting) <= 0
        || key.toLowerCase() === "expires" && Date.parse(setting) <= Date.now();
    });
    if (!value || expired) cookies.delete(name); else cookies.set(name, value);
  }
}
function cookieHeader() { return [...cookies].map(([name, value]) => `${name}=${value}`).join("; "); }
const base = "/holdings/acme", api = `/api/v1${base}`;
const targets = [
  { name: "work-page", path: `${base}/work`, api: false },
  { name: "people-page", path: `${base}/people`, api: false },
  { name: "pms-page", path: `${base}/pms`, api: false },
  { name: "pms-settings-page", path: `${base}/pms/settings`, api: false },
  { name: "work-page-api", path: `${api}/work/summary?paged=1`, api: true },
  { name: "people-directory-api", path: `${api}/people-public-directory`, api: true },
  { name: "pms-dashboard-api", path: `${api}/pms?view=dashboard`, api: true },
  { name: "pms-default-api", path: `${api}/pms`, api: true },
];
const directory = join(privateRoot, `http-benchmark-${new Date().toISOString().replace(/[:.]/g, "-")}`);
await mkdir(directory, { mode: 0o700 });
const samples: Record<string, unknown>[] = [];
const evidence = {
  startedAt: new Date().toISOString(), label, rounds, warmupRounds, timeoutMs, origins,
  runtime: { bun: Bun.version, requiredNode: "26.11.1" }, targets, samples,
  authentication: { supplied: options.has("--credentials-file") || options.has("--cookie-file"), signedIn: false, ownerSelected: false, failure: null as string | null },
  limitations: ["Complete-body HTTP requests measure documents and APIs, not browser navigation or hydration.", "The caller labels the run; this helper does not stop servers, clear output, or establish compiler coldness."],
};
const evidencePath = join(directory, "evidence.json");
async function save() { await privateWrite(evidencePath, JSON.stringify(evidence, null, 2) + "\n"); }
async function authenticatedRequest(path: string, method = "GET", body?: unknown) {
  const response = await fetch(publicOrigin + path, { method, redirect: "manual", signal: AbortSignal.timeout(timeoutMs),
    headers: { Origin: publicOrigin, Cookie: cookieHeader(), ...(body ? { "Content-Type": "application/json" } : {}) },
    body: body ? JSON.stringify(body) : undefined });
  receiveCookies(response.headers);
  return response;
}
try {
  const suppliedCookie = options.get("--cookie-file");
  if (suppliedCookie) {
    const header = (await readFile(await privateFile(suppliedCookie, true), "utf8")).trim();
    if (!header || /[\r\n]/.test(header)) throw new Error("COOKIE_HEADER_FILE_REQUIRED");
    for (const pair of header.split(";")) {
      const separator = pair.indexOf("=");
      if (separator <= 0) throw new Error("COOKIE_HEADER_FILE_REQUIRED");
      cookies.set(pair.slice(0, separator).trim(), pair.slice(separator + 1).trim());
    }
  }
  const credentialsFile = options.get("--credentials-file");
  if (credentialsFile) {
    const credentials = JSON.parse(await readFile(await privateFile(credentialsFile, true), "utf8")) as { email?: unknown; password?: unknown };
    if (typeof credentials.email !== "string" || typeof credentials.password !== "string" || !credentials.email || !credentials.password) throw new Error("CREDENTIALS_FILE_INVALID");
    const response = await authenticatedRequest("/api/auth/sign-in/email", "POST", { email: credentials.email, password: credentials.password });
    // Consume the session response without logging, parsing, or persisting it.
    await response.arrayBuffer();
    if (response.status !== 200 || !cookies.size) throw new Error(`SIGN_IN_HTTP_${response.status}`);
    credentials.email = undefined; credentials.password = undefined;
    evidence.authentication.signedIn = true;
  }
  if (cookies.size) {
    const context = await authenticatedRequest(`/api/v1/account/work-context?from=${encodeURIComponent(`${base}/work`)}&companyId=acme-co`);
    if (context.status !== 200) { await context.arrayBuffer(); throw new Error(`COMPANY_CONTEXT_HTTP_${context.status}`); }
    const workspace = await context.json() as { data: { current?: { companyId?: string } } };
    if (workspace.data.current?.companyId !== "acme-co") throw new Error("REQUESTED_COMPANY_CONTEXT_REQUIRED");
    const response = await authenticatedRequest(`${api}/assignments/active`);
    if (response.status !== 200) { await response.arrayBuffer(); throw new Error(`ASSIGNMENTS_HTTP_${response.status}`); }
    const { data } = await response.json() as { data: { valid: boolean; revision: number; assignmentId: string | null; options: { id: string; role: string; scopeKind: string }[] } };
    const owner = data.options.find(option => option.role === "holding_owner" && option.scopeKind === "holding");
    if (!owner) throw new Error("EXISTING_HOLDING_OWNER_ASSIGNMENT_REQUIRED");
    if (!data.valid || data.assignmentId !== owner.id) {
      const selected = await authenticatedRequest(`${api}/assignments/active`, "POST", { assignmentId: owner.id, revision: data.revision });
      await selected.arrayBuffer();
      if (selected.status !== 200) throw new Error(`ASSIGNMENT_SELECTION_HTTP_${selected.status}`);
    }
    evidence.authentication.signedIn = true;
    evidence.authentication.ownerSelected = true;
    const output = options.get("--save-cookie-file");
    if (output) await privateWrite(await privateFile(output, false), cookieHeader() + "\n");
  } else if (options.has("--save-cookie-file")) throw new Error("AUTHENTICATED_SESSION_REQUIRED");
} catch (error) {
  const code = error instanceof Error && /^[A-Z][A-Z0-9_]+$/.test(error.message) ? error.message : "AUTHENTICATION_FAILED";
  evidence.authentication.failure = code;
  await save();
  console.error(JSON.stringify({ event: "http_benchmark_authentication_failed", code, evidence: evidencePath }));
  process.exit(1);
}
async function sample(origin: string, target: typeof targets[number], round: number, measured: boolean) {
  const started = performance.now();
  let status: number | null = null, headerMs: number | null = null;
  try {
    const response = await fetch(origin + target.path, { redirect: "manual", signal: AbortSignal.timeout(timeoutMs), headers: { Cookie: cookieHeader() } });
    headerMs = performance.now() - started; status = response.status; receiveCookies(response.headers);
    const body = await response.arrayBuffer(), location = response.headers.get("location");
    let validApi = true, streamingRedirect = false, rscFallbackError = false;
    if (target.api && status === 200) {
      try { const data = JSON.parse(new TextDecoder().decode(body)); validApi = !!data && "data" in data && !("error" in data); } catch { validApi = false; }
    } else if (!target.api) {
      const html = new TextDecoder().decode(body);
      // Next can flush an HTTP 200 shell before redirecting inside its stream.
      streamingRedirect = html.includes("NEXT_REDIRECT;")
        || /<meta\b[^>]*\bhttp-equiv\s*=\s*(?:"refresh"|'refresh'|refresh)(?=[\s/>])/i.test(html);
      const errorDocument = /<html\b[^>]*\bid\s*=\s*(?:"__next_error__"|'__next_error__')/i.test(html);
      const fallbackDigests = [...html.matchAll(/<template\b[^>]*\bdata-dgst\s*=\s*(["'])(.*?)\1/gi)];
      // CSR bailouts are intentional. Other error templates indicate a server
      // fallback; classification retains no error text, digest or redirect URL.
      rscFallbackError = errorDocument || fallbackDigests.some(match =>
        !match[2].startsWith("NEXT_REDIRECT") && match[2] !== "BAILOUT_TO_CLIENT_SIDE_RENDERING");
    }
    const redirect = location ? new URL(location, origin) : null;
    return { label, round, measured, origin, name: target.name, path: target.path, status, bytes: body.byteLength,
      durationMs: performance.now() - started, headerMs, serverTiming: response.headers.get("server-timing"),
      redirectPath: redirect ? allowedOrigins.has(redirect.origin) ? redirect.pathname : "external" : null,
      streamingRedirect, rscFallbackError,
      success: status === 200 && validApi && !streamingRedirect && !rscFallbackError,
      error: streamingRedirect ? "StreamedRedirect" : rscFallbackError ? "ServerComponentFallback" : null };
  } catch (error) {
    return { label, round, measured, origin, name: target.name, path: target.path, status, bytes: null,
      durationMs: performance.now() - started, headerMs, serverTiming: null, redirectPath: null,
      streamingRedirect: null, rscFallbackError: null,
      success: false, error: error instanceof Error ? error.name : "RequestError" };
  }
}
for (let round = 1; round <= warmupRounds + rounds; round++) {
  const measured = round > warmupRounds;
  for (const target of targets) for (const origin of origins) samples.push(await sample(origin, target, measured ? round - warmupRounds : round, measured));
  await save();
  console.log(JSON.stringify({ event: "http_benchmark_round", label, round: measured ? round - warmupRounds : round, measured }));
}
const measuredSamples = samples.filter(sample => sample.measured);
const failed = measuredSamples.filter(sample => !sample.success).length;
if (failed) process.exitCode = 1;
const savedCookie = options.get("--save-cookie-file");
if (savedCookie && cookies.size) await privateWrite(await privateFile(savedCookie, false), cookieHeader() + "\n");
await chmod(evidencePath, 0o600);
console.log(JSON.stringify({ event: "http_benchmark_complete", label, measuredSamples: measuredSamples.length, failed, evidence: evidencePath }));
