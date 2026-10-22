import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("compat");
export const config = { matcher: ["/:path*"] };
