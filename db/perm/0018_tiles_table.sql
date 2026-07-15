-- Migrate rbac.tiles → perm.tiles.
--
-- perm.tiles becomes the canonical tile catalog. Drops rbac.tiles-specific columns:
--   - module_id     (legacy cell-grid model)
--   - default_perm  (cell-grid model)
--
-- Keeps group_name as plain text (UI grouping like 'hr', 'finance') rather than
-- a FK to rbac.groups (which represents departments).

CREATE TABLE IF NOT EXISTS perm.tiles (
  id             text PRIMARY KEY,
  display_name   text NOT NULL,
  subtitle       text NOT NULL DEFAULT '',
  icon           text NOT NULL DEFAULT '🧾',
  accent         text NOT NULL DEFAULT 'slate',
  group_name     text NOT NULL,
  sub_view       text,
  href           text NOT NULL,
  request_target text,
  sort_order     integer NOT NULL DEFAULT 0,
  is_system      boolean NOT NULL DEFAULT true,
  owner_group_id text,
  required_level smallint CHECK (required_level IS NULL OR required_level BETWEEN 1 AND 5),
  required_dept_id text REFERENCES perm.roles(id) ON DELETE RESTRICT,
  created_at     timestamptz NOT NULL DEFAULT now(),
  updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS perm_tiles_group_idx ON perm.tiles (group_name, sort_order);
CREATE INDEX IF NOT EXISTS perm_tiles_required_dept_idx ON perm.tiles (required_dept_id);

INSERT INTO perm.tiles (
  id, display_name, subtitle, icon, accent, group_name, sub_view, href,
  request_target, sort_order, is_system, owner_group_id
)
SELECT id, display_name, subtitle, icon, accent, group_name, sub_view, href,
       request_target, sort_order, is_system, owner_group_id
  FROM rbac.tiles
ON CONFLICT (id) DO NOTHING;

-- Trigger to maintain updated_at on UPDATE
CREATE OR REPLACE FUNCTION perm.touch_updated_at() RETURNS trigger AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS perm_tiles_touch ON perm.tiles;
CREATE TRIGGER perm_tiles_touch BEFORE UPDATE ON perm.tiles
  FOR EACH ROW EXECUTE FUNCTION perm.touch_updated_at();
