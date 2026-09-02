-- Per-Holding AI provider config — free: 1 set/Holding, paid: per-module override.
-- Encrypted via aes-gcm at application layer; storage is opaque BYTEA.
-- Endpoint must be https (loopback http allowed), model free-form, capability-gated by commercial flow_ai.

CREATE SCHEMA IF NOT EXISTS erpx_ai_provider_write;

CREATE TABLE erpx_ai_provider_write.configs (
    holding_id UUID NOT NULL DEFAULT erpx_cqrs_write.current_holding_id(),
    kind TEXT NOT NULL CHECK (kind IN ('openai','custom','anthropic','gemini','azure','ollama')),
    endpoint_url TEXT NOT NULL CHECK (octet_length(endpoint_url) BETWEEN 1 AND 512),
    model TEXT NOT NULL CHECK (octet_length(model) BETWEEN 1 AND 128),
    api_key_cipher BYTEA NOT NULL CHECK (octet_length(api_key_cipher) BETWEEN 16 AND 4096),
    api_key_nonce BYTEA NOT NULL CHECK (octet_length(api_key_nonce)=12),
    key_reference TEXT NOT NULL CHECK (btrim(key_reference) <> '' AND octet_length(key_reference) <= 128),
    per_module_override TEXT CHECK (per_module_override IS NULL OR octet_length(per_module_override) BETWEEN 1 AND 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (holding_id)
);

CREATE OR REPLACE FUNCTION erpx_ai_provider_write.touch_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS ai_provider_configs_touch ON erpx_ai_provider_write.configs;
CREATE TRIGGER ai_provider_configs_touch
    BEFORE UPDATE ON erpx_ai_provider_write.configs
    FOR EACH ROW EXECUTE FUNCTION erpx_ai_provider_write.touch_updated_at();

DO $$
DECLARE guarded REGCLASS;
    policy_name TEXT;
BEGIN
    FOREACH guarded IN ARRAY ARRAY['erpx_ai_provider_write.configs'::REGCLASS] LOOP
        EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', guarded);
        EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', guarded);
        policy_name := replace(guarded::TEXT,'.','_')||'_holding_isolation';
        EXECUTE format('DROP POLICY IF EXISTS %I ON %s', policy_name, guarded);
        EXECUTE format(
            'CREATE POLICY %I ON %s USING (holding_id=erpx_cqrs_write.current_holding_id()) WITH CHECK (holding_id=erpx_cqrs_write.current_holding_id())',
            policy_name, guarded
        );
    END LOOP;
END;
$$;

UPDATE erpx_meta.schema_manifest
SET installed_at=now()
WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
  AND database_role='holding_write' AND migration_set='holding-write';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='holding_write' AND migration_set='holding-write'
    ) THEN
        RAISE EXCEPTION 'holding-write current manifest is not current';
    END IF;
END;
$$;
