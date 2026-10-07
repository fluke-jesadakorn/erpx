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
