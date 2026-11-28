import "server-only";
import { headers } from "next/headers";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { nativeSocialPageDataSSR } from "./native-social-page";

type PageInput = Parameters<typeof nativeSocialPageDataSSR>[1];

/** Forward the authenticated rendered workspace to the native Social owner. */
export async function socialPageData(actor: WorkContext, input: PageInput) {
  return nativeSocialPageDataSSR(actor, input, await headers());
}
