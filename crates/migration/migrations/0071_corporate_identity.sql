-- Governed corporate identity is public presentation metadata in the ERPX
-- control plane. It is intentionally independent of customer-hosted data
-- planes and stores only typed token JSON and immutable asset references.

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_profiles (
    profile_id UUID PRIMARY KEY,
    scope_key TEXT NOT NULL UNIQUE,
    scope_kind TEXT NOT NULL
        CHECK (scope_kind IN ('platform', 'holding', 'business_unit')),
    tenant_id TEXT,
    root_holding_id UUID,
    business_unit TEXT,
    current_draft_version BIGINT NOT NULL DEFAULT 0
        CHECK (current_draft_version >= 0),
    current_published_version BIGINT
        CHECK (current_published_version IS NULL OR current_published_version > 0),
    suspended_at TIMESTAMPTZ,
    suspension_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, root_holding_id)
        REFERENCES erpx_control.customer_tenant_bindings (tenant_id, root_holding_id)
        ON DELETE RESTRICT,
    CHECK (
        (scope_kind = 'platform'
            AND tenant_id IS NULL
            AND root_holding_id IS NULL
            AND business_unit IS NULL)
        OR
        (scope_kind = 'holding'
            AND tenant_id IS NOT NULL
            AND root_holding_id IS NOT NULL
            AND business_unit IS NULL)
        OR
        (scope_kind = 'business_unit'
            AND tenant_id IS NOT NULL
            AND root_holding_id IS NOT NULL
            AND business_unit IS NOT NULL)
    ),
    CHECK (business_unit IS NULL OR business_unit IN (
        'operations', 'finance', 'sales', 'procurement', 'inventory', 'assets',
        'maintenance', 'quality', 'marketing-crm', 'hr-payroll', 'point-of-sale', 'legal'
    )),
    CHECK ((suspended_at IS NULL) = (suspension_reason IS NULL)),
    CHECK (suspension_reason IS NULL
        OR length(btrim(suspension_reason)) BETWEEN 1 AND 512)
);

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_platform_profile_idx
    ON erpx_control.corporate_identity_profiles (scope_kind)
    WHERE scope_kind = 'platform';

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_holding_profile_idx
    ON erpx_control.corporate_identity_profiles (tenant_id, root_holding_id)
    WHERE scope_kind = 'holding';

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_business_unit_profile_idx
    ON erpx_control.corporate_identity_profiles
        (tenant_id, root_holding_id, business_unit)
    WHERE scope_kind = 'business_unit';

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_versions (
    profile_id UUID NOT NULL
        REFERENCES erpx_control.corporate_identity_profiles(profile_id) ON DELETE RESTRICT,
    version BIGINT NOT NULL CHECK (version > 0),
    lifecycle TEXT NOT NULL
        CHECK (lifecycle IN ('draft', 'published', 'archived', 'suspended')),
    parent_published_version BIGINT,
    display_name TEXT NOT NULL,
    short_name TEXT NOT NULL,
    tokens JSONB NOT NULL CHECK (jsonb_typeof(tokens) = 'object'),
    assets JSONB NOT NULL CHECK (jsonb_typeof(assets) = 'object'),
    overridden_categories TEXT[] NOT NULL DEFAULT '{}',
    locked_categories TEXT[] NOT NULL DEFAULT '{}',
    updated_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    correlation_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    PRIMARY KEY (profile_id, version),
    CHECK (length(btrim(display_name)) BETWEEN 1 AND 96),
    CHECK (length(btrim(short_name)) BETWEEN 1 AND 24),
    CHECK (length(btrim(updated_by)) BETWEEN 1 AND 255),
    CHECK (length(btrim(reason)) BETWEEN 1 AND 512),
    CHECK (parent_published_version IS NULL OR parent_published_version > 0),
    CHECK (overridden_categories <@ ARRAY[
        'identity', 'color', 'typography', 'shape', 'elevation',
        'density', 'navigation', 'motion'
    ]::TEXT[]),
    CHECK (locked_categories <@ ARRAY[
        'identity', 'color', 'typography', 'shape', 'elevation',
        'density', 'navigation', 'motion'
    ]::TEXT[]),
    CHECK (lifecycle <> 'published' OR published_at IS NOT NULL),
    CHECK (lifecycle <> 'archived' OR archived_at IS NOT NULL)
);

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_one_draft_idx
    ON erpx_control.corporate_identity_versions(profile_id)
    WHERE lifecycle = 'draft';

CREATE UNIQUE INDEX IF NOT EXISTS corporate_identity_one_published_idx
    ON erpx_control.corporate_identity_versions(profile_id)
    WHERE lifecycle = 'published';

CREATE INDEX IF NOT EXISTS corporate_identity_history_idx
    ON erpx_control.corporate_identity_versions(profile_id, version DESC);

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_assets (
    asset_id UUID PRIMARY KEY,
    profile_id UUID NOT NULL
        REFERENCES erpx_control.corporate_identity_profiles(profile_id) ON DELETE RESTRICT,
    asset_kind TEXT NOT NULL
        CHECK (asset_kind IN ('logo_mark', 'wordmark', 'favicon', 'login_artwork')),
    object_key TEXT NOT NULL UNIQUE,
    sha256 CHAR(64) NOT NULL CHECK (sha256 ~ '^[0-9A-Fa-f]{64}$'),
    media_type TEXT NOT NULL
        CHECK (media_type IN ('image/png', 'image/webp', 'image/jpeg', 'image/svg+xml')),
    byte_length BIGINT NOT NULL CHECK (byte_length BETWEEN 1 AND 5242880),
    width INTEGER CHECK (width BETWEEN 1 AND 4096),
    height INTEGER CHECK (height BETWEEN 1 AND 4096),
    validation_state TEXT NOT NULL
        CHECK (validation_state IN ('pending', 'valid', 'rejected')),
    validation_reason TEXT,
    uploaded_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    validated_at TIMESTAMPTZ,
    CHECK (object_key LIKE 'branding/%'),
    CHECK (object_key NOT LIKE '%..%'),
    CHECK (position('://' IN object_key) = 0),
    CHECK (length(btrim(uploaded_by)) BETWEEN 1 AND 255),
    CHECK ((validation_state = 'pending') = (validated_at IS NULL)),
    CHECK (validation_reason IS NULL
        OR length(btrim(validation_reason)) BETWEEN 1 AND 512)
);

CREATE INDEX IF NOT EXISTS corporate_identity_asset_profile_idx
    ON erpx_control.corporate_identity_assets(profile_id, validation_state, asset_kind);

CREATE TABLE IF NOT EXISTS erpx_control.corporate_identity_domains (
    hostname TEXT PRIMARY KEY,
    profile_id UUID NOT NULL
        REFERENCES erpx_control.corporate_identity_profiles(profile_id) ON DELETE RESTRICT,
    verification_digest BYTEA NOT NULL CHECK (octet_length(verification_digest) = 32),
    verified_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (hostname = lower(hostname)),
    CHECK (hostname !~ '[/\\?#:]'),
    CHECK (length(hostname) BETWEEN 1 AND 253),
    CHECK (NOT (verified_at IS NOT NULL AND revoked_at IS NOT NULL))
);

CREATE INDEX IF NOT EXISTS corporate_identity_verified_domain_idx
    ON erpx_control.corporate_identity_domains(profile_id, hostname)
    WHERE verified_at IS NOT NULL AND revoked_at IS NULL;

-- The neutral platform identity is a normal published version so every
-- holding can inherit immediately without a destructive data migration.
INSERT INTO erpx_control.corporate_identity_profiles
    (profile_id, scope_key, scope_kind, current_draft_version,
     current_published_version, created_at, updated_at)
VALUES
    ('00000000-0000-0000-0000-000000000071', 'platform', 'platform', 0, 1, now(), now())
ON CONFLICT (scope_key) DO NOTHING;

INSERT INTO erpx_control.corporate_identity_versions
    (profile_id, version, lifecycle, display_name, short_name, tokens, assets,
     overridden_categories, locked_categories, updated_by, reason,
     correlation_id, created_at, published_at)
VALUES (
    '00000000-0000-0000-0000-000000000071',
    1,
    'published',
    'ERPX Platform',
    'ERPX',
    '{
      "light":{"primary":"#075985","on_primary":"#ffffff","accent":"#0f766e","on_accent":"#ffffff","canvas":"#f5f7f9","surface":"#ffffff","surface_raised":"#edf2f5","border":"#7b8b98","text":"#111827","text_muted":"#4b5b68","link":"#075985","focus":"#075985"},
      "dark":{"primary":"#67e8f9","on_primary":"#082f49","accent":"#5eead4","on_accent":"#042f2e","canvas":"#0b1117","surface":"#111a22","surface_raised":"#1b2833","border":"#687b89","text":"#f1f5f9","text_muted":"#a8bac7","link":"#67e8f9","focus":"#a5f3fc"},
      "font_pack":"erpx_sans","heading_scale_percent":100,"body_scale_percent":100,
      "line_height_percent":150,"density":"comfortable","radius":"soft",
      "elevation":"subtle","navigation":"tinted",
      "navigation_expanded_by_default":true,"motion":"subtle"
    }'::jsonb,
    '{}'::jsonb,
    ARRAY['identity','color','typography','shape','elevation','density','navigation','motion'],
    ARRAY[]::TEXT[],
    'migration:0071',
    'neutral ERPX corporate identity bootstrap',
    '00000000-0000-0000-0000-000000000071',
    now(),
    now()
)
ON CONFLICT (profile_id, version) DO NOTHING;
