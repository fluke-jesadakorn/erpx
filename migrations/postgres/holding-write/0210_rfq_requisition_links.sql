-- Optional typed requisition source for new RFQs. Historical standalone RFQs remain readable.
ALTER TABLE procurement_rfqs ADD COLUMN request_id TEXT REFERENCES employee_requests(id);
ALTER TABLE procurement_rfqs ADD COLUMN request_version BIGINT;
ALTER TABLE procurement_rfqs ADD COLUMN request_evidence_version BIGINT;
ALTER TABLE procurement_rfqs ADD COLUMN request_items_json TEXT;
ALTER TABLE procurement_rfq_lines ADD COLUMN request_item_ordinal INTEGER CHECK(request_item_ordinal>=0);
CREATE UNIQUE INDEX procurement_rfq_active_request ON procurement_rfqs(request_id) WHERE request_id IS NOT NULL AND status<>'cancelled';
