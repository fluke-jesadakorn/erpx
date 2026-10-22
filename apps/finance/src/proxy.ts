import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("finance");
export const config = { matcher: ["/:path*"] };
