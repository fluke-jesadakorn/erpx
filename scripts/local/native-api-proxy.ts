import { request as httpRequest, type IncomingHttpHeaders, type IncomingMessage, type ServerResponse } from "node:http";

/** Development-only direct transport to the native owner service. No business
 * logic: the gateway preserves Host/cookies and streams the request. */
export function nativeApiBase(environment: NodeJS.ProcessEnv): URL | undefined {
  const raw = environment.ERPX_DEV_NATIVE_API;
  if (raw === undefined || raw === "") return undefined;
  let base: URL;
  try { base = new URL(raw); } catch { throw new Error("INVALID_DEV_NATIVE_API"); }
  if (base.protocol !== "http:" || !["127.0.0.1", "localhost"].includes(base.hostname) || base.pathname !== "/" || base.search !== "") throw new Error("INVALID_DEV_NATIVE_API");
  return base;
}

function matchesNativeTemplate(template: string, path: string): boolean {
  if (!template.startsWith("/api/")) return false;
  const pattern = template.split("/");
  const segments = path.split("/");
  return pattern.length === segments.length && pattern.every((part, index) => {
    const segment = segments[index]!;
    if (!/^\{[A-Za-z_][A-Za-z0-9_]*\}$/.test(part)) return part === segment;
    try {
      const decoded = decodeURIComponent(segment);
      return !!decoded && decoded !== "." && decoded !== ".." && !/[/\\]/.test(decoded);
    } catch { return false; }
  });
}

export function nativeApiPath(environment: NodeJS.ProcessEnv, url: string, host?: string, method?: string): boolean {
  const path = url.split("?")[0];
  return (environment.ERPX_DEV_NATIVE_PATHS ?? "").split(",").filter(Boolean).some(entry => {
    const fields = entry.split("|");
    if (fields.length === 3) {
      const [entryHost, entryMethod, template] = fields;
      return !!entryHost && entryHost === host && !!entryMethod && entryMethod === method && matchesNativeTemplate(template!, path);
    }
    if (fields.length > 3) return false;
    const separator = entry.indexOf("|");
    const entryHost = separator < 0 ? undefined : entry.slice(0, separator);
    const prefix = separator < 0 ? entry : entry.slice(separator + 1);
    if (entryHost !== undefined && entryHost !== host) return false;
    return path === prefix || path.startsWith(prefix.endsWith("/") ? prefix : prefix + "/");
  });
}

export function nativeApiRewrite(environment: NodeJS.ProcessEnv, url: string): string {
  for (const entry of (environment.ERPX_DEV_NATIVE_REWRITES ?? "").split(",").filter(Boolean)) {
    const [from, to] = entry.split("=");
    if (from && to && url.includes(from)) return url.replace(from, to);
  }
  return url;
}

/** The native service trusts only loopback callers and private context headers.
 * Remove every browser-supplied internal header before adding resolved context. */
export function stripInternalHeaders(headers: IncomingHttpHeaders): IncomingHttpHeaders {
  return Object.fromEntries(Object.entries(headers).filter(([key]) =>
    !key.startsWith("x-erpx-") && !key.startsWith("x-forwarded-") && key !== "forwarded"));
}

class NativeSessionError extends Error {
  constructor(readonly status: number, readonly body: string) {
    super("NATIVE_CONTEXT_UNAVAILABLE");
  }
}

export function nativeApiError(res: ServerResponse, status: number, code: string, message: string): void {
  res.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "private, no-store" });
  res.end(JSON.stringify({error: {code, message}}));
}

export async function nativeSessionContext(base: URL, req: IncomingMessage, url = req.url ?? ""): Promise<Record<string, string>> {
  const slug = /^\/api\/v1\/holdings\/([^/]+)\//.exec(url)?.[1];
  if (!slug || !req.headers.cookie) return {};
  const requested = req.headers["x-erpx-requested-company"];
  const target = `/internal/session-context?holding=${encodeURIComponent(slug)}`;
  const body = await new Promise<string>((done, reject) => {
    const call = httpRequest({
      host: base.hostname,
      port: Number(base.port || 80),
      method: "GET",
      path: target,
      headers: {
        host: req.headers.host ?? base.host,
        cookie: req.headers.cookie ?? "",
        // The bridge is loopback-only and rejects browser origins. Host stays
        // the exact Holding host; loopback is the transport proof.
        origin: base.origin,
        accept: "application/json",
        ...(typeof requested === "string" ? { "x-erpx-requested-company": requested } : {}),
      },
    }, response => {
      let text = "";
      response.setEncoding("utf8"); response.on("data", chunk => { text += chunk; });
      response.once("end", () => {
        const status = response.statusCode ?? 503;
        if (status === 200) done(text);
        else if ([401, 403, 404, 409, 422, 503].includes(status) && response.headers["content-type"]?.startsWith("application/json"))
          reject(new NativeSessionError(status, text));
        else reject(new Error("NATIVE_CONTEXT_UNAVAILABLE"));
      });
      response.once("error", reject);
    });
    call.setTimeout(5_000, () => call.destroy(new Error("NATIVE_CONTEXT_UNAVAILABLE")));
    call.once("error", reject); call.end();
  });
  let data: unknown;
  try { data = (JSON.parse(body) as { data?: unknown }).data; } catch { throw new Error("NATIVE_CONTEXT_UNAVAILABLE"); }
  if (!data || typeof data !== "object") throw new Error("NATIVE_CONTEXT_UNAVAILABLE");
  const value = data as Record<string, unknown>;
  const text = (key: string) => typeof value[key] === "string" && value[key] !== "" ? String(value[key]) : undefined;
  const headers: Record<string, string | undefined> = {
    "x-erpx-context-company": text("companyId"),
    "x-erpx-context-user": text("userId"),
    "x-erpx-context-assignment": text("assignmentId"),
    "x-erpx-context-revision": typeof value.revision === "number" ? String(value.revision) : undefined,
    "x-erpx-context-assignment-version": typeof value.assignmentVersion === "number" ? String(value.assignmentVersion) : undefined,
    "x-erpx-context-modules": Array.isArray(value.modules) ? value.modules.filter((item): item is string => typeof item === "string").join(",") : undefined,
    "x-erpx-workspace-branch": text("branchSlug"),
    "x-erpx-workspace-department": text("departmentSlug"),
  };
  return Object.fromEntries(Object.entries(headers).filter(([, item]) => item !== undefined)) as Record<string, string>;
}

export async function proxyNativeApi(options: { base: URL; environment?: NodeJS.ProcessEnv; req: IncomingMessage; res: ServerResponse; url?: string; requiresSession?: boolean }): Promise<void> {
  const { base, environment, req, res, requiresSession } = options;
  const url = options.url ?? req.url ?? "/";
  if (requiresSession && !req.headers.cookie) return nativeApiError(res, 401, "UNAUTHENTICATED", "Sign in to continue.");
  let context: Record<string, string> = {};
  try {
    context = await nativeSessionContext(base, req, url);
  } catch (error) {
    if (requiresSession) {
      if (error instanceof NativeSessionError) {
        res.writeHead(error.status, { "Content-Type": "application/json", "Cache-Control": "private, no-store" });
        res.end(error.body);
        return;
      }
      return nativeApiError(res, 503, "NATIVE_ROUTINES_UNAVAILABLE", "The native Operations routine read is unavailable.");
    }
  }
  await new Promise<void>(done => {
    const upstream = httpRequest({
      host: base.hostname,
      port: Number(base.port || 80),
      method: req.method,
      path: nativeApiRewrite(environment ?? process.env, url),
      headers: { ...stripInternalHeaders(req.headers), ...context, "x-erpx-api-transport": "public" },
    }, response => {
      res.writeHead(response.statusCode ?? 502, response.headers);
      response.pipe(res);
      response.once("end", done); response.once("error", () => { res.destroy(); done(); });
    });
    upstream.once("error", () => { if (!res.headersSent) res.writeHead(502, { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "private, no-store" }); res.end("Bad Gateway\n"); done(); });
    req.pipe(upstream);
  });
}
