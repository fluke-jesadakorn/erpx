-- Work contact details are explicitly public within a Holding. Existing
-- profile email and phone remain private until an HR editor sets these fields.
ALTER TABLE hr_people ADD COLUMN work_email TEXT;
ALTER TABLE hr_people ADD COLUMN work_phone TEXT;

ALTER TABLE hr_people ADD CONSTRAINT hr_people_work_email_length
  CHECK (work_email IS NULL OR length(work_email) <= 320);
ALTER TABLE hr_people ADD CONSTRAINT hr_people_work_phone_length
  CHECK (work_phone IS NULL OR length(work_phone) <= 60);
