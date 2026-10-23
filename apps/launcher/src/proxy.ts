import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("apps");
export const config = { matcher: ["/:path*"] };
