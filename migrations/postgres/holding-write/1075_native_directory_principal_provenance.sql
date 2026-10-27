-- Add current Directory provenance without changing the exact1072 People,
-- external relationship, invitation, legacy Directory or lifecycle predicates.
-- sha256(bytea) is a PostgreSQL18 core function; no extension or secret digest.
CREATE FUNCTION iam_directory_observation_current(user_key text,company_key text,at_time bigint)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT EXISTS(
  SELECT 1 FROM iam_members m
  WHERE m.user_id=user_key AND m.company_id=company_key AND m.login_policy='directory'
  AND NOT EXISTS(
   SELECT 1 FROM iam_identities i
   JOIN iam_connections c ON c.id=i.connection_id AND c.company_id=i.company_id
   JOIN iam_directory_source_selections s ON s.connection_id=c.id AND s.company_id=c.company_id
   JOIN iam_directory_observation_sources o ON o.connection_id=s.connection_id AND o.company_id=s.company_id
   WHERE i.id=m.identity_id AND i.user_id=m.user_id AND i.company_id=m.company_id AND i.connection_id=m.connection_id
    AND c.status='active' AND c.config->>'mode'='ldaps'
    AND s.format_version=1 AND o.format_version=s.format_version
    AND s.connection_version=c.version AND o.connection_version=s.connection_version
    AND s.config_fingerprint=CASE WHEN octet_length(c.config::text)<=16384 THEN sha256(convert_to(c.config::text,'UTF8')) END
    AND o.config_fingerprint=s.config_fingerprint AND o.source_revision=s.revision
    AND o.key_id=s.key_id AND o.profile_tag=s.profile_tag
    AND i.observed_at=o.observed_at AND o.observed_at<=at_time AND o.observed_at>at_time-300000
  )
 );
$$;

CREATE OR REPLACE FUNCTION iam_principal_active(user_key text,company_key text,at_time bigint) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT iam_principal_active_before_external(user_key,company_key,at_time)
 AND NOT EXISTS(SELECT 1 FROM iam_access_subjects s WHERE s.company_id=company_key
   AND (s.user_id=user_key OR s.person_id IS NOT NULL AND EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=company_key AND m.user_id=user_key AND m.person_id=s.person_id))
   AND (s.status!='active' OR s.kind='external' AND s.expires_at IS NOT NULL AND s.expires_at<=at_time
     OR EXISTS(SELECT 1 FROM iam_access_invitations i WHERE i.company_id=s.company_id AND i.subject_id=s.id AND i.state='authorized')))
 AND iam_directory_observation_current(user_key,company_key,at_time);
$$;
