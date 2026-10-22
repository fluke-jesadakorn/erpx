import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("account");
export const config = { matcher: ["/:path*"] };
