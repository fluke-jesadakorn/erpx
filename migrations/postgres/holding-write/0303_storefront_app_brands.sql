CREATE TABLE storefront_app_brands (
  company_id TEXT PRIMARY KEY REFERENCES organization_nodes(id),
  short_name TEXT NOT NULL,
  accent_color TEXT NOT NULL DEFAULT '#3659c9',
  logo_attachment_id TEXT REFERENCES attachments(id),
  updated_by TEXT NOT NULL,
  updated_at BIGINT NOT NULL,
  CHECK (length(btrim(short_name)) BETWEEN 1 AND 40),
  CHECK (accent_color ~ '^#[0-9A-Fa-f]{6}$')
);

CREATE INDEX storefront_app_brand_logo ON storefront_app_brands(logo_attachment_id) WHERE logo_attachment_id IS NOT NULL;
