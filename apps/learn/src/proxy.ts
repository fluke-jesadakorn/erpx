import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("learn");
export const config = { matcher: ["/:path*"] };
