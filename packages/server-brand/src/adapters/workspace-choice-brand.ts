import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { BRAND_RD_WORKSPACE_CHOICE, planBrandRead } from "./native-brand-read-ports";
export async function workspaceChoiceBrands(write:SqlDatabase,companyIds:string[]){
  const plan = await planBrandRead(BRAND_RD_WORKSPACE_CHOICE, companyIds);
  return (await write.prepare(plan.sql).bind(...plan.values).all<{ scope_kind: "holding" | "branch"; scope_id: string; primary_publication_id: string | null }>()).results;
}
