-- Additive outbound routing metadata for Marketing provider accounts.
-- Existing rows remain routable for callback verification while the
-- deployment resolves the legacy placeholders as unconfigured outbound
-- routes. No credential material is stored in PostgreSQL.
ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS provider_key TEXT NOT NULL
        DEFAULT 'legacy-unconfigured'
        CHECK (length(btrim(provider_key)) BETWEEN 1 AND 255);

ALTER TABLE erpx_control.marketing_provider_accounts
    ADD COLUMN IF NOT EXISTS outbound_credential_ref TEXT NOT NULL
        DEFAULT 'legacy-unconfigured-credential'
        CHECK (length(btrim(outbound_credential_ref)) BETWEEN 1 AND 255);
