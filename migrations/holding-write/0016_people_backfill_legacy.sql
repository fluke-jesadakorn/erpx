INSERT OR IGNORE INTO hr_people(id,user_id,company_id,legal_name,preferred_name,country_code,status,version,created_by,created_at,updated_at)
SELECT 'person-' || id,user_id,company_id,display_name,display_name,'TH',CASE WHEN status='active' THEN 'active' ELSE 'inactive' END,1,'system',created_at,created_at
FROM employees;

INSERT OR IGNORE INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)
SELECT 'eng-' || e.id,'person-' || e.id,e.company_id,e.branch_id,e.department_id,'employee','salaried',CASE WHEN e.status='active' THEN 'active' ELSE 'ended' END,date('now'),'Employee','USD','system',e.created_at,e.created_at
FROM employees e;
