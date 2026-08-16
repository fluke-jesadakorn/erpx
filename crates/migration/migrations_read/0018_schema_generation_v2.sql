-- Disposable control-read database generation marker.
CREATE SCHEMA erpx_meta;

CREATE TABLE erpx_meta.schema_manifest (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_generation SMALLINT NOT NULL CHECK (schema_generation = 2),
    manifest_version SMALLINT NOT NULL CHECK (manifest_version = 1),
    database_role TEXT NOT NULL CHECK (database_role = 'control_read'),
    migration_set TEXT NOT NULL CHECK (migration_set = 'control-read-v2'),
    installed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO erpx_meta.schema_manifest
    (singleton, schema_generation, manifest_version, database_role, migration_set)
VALUES (TRUE, 2, 1, 'control_read', 'control-read-v2');
