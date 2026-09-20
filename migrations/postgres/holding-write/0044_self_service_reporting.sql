-- Missing reporting data does not confer top-level approval privileges.
ALTER TABLE hr_engagements ADD COLUMN self_service_top_level BOOLEAN NOT NULL DEFAULT false;
