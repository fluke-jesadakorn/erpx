import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("sales");
export const config = { matcher: ["/:path*"] };
