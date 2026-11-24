import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
/** Canonical owner-local record-scope authority witness predicate.
 * Every read statement shares this one alias-parameterized witness so no
 * duplicate SQL literal remains. Callers supply source/authority aliases and
 * positional binds; the two COALESCE placeholders keep each caller's bind order. */
export const marketingRecordScopeAuthoritySql=(source:string,authority:string)=>`EXISTS(SELECT 1 FROM (SELECT ${source}.company_id,COALESCE(${source}.branch_id,?::text) branch_id,COALESCE(${source}.department_id,?::text) department_id) ${authority} WHERE ${assignmentReadSql(authority,'records.read')})`;
