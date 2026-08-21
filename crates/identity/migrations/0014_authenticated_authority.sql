-- Realm and principal kind are immutable authentication facts. Legacy ERPX
-- sessions predate non-human authentication and are therefore backfilled as
-- human workforce sessions; protected Platform Root and the gated local
-- development controller retain platform-administration attribution.
ALTER TABLE erpx_identity.principals
    ADD COLUMN IF NOT EXISTS authenticated_realm TEXT;
ALTER TABLE erpx_identity.principals
    ADD COLUMN IF NOT EXISTS principal_kind TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM erpx_identity.principals
        WHERE (authenticated_realm IS NULL) <> (principal_kind IS NULL)
    ) THEN
        RAISE EXCEPTION 'principal authenticated authority is incomplete';
    END IF;
END;
$$;

UPDATE erpx_identity.principals
SET authenticated_realm = CASE
        WHEN principal_ref = 'local:dev-controller'
          OR EXISTS (
              SELECT 1
              FROM erpx_identity.protected_local_principals protected
              WHERE protected.principal_id = erpx_identity.principals.id
                AND protected.protection_kind = 'platform_root'
          )
        THEN 'platform_administration'
        ELSE 'internal_workforce'
    END,
    principal_kind = 'human'
WHERE authenticated_realm IS NULL AND principal_kind IS NULL;

ALTER TABLE erpx_identity.principals
    ALTER COLUMN authenticated_realm SET NOT NULL;
ALTER TABLE erpx_identity.principals
    ALTER COLUMN principal_kind SET NOT NULL;

ALTER TABLE erpx_identity.principals
    DROP CONSTRAINT IF EXISTS principals_authenticated_realm_check;
ALTER TABLE erpx_identity.principals
    ADD CONSTRAINT principals_authenticated_realm_check CHECK (
        authenticated_realm IN (
            'internal_workforce',
            'platform_administration',
            'customer_portal',
            'supplier_portal',
            'commerce_portal',
            'candidate_portal',
            'signatory_portal'
        )
    );
ALTER TABLE erpx_identity.principals
    DROP CONSTRAINT IF EXISTS principals_principal_kind_check;
ALTER TABLE erpx_identity.principals
    ADD CONSTRAINT principals_principal_kind_check
    CHECK (principal_kind IN ('human', 'agent', 'service'));
ALTER TABLE erpx_identity.principals
    DROP CONSTRAINT IF EXISTS principals_portal_human_check;
ALTER TABLE erpx_identity.principals
    ADD CONSTRAINT principals_portal_human_check CHECK (
        authenticated_realm NOT IN (
            'customer_portal',
            'supplier_portal',
            'commerce_portal',
            'candidate_portal',
            'signatory_portal'
        ) OR principal_kind = 'human'
    );

ALTER TABLE erpx_identity.sessions
    ADD COLUMN IF NOT EXISTS authenticated_realm TEXT;
ALTER TABLE erpx_identity.sessions
    ADD COLUMN IF NOT EXISTS principal_kind TEXT;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM erpx_identity.sessions
        WHERE (authenticated_realm IS NULL) <> (principal_kind IS NULL)
    ) THEN
        RAISE EXCEPTION 'session authenticated authority is incomplete';
    END IF;
END;
$$;

-- Every session created before this migration authenticated as the legacy
-- internal-workforce human authority. If its principal is now attributed to a
-- different realm or kind, the refresh bearer must not silently inherit that
-- stronger/different authority. Fence only rows whose session authority has
-- not been populated yet, so rerunning the additive migration never revokes a
-- session that was legitimately issued after the authority rollout.
-- BEGIN LEGACY AUTHORITY CHANGE FENCE
UPDATE erpx_identity.sessions AS identity_session
SET revoked_at = COALESCE(identity_session.revoked_at, CURRENT_TIMESTAMP),
    last_used_at = GREATEST(
        identity_session.last_used_at,
        COALESCE(identity_session.revoked_at, CURRENT_TIMESTAMP)
    )
FROM erpx_identity.principals principal
WHERE identity_session.principal_id = principal.id
  AND identity_session.authenticated_realm IS NULL
  AND identity_session.principal_kind IS NULL
  AND (
      principal.authenticated_realm <> 'internal_workforce'
      OR principal.principal_kind <> 'human'
  );
-- END LEGACY AUTHORITY CHANGE FENCE

UPDATE erpx_identity.sessions AS identity_session
SET authenticated_realm = principal.authenticated_realm,
    principal_kind = principal.principal_kind
FROM erpx_identity.principals principal
WHERE identity_session.principal_id = principal.id
  AND identity_session.authenticated_realm IS NULL
  AND identity_session.principal_kind IS NULL;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM erpx_identity.sessions identity_session
        JOIN erpx_identity.principals principal
          ON principal.id = identity_session.principal_id
        WHERE identity_session.authenticated_realm <> principal.authenticated_realm
           OR identity_session.principal_kind <> principal.principal_kind
    ) THEN
        RAISE EXCEPTION 'session authenticated authority contradicts its principal';
    END IF;
END;
$$;

ALTER TABLE erpx_identity.sessions
    ALTER COLUMN authenticated_realm SET NOT NULL;
ALTER TABLE erpx_identity.sessions
    ALTER COLUMN principal_kind SET NOT NULL;

ALTER TABLE erpx_identity.sessions
    DROP CONSTRAINT IF EXISTS sessions_authenticated_realm_check;
ALTER TABLE erpx_identity.sessions
    ADD CONSTRAINT sessions_authenticated_realm_check CHECK (
        authenticated_realm IN (
            'internal_workforce',
            'platform_administration',
            'customer_portal',
            'supplier_portal',
            'commerce_portal',
            'candidate_portal',
            'signatory_portal'
        )
    );
ALTER TABLE erpx_identity.sessions
    DROP CONSTRAINT IF EXISTS sessions_principal_kind_check;
ALTER TABLE erpx_identity.sessions
    ADD CONSTRAINT sessions_principal_kind_check
    CHECK (principal_kind IN ('human', 'agent', 'service'));
ALTER TABLE erpx_identity.sessions
    DROP CONSTRAINT IF EXISTS sessions_portal_human_check;
ALTER TABLE erpx_identity.sessions
    ADD CONSTRAINT sessions_portal_human_check CHECK (
        authenticated_realm NOT IN (
            'customer_portal',
            'supplier_portal',
            'commerce_portal',
            'candidate_portal',
            'signatory_portal'
        ) OR principal_kind = 'human'
    );
