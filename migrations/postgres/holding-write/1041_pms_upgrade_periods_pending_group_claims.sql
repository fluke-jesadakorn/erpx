-- Additive JSON shape guards; do not rewrite existing IDs, prices, money or documents.
-- The migration also advances the compiled release schema identity: older PMS code
-- cannot safely interpret historical room-type periods or pending group quota claims.
-- Rollback therefore requires a matching backup rather than an older schema identity.
ALTER TABLE pms_records ADD CONSTRAINT pms_stay_upgrade_history_shape CHECK (
  kind <> 'stay' OR (
    (NOT data_json::jsonb ? 'roomTypePeriods' OR jsonb_typeof(data_json::jsonb->'roomTypePeriods') = 'array')
    AND (NOT data_json::jsonb ? 'upgrades' OR jsonb_typeof(data_json::jsonb->'upgrades') = 'array')
  )
);
ALTER TABLE pms_records ADD CONSTRAINT pms_pending_group_claim_shape CHECK (
  kind <> 'block' OR (
    (NOT data_json::jsonb ? 'groupPickupClaims' OR jsonb_typeof(data_json::jsonb->'groupPickupClaims') = 'array')
    AND (NOT data_json::jsonb ? 'quotaOnly' OR jsonb_typeof(data_json::jsonb->'quotaOnly') = 'boolean')
    AND (data_json::jsonb->>'quotaOnly' IS DISTINCT FROM 'true' OR COALESCE((
      jsonb_typeof(data_json::jsonb->'groupPickupClaims') = 'array'
      AND jsonb_typeof(data_json::jsonb->'expiresAt') = 'number'
      AND jsonb_typeof(data_json::jsonb->'bookingChangeId') = 'string'
      AND length(data_json::jsonb->>'bookingChangeId') > 0
    ), false))
  )
);
