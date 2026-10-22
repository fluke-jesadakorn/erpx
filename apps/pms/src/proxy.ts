import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("pms");
export const config = { matcher: ["/:path*"] };
