import {scopeNodeBranchExists} from './organization-read-tail-sources-r39';
export function socialDepartmentBranchSql(alias: string) { return scopeNodeBranchExists(`${alias}.branch_id`); }