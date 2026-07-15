-- Folio — training courses + completion tracking for promotion gating.
--
-- Each course belongs to a target staff_level: completing the courses
-- required for level N is one of the five buckets in the My Progress
-- checklist (level / dept / tenure / training / manager signoff).
--
-- Seed: minimal but realistic coverage per level so the My Progress page
-- has something to render immediately. HR can add more courses via SQL;
-- the UI is read-only in v1.

BEGIN;

CREATE TABLE IF NOT EXISTS perm.training_courses (
  id              text PRIMARY KEY,
  display_name    text NOT NULL,
  description     text NOT NULL DEFAULT '',
  target_level    smallint NOT NULL CHECK (target_level BETWEEN 1 AND 5),
  sort_order      integer NOT NULL DEFAULT 0,
  is_system       boolean NOT NULL DEFAULT true,
  created_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS perm_training_courses_level_idx
  ON perm.training_courses (target_level, sort_order);

CREATE TABLE IF NOT EXISTS perm.training_completions (
  user_id      integer NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  course_id    text NOT NULL REFERENCES perm.training_courses(id) ON DELETE CASCADE,
  completed_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, course_id)
);

CREATE INDEX IF NOT EXISTS perm_training_completions_user_idx
  ON perm.training_completions (user_id);

DROP TRIGGER IF EXISTS perm_training_courses_touch ON perm.training_courses;
CREATE TRIGGER perm_training_courses_touch
  BEFORE UPDATE ON perm.training_courses
  FOR EACH ROW EXECUTE FUNCTION perm.touch_updated_at();

INSERT INTO perm.training_courses (id, display_name, description, target_level, sort_order)
VALUES
  -- L3 (Manager) prerequisites
  ('mgr-fundamentals',     'Management Fundamentals',       'Coaching, 1:1 cadence, performance reviews.',         3, 10),
  ('dept-budget',          'Department Budgeting',          'How to read a P&L, allocate headcount, track opex.',  3, 20),
  -- L2 (C-Level / IT / HQ) prerequisites
  ('leadership-301',       'Leadership 301',                'Cross-functional leadership, hiring loops.',          2, 10),
  ('governance-101',       'Governance & Compliance 101',   'Audit trails, SOX-equivalent controls.',              2, 20),
  ('exec-finance',         'Executive Finance',             'Cap table, runway, treasury ops.',                    2, 30),
  -- L1 (CEO) prerequisites
  ('board-comms',          'Board Communications',          'Quarterly board deck, investor updates.',             1, 10),
  ('strategy-401',         'Corporate Strategy 401',        'Long-range planning, M&A framework.',                 1, 20)
ON CONFLICT (id) DO NOTHING;

COMMIT;