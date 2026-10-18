import "server-only";
import {cache} from "react";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {qualitySourceRecordBackdrop} from "@/server/modules/quality/public/product-entry";
import {projectTaskSourceRecordBackdrop} from "@/server/modules/projects/public/product-entry";
import {qualityProductPresentationOptions} from "@/server/modules/quality/public/product-presentation-request";
import {projectTaskProductPresentationOptions} from "@/server/modules/projects/public/product-presentation-request";

type Kind="quality_hold"|"stock_count"|"staff_task";
type Summary={kind:"hold"|"count"|"task";title:string;status:string;description?:string};
/** React render-pass deduplication only. Both the background and its close
 * destination use the same minimal authorized owner observation. Commands
 * and subsequent requests always reauthorize their own current sources. */
export const sourceRecordPresentation=cache(async(holdingSlug:string,kind:Kind,id:string):Promise<Summary|null>=>{
 const actor=await requireWorkContext(holdingSlug);
 return kind==="staff_task"?projectTaskSourceRecordBackdrop(actor,{kind:"task",id},await projectTaskProductPresentationOptions()):qualitySourceRecordBackdrop(actor,{kind:kind==="quality_hold"?"hold":"count",id},await qualityProductPresentationOptions());
});
