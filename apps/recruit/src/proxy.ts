import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("recruit");
export const config = { matcher: ["/:path*"] };
