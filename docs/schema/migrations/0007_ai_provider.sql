-- Durable per-Holding AI provider config for BFF. Control-plane mirror of
-- holding_write 0010, using the control_write pool available to the console
-- BFF without requiring per-holding pool resolution.

CREATE SCHEMA IF NOT EXISTS erpx_control;

CREATE TABLE erpx_control.ai_provider_configs (
    holding_id UUID PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('openai','custom','anthropic','gemini','azure','ollama')),
    endpoint_url TEXT NOT NULL CHECK (octet_length(endpoint_url) BETWEEN 1 AND 512),
    model TEXT NOT NULL CHECK (octet_length(model) BETWEEN 1 AND 128),
    api_key_cipher BYTEA NOT NULL CHECK (octet_length(api_key_cipher) BETWEEN 16 AND 4096),
    api_key_nonce BYTEA NOT NULL CHECK (octet_length(api_key_nonce)=12),
    key_reference TEXT NOT NULL CHECK (btrim(key_reference) <> '' AND octet_length(key_reference) <= 128),
    per_module_override TEXT CHECK (per_module_override IS NULL OR octet_length(per_module_override) BETWEEN 1 AND 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION erpx_control.ai_provider_touch_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS ai_provider_configs_touch ON erpx_control.ai_provider_configs;
CREATE TRIGGER ai_provider_configs_touch
    BEFORE UPDATE ON erpx_control.ai_provider_configs
    FOR EACH ROW EXECUTE FUNCTION erpx_control.ai_provider_touch_updated_at();

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM erpx_meta.schema_manifest
        WHERE singleton=TRUE AND schema_revision=2 AND manifest_version=1
          AND database_role='control_write' AND migration_set='control-write'
    ) THEN
        RAISE EXCEPTION 'control-write current manifest is not current';
    END IF;
END;
$$;
