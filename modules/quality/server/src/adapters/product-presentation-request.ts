import "server-only";
import {headers} from "next/headers";
import {productHostForHost} from "@erpx/platform-runtime-ui/lib/product-hosts";
import type {QualityProductPresentationOptions} from "@erpx/quality-server/domain/product-entry";
/** A configured product host tightens presentation; it never supplies authority. */
export async function qualityProductPresentationOptions():Promise<QualityProductPresentationOptions>{
 return productHostForHost((await headers()).get("host"))==="supply"?{strictProduct:true}:{};
}
