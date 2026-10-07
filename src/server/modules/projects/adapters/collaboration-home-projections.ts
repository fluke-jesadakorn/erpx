export function socialProjectAudienceName(){return '(SELECT pr.title FROM staff_projects pr WHERE pr.id=p.audience_id AND pr.company_id=p.company_id)';}
