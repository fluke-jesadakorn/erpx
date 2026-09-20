ALTER TABLE employee_requests ADD COLUMN engagement_id TEXT REFERENCES hr_engagements(id);
-- Bind only unambiguous historical links; never invent a past employment relationship.
UPDATE employee_requests r SET engagement_id=e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
 WHERE r.company_id=e.company_id AND r.requester_user_id=p.user_id AND e.start_date<=to_char(to_timestamp(r.created_at/1000.0),'YYYY-MM-DD')
 AND (e.end_date IS NULL OR e.end_date>=to_char(to_timestamp(r.created_at/1000.0),'YYYY-MM-DD'))
 AND 1=(SELECT count(*) FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=r.company_id AND x.start_date<=to_char(to_timestamp(r.created_at/1000.0),'YYYY-MM-DD') AND (x.end_date IS NULL OR x.end_date>=to_char(to_timestamp(r.created_at/1000.0),'YYYY-MM-DD')));
ALTER TABLE leave_policy_assignments ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
