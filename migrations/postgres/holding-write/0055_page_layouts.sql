CREATE TABLE page_layouts (
 company_id text NOT NULL, owner_id text NOT NULL, workspace_base text NOT NULL, page_key text NOT NULL,
 layout_json text, version bigint NOT NULL CHECK(version>0), updated_at bigint NOT NULL,
 PRIMARY KEY(company_id,owner_id,workspace_base,page_key)
);
CREATE TABLE page_layout_receipts (
 id text PRIMARY KEY, request_hash text NOT NULL, result_json text NOT NULL,
 actor_id text NOT NULL, company_id text NOT NULL, workspace_base text NOT NULL,
 page_key text NOT NULL, action text NOT NULL, created_at bigint NOT NULL
);
CREATE INDEX page_layout_audit ON page_layout_receipts(company_id,page_key,created_at);
