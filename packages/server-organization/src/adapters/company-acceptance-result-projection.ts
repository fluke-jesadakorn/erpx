import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeOrgAll } from "./native-organization-owner-write";

export type CompanyAcceptanceResultRow = {
  id: string;
  company_id: string;
  actor_id: string;
  record_type: string;
  record_id: string;
  version: number;
  created_at: number;
  updated_at: number;
};

/** Native-owned Company-acceptance result projection. The SQL and column order
 * live in the Organization owner crate; this adapter sends only the typed kind
 * and the caller's positional values. No TypeScript SQL copy remains. */
export async function companyAcceptanceResultProjection(
  db: SqlDatabase,
  companyId: string,
  key: string,
  createdAt: number,
  updatedAt: number,
): Promise<CompanyAcceptanceResultRow[]> {
  return nativeOrgAll<CompanyAcceptanceResultRow>(
    db,
    companyId,
    "company_acceptance_result_projection",
    [key, createdAt, updatedAt, companyId],
  );
}
