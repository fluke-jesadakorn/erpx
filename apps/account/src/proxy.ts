import type { NextRequest } from "next/server";
import { developmentConsoleRetired } from "@erpx/contracts/console-retirement";
import { proxy as workspaceProxy } from "@erpx/workplace/proxy";
import { appProxy } from "../../../scripts/local/app-proxy";

const ownedProxy = appProxy("account");
export function proxy(request: NextRequest) {
  // Account owns the retired Console's existing public health reads. Keep
  // every other host/path/method behind the normal application host guard.
  if (developmentConsoleRetired(process.env)
    && request.headers.get("host") === "console-dev-erpx.jesadakorn.com"
    && ["GET", "HEAD"].includes(request.method)
    && ["/api/health", "/api/ready"].includes(request.nextUrl.pathname)) {
    const response = workspaceProxy(request);
    response.headers.set("X-ERPX-App", "account");
    return response;
  }
  return ownedProxy(request);
}
export const config = { matcher: ["/:path*"] };
