/** A single source identity for PMS records, bank reviews and charge corrections. */
export const pmsSourcesSql = `SELECT r.id,r.property_id,r.kind,r.data_json::jsonb data,r.version,r.updated_at,
    p.company_id,p.branch_id,p.name property_name,p.business_date
  FROM pms_records r JOIN pms_properties p ON p.id=r.property_id
  UNION ALL
  SELECT t.id,t.property_id,'transfer',jsonb_build_object('state',t.state,'recordedBy',t.recorded_by,
    'folioId',t.folio_id),t.version,t.updated_at,p.company_id,p.branch_id,p.name,p.business_date
  FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id
  UNION ALL
  SELECT c.id,c.property_id,'correction',jsonb_build_object('state',c.status,'requestedBy',c.requested_by,
    'folioId',c.folio_id),c.version,c.updated_at,p.company_id,p.branch_id,p.name,p.business_date
  FROM pms_charge_corrections c JOIN pms_properties p ON p.id=c.property_id AND p.company_id=c.company_id`;

/** Uses live account grants, including custom roles; never grants a department access to PMS. */
export function pmsSourceReadableSql(source: string, user: string, assignment?:string) {
  const allows = (permission: string) => `iam_role_allows(pa.role,pa.company_id,'${permission}')`;
  return `EXISTS(SELECT 1 FROM assignments pa WHERE pa.user_id=${user} AND pa.company_id=${source}.company_id
    ${assignment?`AND pa.id=${assignment}`:""}
    AND pa.revoked_at IS NULL AND iam_assignment_active(pa.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(pa.user_id,pa.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND pa.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND pa.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
    AND (pa.scope_kind='holding' OR (pa.scope_kind='branch' AND pa.scope_node_id=${source}.branch_id))
    AND EXISTS(SELECT 1 FROM organization_nodes pn WHERE pn.id=${source}.branch_id AND pn.company_id=${source}.company_id AND pn.status='active')
    AND ${allows("pms.read")} AND (
      (${source}.kind IN ('room','task','block') AND (${allows("pms.housekeeping")} OR ${allows("pms.inspect")} OR ${allows("pms.maintenance")}))
      OR (${source}.kind IN ('stay','reservation') AND (${allows("pms.frontdesk")} OR ${allows("pms.reservations")} OR ${allows("pms.cashier")}))
      OR (${source}.kind='folio' AND (${allows("pms.cashier")} OR ${allows("pms.reports")}))
      OR (${source}.kind IN ('refund','correction') AND (${allows("pms.approve")} OR (${allows("pms.cashier")} AND ${source}.data->>'requestedBy'=pa.user_id)))
      OR (${source}.kind='transfer' AND (${allows("finance.approve")} OR (${allows("pms.cashier")} AND ${source}.data->>'recordedBy'=pa.user_id)))
      OR (${source}.kind='shift' AND (${allows("pms.approve")} OR ${allows("pms.audit")} OR (${allows("pms.cashier")} AND ${source}.data->>'userId'=pa.user_id)))
      OR (${source}.kind='night' AND (${allows("pms.audit")} OR ${allows("pms.reports")}))
    ))`;
}

/** PMS post text itself is private, not just the link preview. */
export function pmsPostSourcesReadableSql(post: string, user: string, assignment?:string) {
  return `NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${post}.id AND ps.source_kind='pms_record'
    AND NOT EXISTS(SELECT 1 FROM (${pmsSourcesSql}) pr WHERE pr.id=ps.source_id
      AND pr.company_id=${post}.company_id AND ${pmsSourceReadableSql("pr", user, assignment)}))`;
}
