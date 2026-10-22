import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("work");
export const config = { matcher: ["/:path*"] };
