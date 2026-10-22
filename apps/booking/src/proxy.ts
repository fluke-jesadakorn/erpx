import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("booking");
export const config = { matcher: ["/:path*"] };
