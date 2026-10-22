import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("admin");
export const config = { matcher: ["/:path*"] };
