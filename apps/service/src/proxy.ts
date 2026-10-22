import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("service");
export const config = { matcher: ["/:path*"] };
