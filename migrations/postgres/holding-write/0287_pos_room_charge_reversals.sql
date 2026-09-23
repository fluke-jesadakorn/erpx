CREATE TABLE pos_room_charge_reversals (
  sale_id TEXT PRIMARY KEY REFERENCES pos_sales(id),
  company_id TEXT NOT NULL,
  property_id TEXT NOT NULL REFERENCES pms_properties(id),
  original_entry_id TEXT NOT NULL,
  reversal_entry_id TEXT NOT NULL UNIQUE,
  reason TEXT NOT NULL,
  restocked_lines_json TEXT NOT NULL DEFAULT '[]',
  restored_cost_minor BIGINT NOT NULL DEFAULT 0,
  wasted_cost_minor BIGINT NOT NULL DEFAULT 0,
  reversed_by TEXT NOT NULL,
  reversed_at BIGINT NOT NULL
);
CREATE INDEX pos_room_charge_reversals_company ON pos_room_charge_reversals(company_id,reversed_at DESC);
