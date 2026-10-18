import "server-only";
import {headers} from "next/headers";
import {productHostForHost} from "@/lib/runtime/product-hosts";
import type {TalentProductPresentationOptions,TalentProductSourceTarget} from "../domain/product-entry";
/** Configured product origins tighten the existing source reader, never grant authority. */
export async function talentProductPresentationOptions(kind:TalentProductSourceTarget["kind"]):Promise<TalentProductPresentationOptions>{
 const product=productHostForHost((await headers()).get("host"));return product===(kind==="hire"?"people":"recruit")?{strictProduct:true}:{};
}
