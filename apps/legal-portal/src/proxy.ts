import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("legal-portal");
export const config = { matcher: ["/:path*"] };
