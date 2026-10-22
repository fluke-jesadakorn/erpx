import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("assets");
export const config = { matcher: ["/:path*"] };
