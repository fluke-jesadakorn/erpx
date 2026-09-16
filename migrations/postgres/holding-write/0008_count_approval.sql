ALTER TABLE inventory_counts DROP CONSTRAINT inventory_counts_status_check;
ALTER TABLE inventory_counts ADD CONSTRAINT inventory_counts_status_check CHECK(status IN ('draft','submitted','approved','rejected','posted','cancelled'));
ALTER TABLE inventory_count_decisions DROP CONSTRAINT inventory_count_decisions_action_check;
ALTER TABLE inventory_count_decisions ADD CONSTRAINT inventory_count_decisions_action_check CHECK(action IN ('approve','post','reject','cancel'));
