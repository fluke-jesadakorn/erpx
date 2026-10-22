import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("people");
export const config = { matcher: ["/:path*"] };
