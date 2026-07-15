ALTER TABLE users
  ADD COLUMN IF NOT EXISTS secondary_locale text NOT NULL DEFAULT 'th'
  CHECK (secondary_locale IN ('th','de'));

ALTER TABLE auth.sessions
  ADD COLUMN IF NOT EXISTS locale text NOT NULL DEFAULT 'th'
  CHECK (locale IN ('th','de'));

UPDATE auth.sessions s
   SET locale = u.secondary_locale
  FROM users u
 WHERE s.user_id = u.id
   AND s.locale <> u.secondary_locale;