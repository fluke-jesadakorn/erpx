import {scopeNodeBranchExists} from './organization-read-tail-sources-r39';
export function socialPersonOrganizationProjection(){return scopeNodeBranchExists('?');}
export function socialOrganizationAudienceName(){return '(SELECT n.display_name FROM organization_nodes n WHERE n.id=COALESCE(p.department_id,p.branch_id) AND n.company_id=p.company_id)';}