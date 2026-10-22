import { pmsSourceBranchActiveSql } from "@erpx/server-organization/public/pms-source-authorization";
export function pmsSourceReadableSql(source: string, user: string, assignment?: string) {
    const allows = (permission: string) => `iam_role_allows(pa.role,pa.company_id,'${permission}')`;
    return `EXISTS(SELECT 1 FROM assignments pa WHERE pa.user_id=${user} AND pa.company_id=${source}.company_id
    ${assignment ? `AND pa.id=${assignment}` : ""}
    AND pa.revoked_at IS NULL AND iam_assignment_active(pa.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(pa.user_id,pa.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND pa.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND pa.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND (pa.scope_kind='holding' OR (pa.scope_kind='branch' AND pa.scope_node_id=${source}.branch_id))
    AND ${pmsSourceBranchActiveSql(source)}
    AND (${allows("pms.read")} OR (${allows("finance.read")} AND ${source}.kind IN ('guest_account','transfer'))) AND (
      (${source}.kind IN ('room','task','block') AND (${allows("pms.housekeeping")} OR ${allows("pms.inspect")} OR ${allows("pms.maintenance")}))
      OR (${source}.kind IN ('stay','reservation') AND (${allows("pms.frontdesk")} OR ${allows("pms.reservations")} OR ${allows("pms.cashier")}))
      OR (${source}.kind='guest_account' AND (${allows("pms.cashier")} OR ${allows("pms.reports")}))
      OR (${source}.kind IN ('refund','correction') AND (${allows("pms.approve")} OR (${allows("pms.cashier")} AND ${source}.data->>'requestedBy'=pa.user_id)))
      OR (${source}.kind='transfer' AND (${allows("finance.approve")} OR (${allows("pms.cashier")} AND ${source}.data->>'recordedBy'=pa.user_id)))
      OR (${source}.kind='shift' AND (${allows("pms.approve")} OR ${allows("pms.audit")} OR (${allows("pms.cashier")} AND ${source}.data->>'userId'=pa.user_id)))
      OR (${source}.kind='night' AND (${allows("pms.audit")} OR ${allows("pms.reports")}))
    ))`;
}
