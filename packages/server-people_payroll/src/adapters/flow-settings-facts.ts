import "server-only";
import { ppRd38SinglePredTail } from "./pp-read-tail-r38-sources";
import { ppRd43ActiveLeavePolicyProjection } from "./pp-read-tail-r43-sources";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

export function activeLeavePolicySettings(db: SqlDatabase, companyId: string) {
  return db.prepare(ppRd43ActiveLeavePolicyProjection('name,version,status'))
    .bind(companyId).all<{ name: string; version: number; status: string }>().then(result => result.results);
}
export function leavePolicySettingsHistory(db: SqlDatabase, companyId: string) {
  return db.prepare(ppRd38SinglePredTail("name title,version,created_by actor,created_at at,'Leave policy' kind", "leave_policy_versions", "company_id", " ORDER BY created_at DESC"))
    .bind(companyId).all<{ title: string; version: number; actor: string; at: number; kind: string }>().then(result => result.results);
}
