import { developmentProductOrigin } from "@erpx/server-commercial/public/product-registry";
import { consoleOrigin } from "@erpx/feature-runtime/lib/erpx-hosts";
import { NextResponse } from "next/server";

/** Explicit development cutover only. Routing/SSO failures must not reopen a
 * retired Console. Rollback deliberately restores this switch and compatible code. */
export function legacyConsoleRetired() {
  return process.env.ERPX_CONSOLE_RETIRED === "enabled"
    && process.env.ERPX_ENVIRONMENT === "local"
    && process.env.ERPX_PRODUCT_ENVIRONMENT === "development"
    && consoleOrigin()?.origin === "https://console-dev-erpx.jesadakorn.com";
}

const headers = {
  "Cache-Control": "private, no-store",
  "CDN-Cache-Control": "no-store",
  "Cloudflare-CDN-Cache-Control": "no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "X-Robots-Tag": "noindex, nofollow",
  "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
};

/** Runs before rewrites, auth, assets and owner handlers. Never forwards a
 * request body, callback credential, mutation or user query to another host. */
export function consoleRetirementResponse(request: Request): NextResponse | null {
  if (!legacyConsoleRetired() || request.headers.get("host") !== consoleOrigin()?.host) return null;
  const pathname = new URL(request.url).pathname;
  if (pathname === "/api/health" || pathname === "/api/ready") return null;
  const apps = developmentProductOrigin("apps") + "/";
  const json = pathname === "/api" || pathname.startsWith("/api/") || request.headers.get("accept")?.includes("application/json");
  const body = json
    ? JSON.stringify({ error: { code: "CONSOLE_RETIRED", message: "Console has moved. Open ERPX Apps to continue.", appsUrl: apps } })
    : `<!doctype html><html lang="th"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><title>Console ปิดให้บริการ · ERPX Apps</title><style>body{font:1rem/1.7 system-ui,sans-serif;margin:0;padding:clamp(24px,8vw,96px);background:light-dark(#f2f5f8,#151a21);color:light-dark(#172033,#e7edf6)}main{max-width:40rem;margin:auto}h1{font-size:clamp(1.5rem,5vw,2.3rem)}a{display:inline-block;background:#2f69f5;color:white;padding:.75rem 1.25rem;border-radius:.6rem;text-decoration:none}a:focus-visible{outline:3px solid currentColor;outline-offset:4px}</style><main><p>ERPX · 410 Gone</p><h1>ย้ายจาก Console ไป ERPX Apps แล้ว</h1><p>Console ปิดให้บริการ กรุณาเปิด Apps เพื่อเลือกผลิตภัณฑ์และพื้นที่ทำงานที่คุณมีสิทธิ์ใช้งาน</p><p lang="en">Console has closed. Open Apps to choose your products and workspaces.</p><a href="${apps}">เปิด Apps · Open Apps</a></main></html>`;
  return new NextResponse(request.method === "HEAD" ? null : body, {
    status: 410,
    headers: { ...headers, "Content-Type": json ? "application/json; charset=utf-8" : "text/html; charset=utf-8" },
  });
}
