CREATE TABLE inventory_reorder_policies (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
  product_id TEXT NOT NULL REFERENCES commerce_products(id),
  current_reorder_point_milli BIGINT CHECK(current_reorder_point_milli IS NULL OR current_reorder_point_milli > 0),
  proposed_reorder_point_milli BIGINT CHECK(proposed_reorder_point_milli IS NULL OR proposed_reorder_point_milli > 0),
  requested_by TEXT NOT NULL,
  approved_by TEXT,
  approved_at BIGINT,
  retired_at BIGINT,
  last_decision TEXT CHECK(last_decision IS NULL OR last_decision IN ('approved','rejected','retired')),
  last_reviewed_by TEXT,
  last_review_reason TEXT,
  last_reviewed_at BIGINT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(warehouse_id,product_id),
  CHECK(proposed_reorder_point_milli IS NULL OR requested_by IS NOT NULL),
  CHECK(last_decision IS NULL OR (last_reviewed_by IS NOT NULL AND last_review_reason IS NOT NULL AND last_reviewed_at IS NOT NULL))
);
CREATE INDEX inventory_reorder_policies_scope ON inventory_reorder_policies(company_id,warehouse_id,product_id);

CREATE TABLE inventory_reorder_policy_events (
  id TEXT PRIMARY KEY,
  policy_id TEXT NOT NULL REFERENCES inventory_reorder_policies(id),
  policy_version BIGINT NOT NULL CHECK(policy_version > 0),
  action TEXT NOT NULL CHECK(action IN ('propose','revise','approve','reject','withdraw','retire')),
  actor_id TEXT NOT NULL,
  reason TEXT NOT NULL,
  snapshot_json JSONB NOT NULL CHECK(jsonb_typeof(snapshot_json)='object'),
  created_at BIGINT NOT NULL,
  UNIQUE(policy_id,policy_version)
);

CREATE FUNCTION inventory_reorder_policy_event_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'INVENTORY_REORDER_POLICY_EVENT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER inventory_reorder_policy_events_no_update
  BEFORE UPDATE OR DELETE ON inventory_reorder_policy_events
  FOR EACH ROW EXECUTE FUNCTION inventory_reorder_policy_event_immutable();

SELECT ensure_work_cache_coverage();
