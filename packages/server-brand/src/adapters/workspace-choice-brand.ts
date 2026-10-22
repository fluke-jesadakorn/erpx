import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export async function workspaceChoiceBrands(write:SqlDatabase,companyIds:string[]){const placeholders=companyIds.map(()=>'?').join(',');
  return (await write.prepare("SELECT scope_kind,scope_id,primary_publication_id FROM organization_brand_profiles WHERE scope_kind='holding' OR company_id IN (" + placeholders + ")")
    .bind(...companyIds).all<{ scope_kind: "holding" | "branch"; scope_id: string; primary_publication_id: string | null }>()).results;
}
