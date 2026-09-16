
CREATE TABLE iam_roles(company_id TEXT NOT NULL,id TEXT NOT NULL,name TEXT NOT NULL,built_in BOOLEAN NOT NULL DEFAULT false,published_revision INTEGER NOT NULL DEFAULT 0,status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),PRIMARY KEY(company_id,id));
CREATE TABLE iam_role_revisions(company_id TEXT NOT NULL,role_id TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),permissions_json JSONB NOT NULL,grantable_json JSONB NOT NULL DEFAULT '[]',scopes_json JSONB NOT NULL,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,PRIMARY KEY(company_id,role_id,revision),FOREIGN KEY(company_id,role_id) REFERENCES iam_roles(company_id,id));
CREATE TABLE iam_permissions(id TEXT PRIMARY KEY,definition JSONB NOT NULL);
CREATE TABLE iam_members(company_id TEXT NOT NULL,user_id TEXT NOT NULL,principal_id TEXT NOT NULL,person_id TEXT,status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','suspended','superseded')),login_policy TEXT NOT NULL DEFAULT 'local' CHECK(login_policy IN ('local','directory','emergency')),connection_id TEXT,identity_id TEXT,version INTEGER NOT NULL DEFAULT 1,emergency_until BIGINT,PRIMARY KEY(company_id,user_id));
CREATE TABLE iam_connections(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,name TEXT NOT NULL,provider TEXT NOT NULL CHECK(provider IN ('active_directory','entra_id','univention')),config JSONB NOT NULL,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','tested','active','disabled')),version INTEGER NOT NULL DEFAULT 1,tested_version INTEGER,last_sync_at BIGINT,health TEXT NOT NULL DEFAULT 'untested',last_error TEXT,created_by TEXT NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE iam_identities(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,connection_id TEXT NOT NULL REFERENCES iam_connections(id),external_id TEXT NOT NULL,username TEXT NOT NULL,name TEXT NOT NULL,email TEXT,enabled BOOLEAN NOT NULL,groups_json JSONB NOT NULL DEFAULT '[]',observed_at BIGINT NOT NULL,user_id TEXT,person_id TEXT,UNIQUE(connection_id,external_id));
CREATE TABLE iam_connection_previews(connection_id TEXT PRIMARY KEY REFERENCES iam_connections(id),version INTEGER NOT NULL,snapshot_json JSONB NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE iam_oidc_subjects(connection_id TEXT NOT NULL,issuer TEXT NOT NULL,subject TEXT NOT NULL,external_id TEXT NOT NULL,PRIMARY KEY(connection_id,issuer,subject),UNIQUE(connection_id,external_id));
CREATE TABLE iam_groups(connection_id TEXT NOT NULL REFERENCES iam_connections(id),external_id TEXT NOT NULL,name TEXT NOT NULL,observed_at BIGINT NOT NULL,PRIMARY KEY(connection_id,external_id));
CREATE TABLE iam_mappings(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,connection_id TEXT NOT NULL REFERENCES iam_connections(id),group_id TEXT NOT NULL,role_id TEXT NOT NULL,scope_kind TEXT NOT NULL,scope_node_id TEXT,valid_until BIGINT NOT NULL,parent_id TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 1,status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','revoked')));
CREATE TABLE iam_reviews(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,kind TEXT NOT NULL,payload JSONB NOT NULL,created_by TEXT NOT NULL,created_person TEXT NOT NULL,created_at BIGINT NOT NULL,status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','failed')),version INTEGER NOT NULL DEFAULT 1,reviewed_by TEXT,reviewed_at BIGINT,reason TEXT NOT NULL);
CREATE TABLE iam_migrations(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,connection_id TEXT NOT NULL REFERENCES iam_connections(id),users_json JSONB NOT NULL,status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','awaiting_review','applying','complete','failed')),created_by TEXT NOT NULL,version INTEGER NOT NULL DEFAULT 1,created_at BIGINT NOT NULL);
CREATE TABLE iam_audit(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,actor_id TEXT NOT NULL,assignment_id TEXT NOT NULL,action TEXT NOT NULL,target_id TEXT NOT NULL,evidence JSONB NOT NULL,created_at BIGINT NOT NULL);
CREATE INDEX iam_audit_company_time ON iam_audit(company_id,created_at DESC);
CREATE TABLE iam_receipts(id TEXT PRIMARY KEY,company_id TEXT NOT NULL,actor_id TEXT NOT NULL,request_hash TEXT NOT NULL,result JSONB NOT NULL,created_at BIGINT NOT NULL);
CREATE TABLE iam_revoked_sessions(session_id TEXT NOT NULL,company_id TEXT NOT NULL,user_id TEXT NOT NULL,revoked_at BIGINT NOT NULL,PRIMARY KEY(session_id,company_id));
CREATE TABLE iam_emergency_sessions(session_id TEXT NOT NULL,company_id TEXT NOT NULL,user_id TEXT NOT NULL,expires_at BIGINT NOT NULL,reason TEXT NOT NULL,PRIMARY KEY(session_id,company_id));
CREATE TABLE iam_sync_jobs(connection_id TEXT PRIMARY KEY REFERENCES iam_connections(id),next_run_at BIGINT NOT NULL,lease_until BIGINT,lease_token TEXT,attempts INTEGER NOT NULL DEFAULT 0);
CREATE OR REPLACE FUNCTION iam_role_allows(role_key TEXT,company_key TEXT,permission_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=role_key AND r.company_id IN ('*',company_key) AND r.status='active' AND v.permissions_json @> jsonb_build_array(permission_key)); $$;
CREATE OR REPLACE FUNCTION iam_principal_active(user_key TEXT,company_key TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id WHERE m.user_id=user_key AND m.company_id=company_key AND (m.status!='active' OR m.login_policy='directory' AND (i.enabled IS DISTINCT FROM true OR c.status IS DISTINCT FROM 'active' OR COALESCE(i.observed_at,0)<=at_time-300000))); $$;
CREATE OR REPLACE FUNCTION iam_same_person(a TEXT,b TEXT,company_key TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT a=b OR EXISTS(SELECT 1 FROM iam_members x JOIN iam_members y ON y.company_id=x.company_id AND y.person_id=x.person_id WHERE x.company_id=company_key AND x.user_id=a AND y.user_id=b AND x.person_id IS NOT NULL); $$;
CREATE OR REPLACE FUNCTION iam_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Immutable IAM evidence'; END $$;
CREATE TRIGGER iam_revision_immutable BEFORE UPDATE OR DELETE ON iam_role_revisions FOR EACH ROW EXECUTE FUNCTION iam_immutable();
CREATE TRIGGER iam_audit_immutable BEFORE UPDATE OR DELETE ON iam_audit FOR EACH ROW EXECUTE FUNCTION iam_immutable();
INSERT INTO iam_permissions VALUES('platform.iam.read','{"id":"platform.iam.read","label":"View identities and access","module":"identity_access","roles":["platform_admin","platform_auditor"],"conditions":[],"risk":"normal","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.manage','{"id":"platform.iam.manage","label":"Manage identities and assignments","module":"identity_access","roles":["platform_admin"],"conditions":[],"risk":"privileged","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.roles.manage','{"id":"platform.iam.roles.manage","label":"Create and revise custom roles","module":"identity_access","roles":["platform_admin"],"conditions":[],"risk":"privileged","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.directory.manage','{"id":"platform.iam.directory.manage","label":"Manage directory connections","module":"identity_access","roles":["platform_admin"],"conditions":[],"risk":"privileged","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.review','{"id":"platform.iam.review","label":"Review sensitive access changes","module":"identity_access","roles":["platform_admin"],"conditions":["Independent reviewer and fresh MFA"],"risk":"privileged","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.export','{"id":"platform.iam.export","label":"Export access evidence","module":"identity_access","roles":["platform_admin","platform_auditor"],"conditions":[],"risk":"privileged","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.recovery','{"id":"platform.iam.recovery","label":"Manage emergency access","module":"identity_access","roles":["platform_admin"],"conditions":["Fresh MFA and independent review"],"risk":"privileged","availability":"live","namespace":"platform"}');
INSERT INTO iam_permissions VALUES('platform.iam.self','{"id":"platform.iam.self","label":"View own access and sessions","module":"identity_access","roles":["platform_admin","platform_auditor"],"conditions":[],"risk":"normal","availability":"live","namespace":"platform","identityBased":true}');
INSERT INTO iam_roles VALUES('*','platform_admin','platform admin',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','platform_admin',1,'["platform.iam.read","platform.iam.manage","platform.iam.roles.manage","platform.iam.directory.manage","platform.iam.review","platform.iam.export","platform.iam.recovery","platform.iam.self"]','["platform.iam.read","platform.iam.manage","platform.iam.roles.manage","platform.iam.directory.manage","platform.iam.review","platform.iam.export","platform.iam.recovery","platform.iam.self"]','["platform"]','system',0);
INSERT INTO iam_roles VALUES('*','platform_auditor','platform auditor',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','platform_auditor',1,'["platform.iam.read","platform.iam.export","platform.iam.self"]','[]','["platform"]','system',0);
ALTER TABLE platform_assignments ADD COLUMN parent_id TEXT REFERENCES platform_assignments(id), ADD COLUMN source TEXT NOT NULL DEFAULT 'legacy' CHECK(source IN ('legacy','manual','directory','emergency')), ADD COLUMN source_id TEXT;
CREATE OR REPLACE FUNCTION iam_assignment_active(grant_id TEXT,at_time BIGINT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 WITH RECURSIVE chain AS (
 SELECT a.id,a.user_id,a.role,'platform'::text AS company_id,a.parent_id,a.revoked_at,a.valid_from,a.valid_until,ARRAY[a.id] AS path,false AS cycle FROM platform_assignments a WHERE a.id=grant_id
 UNION ALL SELECT p.id,p.user_id,p.role,'platform'::text,p.parent_id,p.revoked_at,p.valid_from,p.valid_until,c.path||p.id,p.id=ANY(c.path) FROM platform_assignments p JOIN chain c ON p.id=c.parent_id WHERE NOT c.cycle AND cardinality(c.path)<32)
 SELECT EXISTS(SELECT 1 FROM chain) AND NOT EXISTS(SELECT 1 FROM chain WHERE revoked_at IS NOT NULL OR valid_from>at_time OR valid_until<=at_time OR cycle OR cardinality(path)>=32 OR NOT iam_principal_active(user_id,company_id,at_time))
 AND EXISTS(SELECT 1 FROM chain WHERE parent_id IS NULL)
 AND NOT EXISTS(SELECT 1 FROM chain ch JOIN chain p ON p.id=ch.parent_id
 LEFT JOIN iam_roles cr ON cr.id=ch.role AND cr.company_id IN ('*',ch.company_id) AND cr.status='active'
 LEFT JOIN iam_role_revisions cv ON cv.company_id=cr.company_id AND cv.role_id=cr.id AND cv.revision=cr.published_revision
 LEFT JOIN iam_roles pr ON pr.id=p.role AND pr.company_id IN ('*',p.company_id) AND pr.status='active'
 LEFT JOIN iam_role_revisions pv ON pv.company_id=pr.company_id AND pv.role_id=pr.id AND pv.revision=pr.published_revision
 WHERE ch.company_id!=p.company_id OR ch.valid_until>p.valid_until OR cv.revision IS NULL OR pv.revision IS NULL OR NOT ((cv.permissions_json||cv.grantable_json)<@pv.grantable_json)); $$;
 CREATE OR REPLACE FUNCTION iam_parent_guard() RETURNS trigger LANGUAGE plpgsql AS $$
 DECLARE p RECORD; cv JSONB; cg JSONB; pg JSONB;
 BEGIN
 IF NEW.parent_id IS NOT NULL THEN
 SELECT * INTO p FROM platform_assignments WHERE id=NEW.parent_id FOR SHARE;
 IF NOT FOUND OR p.id=NEW.id OR NOT iam_assignment_active(p.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) OR NEW.valid_until>p.valid_until OR NEW.valid_from<p.valid_from THEN RAISE EXCEPTION 'Invalid authorizing assignment' USING ERRCODE='23514'; END IF;
 
 SELECT v.permissions_json,v.grantable_json INTO cv,cg FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=NEW.role AND r.company_id IN ('*','platform');
 SELECT v.grantable_json INTO pg FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id=p.role AND r.company_id IN ('*','platform');
 IF cv IS NULL OR pg IS NULL OR NOT ((cv||cg)<@pg) THEN RAISE EXCEPTION 'Delegated permission ceiling exceeded' USING ERRCODE='23514'; END IF;
 END IF; RETURN NEW; END $$;
 CREATE TRIGGER iam_parent_scope BEFORE INSERT OR UPDATE OF parent_id,role,valid_from,valid_until ON platform_assignments FOR EACH ROW EXECUTE FUNCTION iam_parent_guard();
 
CREATE OR REPLACE FUNCTION iam_recovery_protect() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF OLD.revoked_at IS NULL AND (NEW.revoked_at IS NOT NULL OR NEW.role!=OLD.role OR NEW.valid_until<OLD.valid_until)
 AND EXISTS(SELECT 1 FROM iam_members m WHERE m.user_id=OLD.user_id AND m.company_id='platform' AND m.login_policy='emergency' AND m.status='active')
 AND NOT EXISTS(SELECT 1 FROM platform_assignments a JOIN iam_members m ON m.user_id=a.user_id AND m.company_id='platform' WHERE a.id!=OLD.id AND m.company_id='platform' AND m.login_policy='emergency' AND m.status='active' AND a.role='platform_admin' AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))
 THEN RAISE EXCEPTION 'The final recovery administrator must remain available' USING ERRCODE='23514'; END IF; RETURN NEW; END $$;
 CREATE TRIGGER iam_protect_recovery BEFORE UPDATE ON platform_assignments FOR EACH ROW EXECUTE FUNCTION iam_recovery_protect();
 
CREATE TABLE iam_mfa(user_id TEXT PRIMARY KEY REFERENCES "user"(id),secret TEXT NOT NULL,enabled BOOLEAN NOT NULL DEFAULT false,last_counter BIGINT NOT NULL DEFAULT -1);
 CREATE TABLE iam_mfa_sessions(session_id TEXT PRIMARY KEY REFERENCES session(id) ON DELETE CASCADE,user_id TEXT NOT NULL,verified_at BIGINT NOT NULL);
 CREATE TABLE iam_connection_routes(connection_id TEXT PRIMARY KEY,holding_id TEXT,company_id TEXT NOT NULL);
 CREATE TABLE iam_throttles(key TEXT PRIMARY KEY,window_start BIGINT NOT NULL,attempts INTEGER NOT NULL);
 CREATE TABLE iam_login_attempts(id TEXT PRIMARY KEY,connection_id TEXT NOT NULL,expires_at BIGINT NOT NULL,state_json JSONB NOT NULL,consumed_at BIGINT);
 