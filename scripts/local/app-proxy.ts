import { NextResponse, type NextRequest } from "next/server";
import { appDefinition } from "@erpx/contracts/apps";
import { proxy } from "@erpx/workplace/proxy";

/** Routing ownership cannot grant authentication or broaden an owner API. */
export function appProxy(id: string) {
  const host = id === "compat" ? null : new URL(appDefinition(id).origin).host;
  return function ownedAppProxy(request: NextRequest) {
    const health = ["/api/health", "/api/ready"].includes(request.nextUrl.pathname);
    const actual = request.headers.get("host");
    if (host && actual !== host && !(health && /^(?:127\.0\.0\.1|localhost|\[::1\])(?::\d+)?$/.test(actual ?? "")))
      return new NextResponse("Invalid application host", { status: 421 });
    const response = proxy(request);
    response.headers.set("X-ERPX-App", id);
    return response;
  };
}
