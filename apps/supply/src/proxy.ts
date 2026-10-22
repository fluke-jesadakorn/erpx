import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("supply");
export const config = { matcher: ["/:path*"] };
