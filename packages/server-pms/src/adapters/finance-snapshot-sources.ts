import "server-only";
/** Closed PMS transfer and posting projections retain the original supplied transaction snapshot. */
export function financeSnapshotSources(){return {
  "verifiedTransfers": "FROM pms_transfer_requests r WHERE r.company_id=? AND r.property_id=? AND r.business_date=? AND r.state='verified'",
  "propertyBank": "SELECT p.config_json::jsonb->'accounts'->>'bank' FROM pms_properties p WHERE p.id=r.property_id",
  "postedProperty": "EXISTS(SELECT 1 FROM pms_postings pp WHERE pp.journal_id=j.id AND pp.property_id=?)"
} as const;}
