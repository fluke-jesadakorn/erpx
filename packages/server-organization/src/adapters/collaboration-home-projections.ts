export function socialPersonOrganizationProjection(){return 'EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=a.scope_node_id AND n.company_id=a.company_id AND n.branch_id=?)';}
export function socialOrganizationAudienceName(){return '(SELECT n.display_name FROM organization_nodes n WHERE n.id=COALESCE(p.department_id,p.branch_id) AND n.company_id=p.company_id)';}
