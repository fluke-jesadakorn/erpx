import "server-only";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
/** Public registration visibility without exposing the runtime or database capabilities. */
export async function publicBetaEnabled() { return (await runtimeEnv()).ERPX_PUBLIC_BETA === "true"; }
