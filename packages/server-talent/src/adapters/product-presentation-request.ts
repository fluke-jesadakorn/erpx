import "server-only";
import {headers} from "next/headers";
import {productHostForHost} from "@erpx/feature-runtime/lib/product-hosts";
import type {TalentProductPresentationOptions,TalentProductSourceTarget} from "@erpx/server-talent/domain/product-entry";
/** Configured product origins tighten the existing source reader, never grant authority. */
export async function talentProductPresentationOptions(kind:TalentProductSourceTarget["kind"]):Promise<TalentProductPresentationOptions>{
 const product=productHostForHost((await headers()).get("host"));return product===(kind==="hire"?"people":"recruit")?{strictProduct:true}:{};
}
