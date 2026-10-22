import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("projects");
export const config = { matcher: ["/:path*"] };
