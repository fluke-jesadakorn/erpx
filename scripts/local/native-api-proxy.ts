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

export function nativeApiPath(environment: NodeJS.ProcessEnv, url: string, host?: string): boolean {
  const path = url.split("?")[0];
  return (environment.ERPX_DEV_NATIVE_PATHS ?? "").split(",").filter(Boolean).some(entry => {
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

export async function nativeSessionContext(base: URL, req: IncomingMessage): Promise<Record<string, string>> {
  const slug = /^\/api\/v1\/holdings\/([^/]+)\//.exec(req.url ?? "")?.[1];
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
      response.once("end", () => response.statusCode === 200 ? done(text) : reject(new Error("NATIVE_CONTEXT_UNAVAILABLE")));
      response.once("error", reject);
    });
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

export async function proxyNativeApi(options: { base: URL; environment?: NodeJS.ProcessEnv; req: IncomingMessage; res: ServerResponse }): Promise<void> {
  const { base, environment, req, res } = options;
  const context = await nativeSessionContext(base, req).catch(() => ({}));
  await new Promise<void>(done => {
    const upstream = httpRequest({
      host: base.hostname,
      port: Number(base.port || 80),
      method: req.method,
      path: nativeApiRewrite(environment ?? process.env, req.url ?? "/"),
      headers: { ...stripInternalHeaders(req.headers), ...context },
    }, response => {
      res.writeHead(response.statusCode ?? 502, response.headers);
      response.pipe(res);
      response.once("end", done); response.once("error", () => { res.destroy(); done(); });
    });
    upstream.once("error", () => { if (!res.headersSent) res.writeHead(502, { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "private, no-store" }); res.end("Bad Gateway\n"); done(); });
    req.pipe(upstream);
  });
}
