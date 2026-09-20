-- Persist the original confirmation response; retries must not expose later edits.
ALTER TABLE pms_booking_receipts ADD COLUMN response_json TEXT CHECK(response_json IS NULL OR erpx_json_valid(response_json));
