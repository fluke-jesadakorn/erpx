import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("portal");
export const config = { matcher: ["/:path*"] };
