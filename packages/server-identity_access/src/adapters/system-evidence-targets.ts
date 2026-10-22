import "server-only";
import type { PoolClient } from "pg";
import type { SystemEvidenceTarget } from "@erpx/server-platform_administration/public/system-evidence-contract";
const tables = { iam_review: "iam_reviews", iam_migration: "iam_migrations", people_provision_operation: "people_provision_operations" } as const;
export async function lockSystemEvidenceTarget(client: PoolClient, target: SystemEvidenceTarget, id: string, companyId: string) { return (await client.query<{ version: number; status: string }>(`SELECT version,status FROM ${tables[target]} WHERE id=$1 AND company_id=$2 FOR UPDATE`, [id, companyId])).rows[0] ?? null; }
export async function advanceSystemEvidenceTarget(client: PoolClient, target: SystemEvidenceTarget, id: string, companyId: string, now?: number) { await client.query(`UPDATE ${tables[target]} SET version=version+1${target === "people_provision_operation" ? ",updated_at=$3" : ""} WHERE id=$1 AND company_id=$2`, target === "people_provision_operation" ? [id, companyId, now] : [id, companyId]); }
