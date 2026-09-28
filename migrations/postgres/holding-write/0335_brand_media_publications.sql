-- Brand pointers select only independently approved, sanitized derivatives.
-- Legacy attachment pointers remain for migration/audit compatibility but are
-- never used as public media while the canonical attachment gate is enabled.
ALTER TABLE organization_brand_profiles
  ADD COLUMN primary_publication_id TEXT REFERENCES attachment_publications(id),
  ADD COLUMN inverse_publication_id TEXT REFERENCES attachment_publications(id);

ALTER TABLE storefront_app_brands
  ADD COLUMN logo_publication_id TEXT REFERENCES attachment_publications(id);

CREATE INDEX organization_brand_primary_publication
  ON organization_brand_profiles(primary_publication_id)
  WHERE primary_publication_id IS NOT NULL;
CREATE INDEX organization_brand_inverse_publication
  ON organization_brand_profiles(inverse_publication_id)
  WHERE inverse_publication_id IS NOT NULL;
CREATE INDEX storefront_brand_logo_publication
  ON storefront_app_brands(logo_publication_id)
  WHERE logo_publication_id IS NOT NULL;
