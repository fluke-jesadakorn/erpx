import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("legal");
export const config = { matcher: ["/:path*"] };
