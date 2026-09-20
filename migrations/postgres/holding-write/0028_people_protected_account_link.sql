-- The HR primary account is another verified-link path; protect it like iam_members.
CREATE FUNCTION people_protect_scheduled_hr_account() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.user_id IS NOT NULL
 AND EXISTS(SELECT 1 FROM people_access_cutoffs x WHERE x.company_id=NEW.company_id AND x.person_id=NEW.id AND x.cancelled_at IS NULL AND x.restored_at IS NULL)
 AND (EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=NEW.company_id AND a.user_id=NEW.user_id AND a.role='holding_owner' AND a.revoked_at IS NULL)
   OR EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=NEW.company_id AND m.user_id=NEW.user_id AND m.login_policy='emergency')) THEN
  RAISE EXCEPTION 'Cancel the scheduled departure before linking protected administration' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE TRIGGER people_protected_hr_account BEFORE INSERT OR UPDATE OF user_id ON hr_people FOR EACH ROW EXECUTE FUNCTION people_protect_scheduled_hr_account();
