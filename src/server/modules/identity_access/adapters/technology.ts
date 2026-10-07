import 'server-only';import {createHash,randomUUID} from 'node:crypto';import type {PoolClient} from 'pg';import type {TechnologyStore} from '../../identity_access/public/technology-contracts';import type {DirectoryMembershipFact,ProvisionConnection,ProvisionChild} from '../../identity_access/public/technology-contracts';
import type {PostgresDatabase} from '../../runtime/public/runtime/postgres';import {ApiError} from '../../runtime/public/http';import {iamDb} from '../public/iam-db';import {assertCompanyScope} from '../public/iam-context';import {requirePermission} from '../public/iam-permissions';import {iamCommand} from '../public/iam-transactions';import {requireFreshMfa} from '../public/iam-mfa';import {grantInput,validateIamGrant,insertIamGrant,proposeIamReview} from '../public/iam-service';import {prepareDirectoryPreview} from '../public/directory-operations';import {technologyProvisioningFacts} from '../../people_payroll/public/technology-provisioning-facts';import {technologyVerifiedPersonUsersProjection,technologyRestorationAssignmentProjection} from '../../people_payroll/public/technology-provisioning-projection';import {technologyProvisioningEffects} from '../../workflow/public/technology-provisioning-effects';import {createTechnologyService} from '../application/technology';const clients=new WeakMap<TechnologyStore,PoolClient>();function nativeStore(c:PoolClient):TechnologyStore{const store:TechnologyStore={...technologyProvisioningFacts(c),...technologyProvisioningEffects(c),parentExpiry:async(facts)=>{const{actor}=facts;return (await c.query<{valid_until:number}>("SELECT valid_until FROM assignments WHERE id=$1", [
      actor.assignmentId,
    ])).rows[0];},
membershipSources:async(facts)=>{const{actor}=facts;return (await c.query<DirectoryMembershipFact>(
      "SELECT s.* FROM people_directory_membership_sources s JOIN iam_connections c ON c.id=s.connection_id WHERE c.company_id=$1 ORDER BY s.connection_id,s.group_id,s.member_id,s.source_id",
      [actor.companyId],
    )).rows;},
groupAvailable:async(facts)=>{const{actor,groupId,e}=facts;return (await c.query(
            "SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean AND um_person_in_unit($1,$3,definition->>'scopeNodeId')",
            [actor.companyId, groupId, e.person_id],
          )).rowCount;},
verifiedUsers:async(facts)=>{const{actor,e,definition}=facts;return (await c.query<{user_id:string}>(
        `SELECT DISTINCT u.user_id FROM (${technologyVerifiedPersonUsersProjection()} UNION SELECT user_id FROM iam_members WHERE company_id=$1 AND person_id=$2) u LEFT JOIN iam_members m ON m.company_id=$1 AND m.user_id=u.user_id WHERE u.user_id IS NOT NULL AND COALESCE(m.status,'active')='active' AND (CASE WHEN $3='local' THEN COALESCE(m.login_policy,'local')!='directory' ELSE m.login_policy='directory' AND m.connection_id=$4 END) ORDER BY u.user_id`,
        [
          actor.companyId,
          e.person_id,
          definition.account,
          definition.connectionId,
        ],
      )).rows;},
roleRevision:async(facts)=>{const{grant,actor}=facts;return (await c.query(
            "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active' AND published_revision=$3",
            [grant.roleId, actor.companyId, grant.expectedRoleRevision],
          )).rowCount;},
connectionInventory:async(facts)=>{const{connectionId,actor}=facts;return (await c.query<ProvisionConnection>(
            "SELECT c.version,s.inventory,s.write_enabled,s.tested_version,c.status FROM iam_connections c JOIN people_directory_state s ON s.connection_id=c.id WHERE c.id=$1 AND c.company_id=$2",
            [connectionId, actor.companyId],
          )).rows[0];},
directoryMembers:async(facts)=>{const{connectionId,actor,e}=facts;return (await c.query<{external_id:string}>(
          "SELECT DISTINCT i.external_id FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE i.connection_id=$1 AND m.company_id=$2 AND m.person_id=$3 AND m.status='active' ORDER BY i.external_id",
          [connectionId, actor.companyId, e.person_id],
        )).rows;},
createDirectoryOperation:async(facts)=>{const{operationId,actor,d,now,id}=facts;return void await c.query(
          "INSERT INTO people_directory_operations(id,company_id,connection_id,connection_version,state_version,status,definition,evidence,created_by,assignment_id,created_at,updated_at,approval_mode,mfa_session_id,provision_id) VALUES($1,$2,$3,$4,$5,'queued',$6,$7,$8,$9,$10,$10,'direct',$11,$12)",
          [
            operationId,
            actor.companyId,
            d.change.connectionId,
            d.change.expectedVersion,
            d.stateVersion,
            JSON.stringify(d.change),
            JSON.stringify(d.evidence),
            actor.userId,
            actor.assignmentId,
            now,
            actor.sessionId,
            id,
          ],
        );},
sharedDirectoryLease:async()=>{return (await c.query<{available:boolean}>("SELECT pg_try_advisory_xact_lock(821039320) AS available")).rows[0];},
directoryChildren:async(facts)=>{const{op}=facts;return (await c.query<ProvisionChild>(
          "SELECT id,status,last_error FROM people_directory_operations WHERE provision_id=$1 ORDER BY id",
          [op.id],
        )).rows;},
operatorAuthority:async(facts)=>{const{op,observedNow}=facts;return (await c.query(
            "SELECT 1 FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND scope_kind='holding' AND iam_assignment_active(id,$4) AND iam_role_allows(role,company_id,'iam.directory.apply')",
            [op.assignment_id, op.created_by, op.company_id, observedNow],
          )).rowCount;},
revokeOldGrants:async(facts)=>{const{op,plan,observedNow}=facts;return void await c.query(
            `UPDATE assignments SET revoked_at=$3,version=version+1 WHERE company_id=$1 AND ${technologyRestorationAssignmentProjection()}`,
            [op.company_id, plan.personId, observedNow, op.id],
          );},
retireDirectorySources:async(facts)=>{const{plan}=facts;return void await c.query(
          "DELETE FROM people_directory_membership_sources WHERE application_id=ANY($1::text[])",
          [plan.oldIds],
        );},
trackExternalMembership:async(facts)=>{const{plan,groupId,memberId,now}=facts;return void await c.query(
                "INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,'external',NULL,$4) ON CONFLICT DO NOTHING",
                [plan.definition.connectionId, groupId, memberId, now],
              );},
trackApplicationMembership:async(facts)=>{const{plan,groupId,memberId,applicationId,now}=facts;return void await c.query(
              "INSERT INTO people_directory_membership_sources VALUES($1,$2,$3,$4,$4,$5) ON CONFLICT DO NOTHING",
              [
                plan.definition.connectionId,
                groupId,
                memberId,
                applicationId,
                now,
              ],
            );},
appendAppliedAudit:async(facts)=>{const{generatedId,op,steps,observedNow}=facts;return void await c.query(
        "INSERT INTO iam_audit VALUES($1,$2,$3,$4,'people.provision.applied',$5,$6,$7)",
        [
          generatedId,
          op.company_id,
          op.created_by,
          op.assignment_id,
          op.id,
          JSON.stringify({ steps }),
          observedNow,
        ],
      );}};clients.set(store,c);return store;}function client(store:TechnologyStore){const c=clients.get(store);if(!c)throw Error('Technology transaction context is missing.');return c;}
function bind(raw:PostgresDatabase|null){const database=()=>{if(!raw)throw Error('Technology database context is missing.');return iamDb(raw);};return createTechnologyService({error:(s,c,m)=>new ApiError(s,c,m),now:Date.now,id:randomUUID,errorCode:error=>error instanceof ApiError?error.code:undefined,hash:v=>createHash('sha256').update(JSON.stringify(v)).digest('hex'),assertCompanyScope,authorize:requirePermission,fresh:requireFreshMfa,parseGrant:input=>grantInput.parse(input),directoryPreview:(store,actor,input)=>prepareDirectoryPreview(client(store),actor,input),validateGrant:(store,actor,input,parent)=>validateIamGrant(client(store),actor,input,parent),insertGrant:(store,actor,input,parent,source,sourceId)=>insertIamGrant(client(store),actor,input,parent,source,sourceId),proposeReview:(store,actor,operation,payload,reason)=>proposeIamReview(client(store),actor,operation,payload,reason),read:async run=>{const c=await database().pool.connect();try{return await run(nativeStore(c));}finally{c.release();}},preview:async run=>{const c=await database().pool.connect();try{await c.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');return await run(nativeStore(c));}finally{await c.query('ROLLBACK');c.release();}},command:(actor,key,operation,payload,permission,run)=>iamCommand(database(),actor,key,operation,payload,permission,c=>run(nativeStore(c))),due:async()=>{const c=await database().pool.connect();try{return await nativeStore(c).dueOperations({});}finally{c.release();}},worker:async()=>{const c=await database().pool.connect();return {store:nativeStore(c),begin:async()=>{await c.query('BEGIN ISOLATION LEVEL SERIALIZABLE');},commit:async()=>{await c.query('COMMIT');},rollback:async()=>{await c.query('ROLLBACK');},release:()=>c.release()};}});}
export function technologyData(raw:PostgresDatabase,...args:Parameters<ReturnType<typeof bind>['technologyData']>){return bind(raw).technologyData(...args);}export function previewProvisioning(raw:PostgresDatabase,...args:Parameters<ReturnType<typeof bind>['previewProvisioning']>){return bind(raw).previewProvisioning(...args);}export function applyProvisioning(raw:PostgresDatabase,...args:Parameters<ReturnType<typeof bind>['applyProvisioning']>){return bind(raw).applyProvisioning(...args);}export function applyProvisioningReview(c:PoolClient,...args:Parameters<ReturnType<typeof bind>['applyProvisioningReview']> extends [TechnologyStore,...infer A]?A:never){return bind(null).applyProvisioningReview(nativeStore(c),...args);}export function drainProvisioning(raw:PostgresDatabase){return bind(raw).drainProvisioning();}export function retryProvisioning(raw:PostgresDatabase,...args:Parameters<ReturnType<typeof bind>['retryProvisioning']>){return bind(raw).retryProvisioning(...args);}