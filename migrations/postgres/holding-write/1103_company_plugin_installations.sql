CREATE TABLE company_plugin_installations (
    company_id TEXT NOT NULL,
    plugin_id TEXT NOT NULL,
    installation_version BIGINT NOT NULL CHECK (installation_version > 0),
    plugin_version TEXT NOT NULL,
    repository_id BIGINT NOT NULL CHECK (repository_id > 0),
    repository_commit TEXT NOT NULL CHECK (repository_commit ~ '^[0-9a-f]{40}$'),
    manifest_digest TEXT NOT NULL CHECK (manifest_digest ~ '^[0-9a-f]{64}$'),
    manifest JSONB NOT NULL CHECK (jsonb_typeof(manifest) = 'object'),
    installed_by TEXT NOT NULL,
    installed_at BIGINT NOT NULL,
    withdrawn_by TEXT,
    withdrawn_at BIGINT,
    PRIMARY KEY (company_id, plugin_id, installation_version),
    CHECK ((withdrawn_by IS NULL) = (withdrawn_at IS NULL)),
    CHECK (withdrawn_at IS NULL OR withdrawn_at >= installed_at)
);

CREATE UNIQUE INDEX company_plugin_active_version
    ON company_plugin_installations (company_id, plugin_id)
    WHERE withdrawn_at IS NULL;
