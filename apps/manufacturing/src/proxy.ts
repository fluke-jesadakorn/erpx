import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("manufacturing");
export const config = { matcher: ["/:path*"] };
