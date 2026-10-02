import { NextResponse, type NextRequest } from "next/server";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { frontAppForHost, frontOrigin, frontVisiblePath } from "./lib/front-host";
import { consoleOrigin, isMarketingHost, marketingConsoleUrl, marketingIsIndexable, marketingOrigin } from "./lib/erpx-hosts";

type ParsedScope = {
  holdingSlug: string;
  kind: "holding" | "branch" | "department";
  branchSlug?: string;
  departmentSlug?: string;
  destination: string[];
};

function parse(parts: string[]): ParsedScope | null {
  const holdingSlug = parts[0];
  if (!holdingSlug) return null;
  if (parts[1] === "branches" && parts[2]) {
    if (parts[3] === "departments" && parts[4])
      return {
        holdingSlug,
        kind: "department",
        branchSlug: parts[2],
        departmentSlug: parts[4],
        destination: parts.slice(5),
      };
    return {
      holdingSlug,
      kind: "branch",
      branchSlug: parts[2],
      destination: parts.slice(3),
    };
  }
  if (parts[1] === "departments" && parts[2])
    return {
      holdingSlug,
      kind: "department",
      departmentSlug: parts[2],
      destination: parts.slice(3),
    };
  return {
    holdingSlug,
    kind: "holding",
    destination: parts.slice(1),
  };
}

function requestedPagePath(request: NextRequest) {
  const search = new URLSearchParams(request.nextUrl.search);
  search.delete("_rsc");
  return request.nextUrl.pathname + (search.size ? `?${search}` : "");
}

export function proxy(request: NextRequest) {
  const production = process.env.NODE_ENV === "production";
  const path = request.nextUrl.pathname;
  const host = request.headers.get("host") ?? new URL(request.url).host;
  const frontApp = frontAppForHost(host);
  const erpOrigin = consoleOrigin();
  const marketing = isMarketingHost(host);
  const localHost = /^(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?$/.test(host ?? "");
  const loopbackHealth = localHost && (path === "/api/health" || path === "/api/ready");
  if (!frontApp && !marketing && host !== erpOrigin?.host && !(localHost && (!production || loopbackHealth))) return new NextResponse("Invalid host", { status: 421 });
  if (production) {
    const origin = frontApp ? frontOrigin(frontApp)! : marketing ? marketingOrigin()! : erpOrigin!;
    if (!loopbackHealth && request.headers.get("x-forwarded-proto") === "http") {
      const secure = new URL(origin);
      secure.pathname = path;
      secure.search = request.nextUrl.search;
      return NextResponse.redirect(secure, 308);
    }
    if (process.env.ERPX_DATA_DIR && existsSync(join(process.env.ERPX_DATA_DIR, "maintenance")))
      return new NextResponse("ERPX is undergoing scheduled maintenance.", { status: 503, headers: { "Retry-After": "60", "Cache-Control": "no-store" } });
  }
  if (path === "/robots.txt") {
    const body = marketingIsIndexable(host)
      ? "User-agent: *\nAllow: /\nSitemap: https://erpx.jesadakorn.com/sitemap.xml\n"
      : "User-agent: *\nDisallow: /\n";
    return new NextResponse(body, { headers: { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "public, max-age=300" } });
  }
  if (path === "/sitemap.xml") {
    if (!marketingIsIndexable(host)) return new NextResponse("Not found", { status: 404 });
    return new NextResponse('<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>https://erpx.jesadakorn.com/</loc></url></urlset>', { headers: { "Content-Type": "application/xml; charset=utf-8" } });
  }
  if (marketing) {
    if (path === "/api/health" || path === "/api/ready") return NextResponse.next();
    if (path.startsWith("/_next/")) return NextResponse.next();
    if (path.startsWith("/api/")) return new NextResponse("Not found", { status: 404 });
    if (path === "/") {
      const internal = new URL(request.url);
      internal.pathname = "/marketing";
      const response = NextResponse.rewrite(internal);
      if (!marketingIsIndexable(host)) response.headers.set("X-Robots-Tag", "noindex, nofollow");
      return response;
    }
    if (path === "/favicon.ico" || path === "/icon.svg" || path === "/apple-icon.png" || /^\/icon-(?:192|512|maskable-512)\.png$/.test(path) || path.startsWith("/brand/")) return NextResponse.next();
    if (path === "/manifest.webmanifest") return new NextResponse("Not found", { status: 404 });
    if (path === "/marketing") return new NextResponse("Not found", { status: 404 });
    return NextResponse.redirect(marketingConsoleUrl(path, request.nextUrl.search), 308);
  }
  if (path === "/marketing") return new NextResponse("Not found", { status: 404 });
  if (frontApp) {
    if (path === "/api/health" || path === "/api/ready" || path.startsWith("/_next/") || path === "/fonts/NotoSansThai.ttf") return NextResponse.next();
    if (path === "/erpx-push-sw.js") return NextResponse.next();
    if (path.startsWith("/api/v1/front/")) {
      const parts = path.slice("/api/v1/front/".length).split("/");
      if (parts[2] !== frontApp) return new NextResponse("Not found", { status: 404 });
    } else if (path === "/api/v1/front-access/quick-access") {
      return NextResponse.next();
    } else if (/^\/api\/v1\/holdings\/[^/]+(?:\/branches\/[^/]+(?:\/departments\/[^/]+)?|\/departments\/[^/]+)?\/push$/.test(path)) {
      // Keep the existing scoped Push API on this Front origin; the normal
      // workspace rewrite below supplies its authoritative scope headers.
    } else if (path.startsWith("/api/auth/")) {
      if (!/^\/api\/auth\/(?:sign-in(?:\/email|\/social)?|sign-out|get-session|forget-password|request-password-reset|reset-password|send-verification-email|verify-email|callback\/directory-[a-f0-9-]{36})(?:\/|$)/i.test(path))
        return new NextResponse("Not found", { status: 404 });
    } else if (path === "/" || path === "/login" || path === "/forgot-password" || path === "/verify-email" || path === "/reset-password" || path === "/manifest.webmanifest" || /^\/icon\/(192|512)$/.test(path)) {
      const internal = new URL(request.url);
      internal.pathname = "/front-access" + (path === "/" ? "" : path);
      return NextResponse.rewrite(internal);
    } else if (path === "/favicon.ico") {
      return NextResponse.redirect(new URL("/icon/192", request.url), 307);
    } else if (path.startsWith("/front/")) {
      const parts = path.slice(7).split("/").filter(Boolean);
      if (parts.length < 3 || parts[2] !== frontApp) return new NextResponse("Not found", { status: 404 });
      const destination = new URL(frontVisiblePath(parts[0], parts[1], parts.slice(3)) + request.nextUrl.search, request.url);
      return NextResponse.redirect(destination, 308);
    } else if (path.startsWith("/api/") || path.startsWith("/front-access") || path.startsWith("/holdings") || path.startsWith("/platform")) {
      return new NextResponse("Not found", { status: 404 });
    } else {
      const parts = path.split("/").filter(Boolean);
      if (parts.length < 2 || parts.some((part) => !/^[A-Za-z0-9_-]+$/.test(part))) return new NextResponse("Not found", { status: 404 });
      const internal = new URL(request.url);
      internal.pathname = "/front/" + [parts[0], parts[1], frontApp, ...parts.slice(2)].join("/");
      const headers = new Headers(request.headers);
      headers.set("x-erpx-front-company", parts[1]);
      headers.set("x-erpx-front-app", frontApp);
      return NextResponse.rewrite(internal, { request: { headers } });
    }
  } else if (path.startsWith("/front/") && (production || !!frontOrigin("pms") || !!frontOrigin("pos"))) {
    const parts = path.slice(7).split("/").filter(Boolean);
    const app = parts[2];
    if (parts.length >= 3 && (app === "pms" || app === "pos")) {
      const origin = frontOrigin(app);
      if (origin) return NextResponse.redirect(new URL(frontVisiblePath(parts[0], parts[1], parts.slice(3)) + request.nextUrl.search, origin), 308);
    }
  }
  if (!frontApp && path.startsWith("/front-access")) return new NextResponse("Not found", { status: 404 });
  if (!frontApp && path === "/api/v1/front-access/quick-access") return new NextResponse("Not found", { status: 404 });
  if (!frontApp && path.startsWith("/api/v1/front/") && frontOrigin("pms") && frontOrigin("pos")) return new NextResponse("Not found", { status: 404 });
  if (request.nextUrl.pathname.startsWith("/platform/")) {
    const headers = new Headers(request.headers);
    headers.set("x-erpx-page-path", requestedPagePath(request));
    const response = NextResponse.next({ request: { headers } });
    if (request.method === "GET")
      response.cookies.set("erpx_last_workspace", request.nextUrl.pathname, {
        httpOnly: true,
        sameSite: "lax",
        secure: production || request.nextUrl.protocol === "https:",
        path: "/",
        maxAge: 60 * 60 * 24 * 90,
      });
    return response;
  }
  if (request.nextUrl.pathname.startsWith("/api/v1/platform/")) {
    const headers = new Headers(request.headers);
    headers.set("x-erpx-method", request.method);
    return NextResponse.next({ request: { headers } });
  }
  if (request.nextUrl.pathname.startsWith("/api/v1/front/")) {
    const parts = request.nextUrl.pathname.slice("/api/v1/front/".length).split("/").filter(Boolean);
    const [holdingSlug, companySlug, app, ...rest] = parts;
    if (!holdingSlug || !companySlug || (app !== "pos" && app !== "pms"))
      return new NextResponse("Not found", { status: 404 });
    const requestedLocation = request.headers.get("x-erpx-front-location") ?? "";
    const locationParts = requestedLocation === "" || requestedLocation === "holding"
      ? []
      : requestedLocation.split("/").filter(Boolean);
    if (locationParts.length && !(
      (locationParts[0] === "branches" && locationParts.length === 2) ||
      (locationParts[0] === "departments" && locationParts.length === 2) ||
      (locationParts[0] === "branches" && locationParts.length === 4 && locationParts[2] === "departments")
    )) return new NextResponse("Invalid storefront location", { status: 404 });
    const headers = new Headers(request.headers);
    const parsed = parse([holdingSlug, ...locationParts]);
    if (!parsed) return new NextResponse("Not found", { status: 404 });
    for (const key of ["x-erpx-workspace-kind", "x-erpx-branch-slug", "x-erpx-department-slug", "x-erpx-destination", "x-erpx-api", "x-erpx-method", "x-erpx-front-company", "x-erpx-front-app"])
      headers.delete(key);
    headers.set("x-erpx-front-company", companySlug);
    headers.set("x-erpx-front-app", app);
    headers.set("x-erpx-destination", [app, ...rest].join("/") || app);
    headers.set("x-erpx-api", "true");
    headers.set("x-erpx-method", request.method);
    headers.set("x-erpx-workspace-kind", parsed.kind);
    if (parsed.branchSlug) headers.set("x-erpx-branch-slug", parsed.branchSlug);
    if (parsed.departmentSlug) headers.set("x-erpx-department-slug", parsed.departmentSlug);
    // Keep the Front boundary as its own filesystem route. The route handler
    // applies a daily-command allowlist before delegating to the same domain
    // services used by ERPX; rewriting here would silently re-open all admin
    // POS/PMS commands.
    return NextResponse.next({ request: { headers } });
  }
  if (request.nextUrl.pathname.startsWith("/front/")) {
    const parts = request.nextUrl.pathname.slice("/front/".length).split("/").filter(Boolean);
    const [holdingSlug, companySlug, app] = parts;
    if (holdingSlug && companySlug && (app === "pos" || app === "pms")) {
      const headers = new Headers(request.headers);
      headers.delete("x-erpx-front-company");
      headers.delete("x-erpx-front-app");
      headers.set("x-erpx-front-company", companySlug);
      headers.set("x-erpx-front-app", app);
      return NextResponse.next({ request: { headers } });
    }
    return NextResponse.next();
  }
  const api = request.nextUrl.pathname.startsWith("/api/v1/holdings/");
  if (!api && !request.nextUrl.pathname.startsWith("/holdings/")) {
    const headers = new Headers(request.headers);
    headers.set("x-erpx-page-path", requestedPagePath(request));
    return NextResponse.next({ request: { headers } });
  }
  const prefix = api ? "/api/v1/holdings/" : "/holdings/";
  const parts = request.nextUrl.pathname
    .slice(prefix.length)
    .split("/")
    .filter(Boolean);
  const parsed = parse(parts);
  if (!parsed) return NextResponse.next();

  const headers = new Headers(request.headers);
  headers.set("x-erpx-page-path", requestedPagePath(request));
  for (const key of ["x-erpx-workspace-kind","x-erpx-branch-slug","x-erpx-department-slug","x-erpx-destination","x-erpx-api","x-erpx-method"]) headers.delete(key);
  headers.set("x-erpx-destination",parsed.destination.join("/") || "home");
  headers.set("x-erpx-api",String(api));
  headers.set("x-erpx-method",request.method);
  headers.set("x-erpx-workspace-kind", parsed.kind);
  if (parsed.branchSlug)
    headers.set("x-erpx-branch-slug", parsed.branchSlug);
  if (parsed.departmentSlug)
    headers.set("x-erpx-department-slug", parsed.departmentSlug);

  // Preserve the actual loopback origin. NextURL normalizes 127.0.0.1 to
  // localhost, which would turn this into a second HTTP request and lose scope.
  const internal = new URL(request.url);
  internal.pathname = `${prefix}${parsed.holdingSlug}/${parsed.destination.join("/") || "home"}`;
  const rewritten = internal.pathname !== request.nextUrl.pathname;
  const response = rewritten
    ? NextResponse.rewrite(internal, { request: { headers } })
    : NextResponse.next({ request: { headers } });
  if (!api && request.method === "GET")
    response.cookies.set("erpx_last_workspace", request.nextUrl.pathname, {
      httpOnly: true,
      sameSite: "lax",
      secure: production || request.nextUrl.protocol === "https:",
      path: "/",
      maxAge: 60 * 60 * 24 * 90,
    });
  return response;
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|vendor/).*)"],
};
