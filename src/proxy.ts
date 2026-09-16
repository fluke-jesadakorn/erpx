import { NextResponse, type NextRequest } from "next/server";
import { existsSync } from "node:fs";
import { join } from "node:path";

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

export function proxy(request: NextRequest) {
  const production = process.env.NODE_ENV === "production";
  const path = request.nextUrl.pathname;
  if (production && path !== "/api/health" && path !== "/api/ready") {
    const origin = new URL(process.env.ERPX_PUBLIC_ORIGIN!);
    if (request.headers.get("host") !== origin.host)
      return new NextResponse("Invalid host", { status: 421 });
    if (request.headers.get("x-forwarded-proto") === "http")
      return NextResponse.redirect(new URL(path + request.nextUrl.search, origin), 308);
    if (process.env.ERPX_DATA_DIR && existsSync(join(process.env.ERPX_DATA_DIR, "maintenance")))
      return new NextResponse("ERPX is undergoing scheduled maintenance.", { status: 503, headers: { "Retry-After": "60", "Cache-Control": "no-store" } });
  }
  if (request.nextUrl.pathname.startsWith("/platform/")) {
    const response = NextResponse.next();
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
  const api = request.nextUrl.pathname.startsWith("/api/v1/holdings/");
  if (!api && !request.nextUrl.pathname.startsWith("/holdings/")) return NextResponse.next();
  const prefix = api ? "/api/v1/holdings/" : "/holdings/";
  const parts = request.nextUrl.pathname
    .slice(prefix.length)
    .split("/")
    .filter(Boolean);
  const parsed = parse(parts);
  if (!parsed) return NextResponse.next();

  const headers = new Headers(request.headers);
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
  matcher: ["/((?!_next/static|_next/image|favicon.ico|vendor/).*)"],
};
