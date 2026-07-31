-- A delivery projection cursor is tenant/holding scoped. 0042 originally
-- keyed the table by projection name alone, which allowed one holding to
-- overwrite another holding's cursor and made a concurrent projector fail
-- closed as a scope conflict.  Preserve existing rows while widening the
-- durable key to the actual read-model scope.
ALTER TABLE erpx_tenant.marketing_delivery_projection_checkpoints
    DROP CONSTRAINT IF EXISTS marketing_delivery_projection_checkpoints_pkey;

ALTER TABLE erpx_tenant.marketing_delivery_projection_checkpoints
    ADD PRIMARY KEY (projection_name, holding_ref);
