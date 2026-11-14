//! The retained `pmsSourcesSql` union, owned by PMS.
//!
//! Exact copy of `packages/server-pms/src/adapters/pms/source-query.ts`. The
//! same union is required by the Identity-owned PMS social recipient query; the
//! Identity crate cannot depend on PMS (PMS already depends on
//! Platform Administration, which reaches Identity), so that adapter retains an
//! identical local copy. Keep both copies byte-for-byte equal.
pub const PMS_SOURCES_SQL: &str = "SELECT r.id,r.property_id,r.kind,r.data_json::jsonb data,r.version,r.updated_at,\n    p.company_id,p.branch_id,p.name property_name,p.business_date\n  FROM pms_records r JOIN pms_properties p ON p.id=r.property_id\n  UNION ALL\n  SELECT t.id,t.property_id,'transfer',jsonb_build_object('state',t.state,'recordedBy',t.recorded_by,\n    'guestAccountId',t.guest_account_id),t.version,t.updated_at,p.company_id,p.branch_id,p.name,p.business_date\n  FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id\n  UNION ALL\n  SELECT c.id,c.property_id,'correction',jsonb_build_object('state',c.status,'requestedBy',c.requested_by,\n    'guestAccountId',c.guest_account_id),c.version,c.updated_at,p.company_id,p.branch_id,p.name,p.business_date\n  FROM pms_charge_corrections c JOIN pms_properties p ON p.id=c.property_id AND p.company_id=c.company_id";
