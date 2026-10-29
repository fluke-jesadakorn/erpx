import { NextResponse, type NextRequest } from "next/server";
import { appDefinition } from "@erpx/contracts/apps";
import { holdingAwareProxy } from "@erpx/workplace/proxy";
import {holdingRoutingEnabled,holdingHostForHost} from '@erpx/contracts/holding-hosts';

/** Routing ownership cannot grant authentication or broaden an owner API. */
export function appProxy(id: string, registeredAppsPages: readonly string[] = []) {
  const host = id === "compat" ? null : new URL(appDefinition(id).origin).host;
  return async function ownedAppProxy(request: NextRequest) {
    const health = ["/api/health", "/api/ready"].includes(request.nextUrl.pathname);
    const actual = request.headers.get("host");
    if (host && actual !== host && !(holdingRoutingEnabled(process.env)&&id!=='account'&&holdingHostForHost(actual)) && !(health && /^(?:127\.0\.0\.1|localhost|\[::1\])(?::\d+)?$/.test(actual ?? "")))
      return new NextResponse("Invalid application host", { status: 421 });
    const response = await holdingAwareProxy(request, id === "apps" ? registeredAppsPages : []);
    response.headers.set("X-ERPX-App", id);
    return response;
  };
}
