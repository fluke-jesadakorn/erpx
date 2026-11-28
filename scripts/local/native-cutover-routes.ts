import type { ServerResponse } from "node:http";
import { oasisDemoAccountHost, oasisDemoHolding } from '@erpx/platform-contracts/holding-hosts';

function validId(segment: string): boolean {
  const id = decodeURIComponent(segment);
  return !!id && id !== "." && id !== ".." && !/[/\\\x00-\x1f\x7f]/.test(id);
}

export type HoldingNativeWorkspace = { branch?: string; department?: string };
export type ShellPreference = 'time' | 'workspace';

export function shellPreferenceRoute(url: string): { kind: ShellPreference; url: string; app?: string } | undefined {
  const prefixed = /^\/__erpx\/([a-z0-9-]+)(\/api\/.*)$/.exec(url);
  const candidate = prefixed?.[2] ?? url;
  const path = candidate.split('?', 1)[0];
  if (path === '/api/v1/account/time-preference') return {kind:'time', url:candidate, app:prefixed?.[1]};
  if (path === '/api/v1/workspace-preference') return {kind:'workspace', url:candidate, app:prefixed?.[1]};
}

export function respondToShellPreferenceMethod(
  response: ServerResponse,
  method: string | undefined,
  kind: ShellPreference,
  readOnly: boolean,
): boolean {
  const allowed = kind === 'workspace' ? ['POST'] : readOnly ? ['GET', 'HEAD'] : ['GET', 'HEAD', 'PATCH'];
  if (method && allowed.includes(method)) return false;
  response.writeHead(method === 'OPTIONS' ? 204 : 405, {
    Allow: [...allowed, 'OPTIONS'].join(', '), 'Cache-Control': 'private, no-store',
  });
  response.end();
  return true;
}

const read = ['GET', 'HEAD'];
const write = ['POST'];
const readWrite = [...read, ...write];
const timePreferenceMethods = [...read, 'PATCH'];
const oasisAccountRoutes: Readonly<Record<string, readonly string[]>> = {
  '/api/auth/get-session': read,
  '/api/auth/sign-in/email': write,
  '/api/auth/sign-out': write,
  '/api/v1/account/device-accounts': [...read, 'DELETE'],
  '/api/v1/account/device-accounts/initialize': write,
  '/api/v1/account/device-accounts/active': write,
  '/api/v1/account/device-accounts/sign-in': write,
  '/api/v1/account/time-preference': timePreferenceMethods,
  '/sso/authorize': ['GET'],
};
const oasisHoldingRoutes: Readonly<Record<string, readonly string[]>> = {
  '/api/auth/get-session': read,
  '/api/auth/sign-out': write,
  '/api/v1/account/time-preference': timePreferenceMethods,
  '/sso/start': ['GET'],
  '/sso/callback': ['GET'],
};
const oasisSalesRoutes: Readonly<Record<string, readonly string[]>> = {
  'assignments/active': readWrite,
  'companies/active': write,
  'crm/activities': readWrite,
  'crm/activities/work': read,
  'crm/leads': read,
  'commerce/orders': write,
  'commerce/pending-quotations': read,
  'reports/sales-crm': read,
  'feed': readWrite,
  'plugins': readWrite,
};

/** Public methods of the walkthrough. Private SSR transport is never exposed here. */
export function oasisDemoNativeRoute(host: string, url: string): { url: string; methods: readonly string[]; workspace?: HoldingNativeWorkspace; workContext?: true; workspacePreference?: true } | undefined {
  if (host !== oasisDemoAccountHost && host !== oasisDemoHolding.hostname) return;
  const [path] = url.split('?', 1);
  if (path === '/api/health' || path === '/api/ready') return {url, methods: read};
  const methods = (host === oasisDemoAccountHost ? oasisAccountRoutes : oasisHoldingRoutes)[path!];
  if (methods) return {url, methods};
  if (host !== oasisDemoHolding.hostname) return;
  if (path === '/api/v1/workspace-preference') return {
    url: `/api/v1/holdings/${oasisDemoHolding.holdingSlug}/workspace-preference`,
    methods: write,
    workspacePreference: true,
  };
  const scoped = normalizeHoldingNativePath(url, oasisDemoHolding.holdingSlug);
  const canonical = scoped.url.split('?', 1)[0]!;
  const prefix = `/api/v1/holdings/${oasisDemoHolding.holdingSlug}/`;
  if (!canonical.startsWith(prefix)) return;
  const route = canonical.slice(prefix.length);
  if (Object.hasOwn(oasisSalesRoutes, route)) return {...scoped, methods: oasisSalesRoutes[route]!,
    ...(route === 'assignments/active' || route === 'companies/active' ? {} : {workContext:true})};
  const conversation = /^posts\/([^/]+)\/comments$/.exec(route);
  if (conversation) {
    try { if (!validId(conversation[1]!)) return; } catch { return; }
    return {...scoped, methods: readWrite, workContext:true};
  }
  const pricing = /^commerce\/pricing(?:\/([^/]+)(?:\/edit)?)?$/.exec(route);
  if (pricing) {
    try { if (pricing[1] && !validId(pricing[1])) return; } catch { return; }
    const suffix = route.slice("commerce/pricing".length);
    const query = scoped.url.slice(canonical.length);
    return {...scoped, url: `${prefix}sales/pricing${suffix}${query}`, methods: write, workContext:true};
  }
  const match = /^(crm\/activities|crm\/leads|commerce\/orders)\/([^/]+)(?:\/(changes|opportunity\/changes|transition))?$/.exec(route);
  if (!match) return;
  const [, owner, id, action] = match;
  try { if (!validId(id!)) return; } catch { return; }
  if (owner === 'commerce/orders' && !action) return {...scoped, methods: readWrite, workContext:true};
  if (owner === 'commerce/orders' && action === 'transition' ||
      owner === 'crm/activities' && action === 'changes' ||
      owner === 'crm/leads' && (action === 'changes' || action === 'opportunity/changes')) return {...scoped, methods: write, workContext:true};
}

function canonicalQualityAction(url: string, prefix: string): string {
  const match = /^quality\/([^/?]+)(?:\/([^/?]+))?(\?.*)?$/.exec(url.slice(prefix.length));
  if (!match) return url;
  try {
    const segment = decodeURIComponent(match[1]!);
    if (match[1] !== segment && (segment === "specifications" || !match[2] && ["alerts", "read-plan"].includes(segment)))
      return `${prefix}quality/${segment}${match[2] ? `/${match[2]}` : ""}${match[3] ?? ""}`;
    if (!match[2]?.includes("%")) return url;
    const action = decodeURIComponent(match[2]);
    if (action === "cancellations" || ["acknowledge", "cancel", "inspect", "release", "disposition"].includes(action))
      return `${prefix}quality/${match[1]}/${action}${match[3] ?? ""}`;
  } catch {
    return url;
  }
  return url;
}

export function normalizeHoldingNativePath(url: string, holdingSlug: string): { url: string; workspace?: HoldingNativeWorkspace } {
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!url.startsWith(prefix)) return { url };
  const scoped = /^(?:branches\/([a-z0-9_-]{1,128})\/(?:departments\/([a-z0-9_-]{1,128})\/)?|departments\/([a-z0-9_-]{1,128})\/)(.+)$/.exec(url.slice(prefix.length));
  if (!scoped) return { url: canonicalQualityAction(url, prefix) };
  const [, branch, nestedDepartment, department, path] = scoped;
  return { url: canonicalQualityAction(prefix + path, prefix), workspace: { branch, department: nestedDepartment ?? department } };
}

export function isHoldingQualityPath(url: string, holdingSlug: string): boolean {
  const path = url.split("?", 1)[0];
  const root = `/api/v1/holdings/${holdingSlug}/quality`;
  return path === root || path?.startsWith(root + "/") === true;
}

/** Preserve the former catch-all's automatic HTTP method responses. */
export function respondToHoldingQualityMethod(res: ServerResponse, method: string | undefined): boolean {
  if (method === "GET" || method === "HEAD" || method === "POST") return false;
  if (method === "OPTIONS") res.writeHead(204, { Allow: "GET, HEAD, OPTIONS, POST" });
  else res.writeHead(405);
  res.end();
  return true;
}

/** Native Holding operations and paths reserved for private SSR. */
export function holdingNativeCutoverPath(url: string, holdingSlug: string, method: string | undefined): "records" | "record-action" | "routine-templates" | "routine-schedules" | "routine-command" | "native-private-read" | "quality-read" | "quality-command" | "quality-missing" | "plugins" | "social" | "navigation-command" | "quotation-summary" | undefined {
  const [path] = url.split("?", 1);
  const prefix = `/api/v1/holdings/${holdingSlug}/`;
  if (!path?.startsWith(prefix)) return;
  const [owner, segment, ...tail] = path.slice(prefix.length).split("/");
  if (owner === "commerce" && segment === "pending-quotations" && tail.length === 0 && ["GET", "HEAD"].includes(method ?? "")) return "quotation-summary";
  if (owner === "navigation" && segment === undefined) {
    if (method === "POST") return "navigation-command";
    // Native navigation GET still hardcodes expense_available=true and
    // routing_attention=false, and cutover.work_projection.feed-navigation-runtime
    // explicitly keeps rich (?include=attention) GET outside the public proxy
    // until those owner facts reach parity. A compact read would publish the
    // same unresolved work count and routing flag, so quarantine GET/HEAD too.
    if (method === "GET" || method === "HEAD") return "native-private-read";
  }
  if (owner === "plugins" && segment === undefined) return "plugins";
  if (owner === "feed" && segment === undefined) return "social";
  if (owner === "posts" && segment && tail.length === 1 && tail[0] === "comments") {
    try { if (validId(segment)) return "social"; } catch { return; }
  }
  if (owner === "quality" && method === "POST" && segment === "specifications" && tail.length <= 1) {
    try {
      if (tail.length === 0 || validId(tail[0]!)) return "quality-command";
    } catch {
      return;
    }
  }
  if (owner === "quality" && method === "POST") {
    if (segment === undefined) return "quality-command";
    try {
      if (validId(segment) && tail.length === 1 && ["acknowledge", "cancel", "inspect", "release", "disposition"].includes(tail[0]!)) return "quality-command";
      if (segment === "read-plan" && tail.length === 0) return "quality-missing";
    } catch {
      return;
    }
  }
  if (owner === "quality" && (method === "GET" || method === "HEAD")) {
    if (segment === undefined) return "quality-read";
    try {
      if (!validId(segment)) return;
      if (tail.length === 1 && tail[0] === "cancellations") return "quality-read";
      if (tail.length === 0) return "quality-read";
      if (tail.every(validId)) return "quality-missing";
    } catch {
      return;
    }
  }
  if (owner === "routine-template-editor-context" && segment === undefined) return "native-private-read";
  if (owner === "sales" && segment === "price-list-workspace" && tail.length === 0) return "native-private-read";
  if (owner === "people-payroll" && segment === "leave-policy-history" && tail.length === 0) return "native-private-read";
  if (owner === "routine-templates" || owner === "routine-schedules") {
    try {
      if (segment !== undefined && !validId(segment)) return;
      if (!tail.every(validId)) return;
      return method === "POST" ? "routine-command" : owner;
    } catch {
      return;
    }
  }
  if (owner !== "records" || !segment) return;
  try {
    const module = decodeURIComponent(segment);
    if (!/^[a-z0-9_-]+$/.test(module) || module === "search" || module === "native-search") return;
    if (tail.length === 0) return owner;
    if (!validId(tail[0]!)) return;
    if (tail.length === 1 || tail.length === 2 && ["references", "transition", "mentions", "suggestions"].includes(tail[1]!)) return "record-action";
    if (tail.length === 3 && tail[1] === "suggestions" && validId(tail[2]!)) return "record-action";
  } catch {
    return;
  }
}
