import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("support");
export const config = { matcher: ["/:path*"] };
