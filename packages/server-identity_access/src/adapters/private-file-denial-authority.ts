import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {privateFileDenialOrganizationWitness} from '@erpx/server-organization/public/private-file-denial-sources';
import {privateFileDenialPeopleWitness} from '@erpx/server-people_payroll/public/private-file-denial-sources';

/** Add to the owner's query-free lease batch before lockQueuedCommandAuthority.
 * Its existing Identity/Organization/People leases cover every other fact. */
export function privateFileDenialAuthoritySourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE iam_permissions IN SHARE MODE')];}
/** Opaque equality witness only. In particular, an unchanged invalid principal
 * stays invalid: this records the original rejection, never grants bytes. The
 * original passed context is hashed alongside raw native facts without time-
 * dependent decisions, so expiry cannot become fabricated source mutation. */
export async function privateFileDenialAuthorityWitness(database:SqlDatabase,actor:WorkContext):Promise<string>{
 const image=await database.prepare(`WITH RECURSIVE denial_scope AS(SELECT ?::text company_id,?::text user_id,?::text assignment_id,?::text role_id,?::text session_id),
 chain AS(SELECT assignment.*,ARRAY[assignment.id] visited FROM assignments assignment CROSS JOIN denial_scope WHERE assignment.id=denial_scope.assignment_id
 UNION ALL SELECT parent.*,child.visited||parent.id FROM assignments parent JOIN chain child ON parent.id=child.parent_id WHERE NOT parent.id=ANY(child.visited) AND cardinality(child.visited)<32),
 principals AS(SELECT company_id,user_id FROM chain UNION SELECT company_id,user_id FROM denial_scope),
 members AS(SELECT member.* FROM iam_members member WHERE EXISTS(SELECT 1 FROM principals principal WHERE principal.company_id=member.company_id AND (principal.user_id=member.user_id OR member.person_id IS NOT NULL AND EXISTS(SELECT 1 FROM iam_members selected WHERE selected.company_id=principal.company_id AND selected.user_id=principal.user_id AND selected.person_id=member.person_id)))),
 roles AS(SELECT role.* FROM iam_roles role WHERE EXISTS(SELECT 1 FROM chain WHERE chain.role=role.id AND role.company_id IN ('*',chain.company_id)) OR EXISTS(SELECT 1 FROM denial_scope WHERE role.id=denial_scope.role_id AND role.company_id IN ('*',denial_scope.company_id)))
 SELECT jsonb_build_object(
 'chain',(SELECT jsonb_agg(to_jsonb(chain) ORDER BY chain.id) FROM chain),
 'members',(SELECT jsonb_agg(to_jsonb(member) ORDER BY member.company_id,member.user_id) FROM members member),
 'identities',(SELECT jsonb_agg(to_jsonb(identity_row) ORDER BY identity_row.id) FROM iam_identities identity_row WHERE EXISTS(SELECT 1 FROM members member WHERE member.identity_id=identity_row.id AND member.company_id=identity_row.company_id)),
 'connections',(SELECT jsonb_agg(to_jsonb(connection) ORDER BY connection.id) FROM iam_connections connection WHERE EXISTS(SELECT 1 FROM members member WHERE member.connection_id=connection.id AND member.company_id=connection.company_id)),
 'roles',(SELECT jsonb_agg(to_jsonb(role) ORDER BY role.company_id,role.id) FROM roles role),
 'revisions',(SELECT jsonb_agg(to_jsonb(revision) ORDER BY revision.company_id,revision.role_id,revision.revision) FROM iam_role_revisions revision JOIN roles role ON role.company_id=revision.company_id AND role.id=revision.role_id AND role.published_revision=revision.revision),
 'permissions',(SELECT jsonb_agg(to_jsonb(permission) ORDER BY permission.id) FROM iam_permissions permission),
 'revocations',(SELECT jsonb_agg(to_jsonb(revocation) ORDER BY to_jsonb(revocation)::text) FROM iam_revoked_sessions revocation WHERE revocation.company_id=denial_scope.company_id AND revocation.user_id=denial_scope.user_id AND (denial_scope.session_id IS NULL OR revocation.session_id=denial_scope.session_id)),
 'emergency',(SELECT jsonb_agg(to_jsonb(emergency) ORDER BY to_jsonb(emergency)::text) FROM iam_emergency_sessions emergency WHERE emergency.company_id=denial_scope.company_id AND emergency.user_id=denial_scope.user_id AND (denial_scope.session_id IS NULL OR emergency.session_id=denial_scope.session_id)),
 'contexts',(SELECT jsonb_agg(to_jsonb(context_row) ORDER BY context_row.session_id) FROM active_work_contexts context_row WHERE context_row.company_id=denial_scope.company_id AND context_row.user_id=denial_scope.user_id AND (denial_scope.session_id IS NULL OR context_row.session_id=denial_scope.session_id)),
 'subjects',(SELECT jsonb_agg(to_jsonb(subject) ORDER BY to_jsonb(subject)::text) FROM iam_access_subjects subject WHERE subject.company_id=denial_scope.company_id AND (subject.user_id=denial_scope.user_id OR subject.person_id IN(SELECT member.person_id FROM members member WHERE member.company_id=subject.company_id))),
 'attributes',(SELECT jsonb_agg(to_jsonb(attribute) ORDER BY attribute.id) FROM um_attributes attribute WHERE attribute.company_id=denial_scope.company_id),
 'values',(SELECT jsonb_agg(to_jsonb(value_row) ORDER BY value_row.target,value_row.object_id) FROM um_values value_row WHERE value_row.company_id=denial_scope.company_id),
 'groups',(SELECT jsonb_agg(to_jsonb(group_row) ORDER BY group_row.id) FROM um_groups group_row WHERE group_row.company_id=denial_scope.company_id),
 'memberships',(SELECT jsonb_agg(to_jsonb(membership) ORDER BY membership.group_id,membership.person_id) FROM um_memberships membership WHERE membership.company_id=denial_scope.company_id),
 'policies',(SELECT jsonb_agg(to_jsonb(policy) ORDER BY policy.id) FROM um_policies policy WHERE policy.company_id=denial_scope.company_id),
 'attachments',(SELECT jsonb_agg(to_jsonb(attachment) ORDER BY attachment.id) FROM um_attachments attachment WHERE attachment.company_id=denial_scope.company_id),
 'candidates',(SELECT jsonb_agg(to_jsonb(candidate) ORDER BY candidate.id) FROM um_candidates candidate WHERE candidate.company_id=denial_scope.company_id),
 'organization',${privateFileDenialOrganizationWitness},'people',${privateFileDenialPeopleWitness})::text image FROM denial_scope`).bind(actor.companyId,actor.userId,actor.assignmentId,actor.role,actor.authority?.sessionId??null).first<string>('image');
 if(!image)throw Error('PRIVATE_FILE_DENIAL_AUTHORITY_WITNESS_REQUIRED');
 return createHash('sha256').update(JSON.stringify([actor,image])).digest('hex');
}
