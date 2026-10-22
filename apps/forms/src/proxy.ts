import { appProxy } from "../../../scripts/local/app-proxy";
export const proxy = appProxy("forms");
export const config = { matcher: ["/:path*"] };
