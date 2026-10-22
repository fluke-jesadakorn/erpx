import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import 'server-only';
import {createHash} from 'node:crypto';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {IamActor} from '@erpx/server-identity_access/domain/iam-actor';
import {iamDb} from '@erpx/server-identity_access/adapters/iam-db';
import {iamCommand} from '@erpx/server-identity_access/adapters/iam-transactions';
import {requirePermission} from '@erpx/server-identity_access/adapters/iam-permissions';
import {requireFreshMfa} from '@erpx/server-identity_access/public/iam-mfa';
import {validateIamGrant,insertIamGrant} from '@erpx/server-identity_access/adapters/iam-service';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createUserManagementApplication,type UserManagementStore} from '@erpx/server-identity_access/application/user-management';
import type {ManagementObjectKind} from '@erpx/server-identity_access/domain/user-management';
const clients=new WeakMap<UserManagementStore,PoolClient>();
function managementTable(kind:ManagementObjectKind){return {attribute:'um_attributes',group:'um_groups',policy:'um_policies',attachment:'um_attachments'}[kind];}
function nativeStore(c:PoolClient):UserManagementStore{const store:UserManagementStore={activeScopeNode:async(facts)=>{const{actor,node}=facts;return (await c.query(
        `SELECT 1 ${organizationSources.user_management_organization_nodes_from} WHERE company_id=$1 AND id=$2 AND status='active'`,
        [actor.companyId, node],
      )).rowCount;},
descendantScope:async(facts)=>{const{actor,node}=facts;return (await c.query("SELECT um_descendant($1,$2,$3) ok", [
          actor.companyId,
          node,
          actor.scopeNodeId,
        ])).rows[0];},
person:async(facts)=>{const{id,actor}=facts;return (await c.query(
      `SELECT * ${peoplepayrollSources.user_management_hr_people_from} WHERE id=$1 AND company_id=$2 AND merged_into_id IS NULL`,
      [id, actor.companyId],
    )).rows[0];},
personInScope:async(facts)=>{const{actor,id}=facts;return (await c.query("SELECT um_person_in_unit($1,$2,$3) ok", [
          actor.companyId,
          id,
          actor.scopeNodeId,
        ])).rows[0];},
group:async(facts)=>{const{actor,id}=facts;return (await c.query("SELECT * FROM um_groups WHERE company_id=$1 AND id=$2", [
      actor.companyId,
      id,
    ])).rows[0];},
groups:async(facts)=>{const{company,scope}=facts;return (await c.query(
      "SELECT g.id,g.version,g.definition FROM um_groups g WHERE company_id=$1 AND ($2::text IS NULL OR g.definition->>'scopeNodeId' IS NOT NULL AND um_descendant($1,g.definition->>'scopeNodeId',$2)) ORDER BY g.definition->>'name',g.id",
      [company, scope],
    )).rows;},
people:async(facts)=>{const{company,scope}=facts;return (await c.query(
      `SELECT p.id,p.legal_name name,p.user_id,um_person_facts($1,p.id) definition,COALESCE((SELECT jsonb_agg(DISTINCT COALESCE(e.department_id,e.branch_id)) ${peoplepayrollSources.user_management_hr_engagements_e_from} WHERE e.company_id=$1 AND e.person_id=p.id AND e.status='active'),'[]'::jsonb) groups_json ${peoplepayrollSources.user_management_hr_people_p_from} WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND ($2::text IS NULL OR um_person_in_unit($1,p.id,$2)) ORDER BY p.legal_name,p.id`,
      [company, scope],
    )).rows;},
memberships:async(facts)=>{const{company}=facts;return (await c.query(
      `SELECT g.id,p.id person_id,
 EXISTS(SELECT 1 FROM um_memberships m WHERE m.company_id=$1 AND m.group_id=g.id AND m.person_id=p.id) direct,
 g.definition->>'kind'='automatic' AND um_rule_matches(g.definition->'rule',um_person_facts($1,p.id)) rule,
 EXISTS(SELECT 1 FROM um_groups child WHERE child.company_id=$1 AND child.definition->>'parentId'=g.id AND um_group_member($1,p.id,child.id)) nested,
 (EXISTS(SELECT 1 ${peoplepayrollSources.user_management_people_lifecycle_cases_lc_from} ${peoplepayrollSources.user_management_people_package_revisions_pr_join} ON pr.company_id=lc.company_id AND pr.package_id=lc.package_id AND pr.revision=lc.package_revision WHERE lc.company_id=$1 AND lc.person_id=p.id AND people_case_active(lc.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND pr.definition->'groupIds' @> jsonb_build_array(g.id)) OR EXISTS(SELECT 1 ${peoplepayrollSources.user_management_people_applied_packages_ap_from} WHERE ap.company_id=$1 AND ap.person_id=p.id AND people_application_active(ap.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND ap.definition->'groupIds' @> jsonb_build_array(g.id))) position
 FROM um_groups g CROSS ${peoplepayrollSources.user_management_hr_people_p_join} WHERE g.company_id=$1 AND p.company_id=$1 AND um_group_member($1,p.id,g.id)`,
      [company],
    )).rows;},
nodes:async(facts)=>{const{company,scope}=facts;return (await c.query(
      `SELECT id,parent_id,display_name,kind ${organizationSources.user_management_organization_nodes_from} WHERE company_id=$1 AND status='active' AND ($2::text IS NULL OR um_descendant($1,id,$2)) ORDER BY display_name,id`,
      [company, scope],
    )).rows;},
attachments:async(facts)=>{const{company}=facts;return (await c.query(
      "SELECT * FROM um_attachments WHERE company_id=$1 ORDER BY id",
      [company],
    )).rows;},
policies:async(facts)=>{const{company}=facts;return (await c.query(
      "SELECT * FROM um_policies WHERE company_id=$1 ORDER BY definition->>'name',id",
      [company],
    )).rows;},
attributes:async(facts)=>{const{company}=facts;return (await c.query(
      "SELECT * FROM um_attributes WHERE company_id=$1 ORDER BY definition->>'label',id",
      [company],
    )).rows;},
values:async(facts)=>{const{company}=facts;return (await c.query("SELECT * FROM um_values WHERE company_id=$1", [company])).rows;},
ancestry:async(facts)=>{const{company}=facts;return (await c.query(
      `SELECT id,parent_id ${organizationSources.user_management_organization_nodes_from} WHERE company_id=$1`,
      [company],
    )).rows;},
candidates:async(facts)=>{const{company,observedNow}=facts;return (await c.query(
      "SELECT c.*,r.status review_status,r.version review_version,CASE WHEN c.status='active' THEN iam_assignment_active(c.assignment_id,$2) ELSE true END enabled FROM um_candidates c LEFT JOIN iam_reviews r ON r.id=c.review_id WHERE c.company_id=$1 ORDER BY c.created_at DESC,c.id",
      [company, observedNow],
    )).rows;},
lockObjectVersion:async(facts)=>{const{table,actor,id}=facts;return (await c.query(
        `SELECT version FROM ${managementTable(table)} WHERE company_id=$1 AND id=$2 FOR UPDATE`,
        [actor.companyId, id],
      )).rows[0];},
replaceOwnedAttachment:async(facts)=>{const{table,actor,id,version,definition,extra}=facts;return void await c.query(
      `INSERT INTO ${managementTable(table)}(company_id,id,version,definition,author_id,parent_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,definition=excluded.definition,author_id=excluded.author_id,parent_id=excluded.parent_id`,
      [
        actor.companyId,
        id,
        version,
        JSON.stringify(definition),
        extra.authorId,
        extra.parentId,
      ],
    );},
replaceObject:async(facts)=>{const{table,actor,id,version,definition}=facts;return void await c.query(
      `INSERT INTO ${managementTable(table)}(company_id,id,version,definition) VALUES($1,$2,$3,$4) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,definition=excluded.definition`,
      [actor.companyId, id, version, JSON.stringify(definition)],
    );},
appendObjectHistory:async(facts)=>{const{actor,table,id,version,definition,observedNow}=facts;return void await c.query(
    "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) VALUES($1,$2,$3,$4,$5,$6)",
    [
      actor.companyId,
      managementTable(table),
      id,
      version,
      JSON.stringify(definition),
      observedNow,
    ],
  );},
parentExpiry:async(facts)=>{const{actor}=facts;return (await c.query("SELECT valid_until FROM assignments WHERE id=$1", [
      actor.assignmentId,
    ])).rows[0];},
grantScopeNode:async(facts)=>{const{actor,grant}=facts;return (await c.query(
          `SELECT 1 ${organizationSources.user_management_organization_nodes_from} WHERE company_id=$1 AND id=$2 AND kind=$3`,
          [actor.companyId, grant.scopeNodeId, grant.scopeKind],
        )).rowCount;},
publishedGrantRole:async(facts)=>{const{actor,grant}=facts;return (await c.query(
        "SELECT v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.published_revision=$3 AND r.status='active'",
        [actor.companyId, grant.roleId, grant.expectedRoleRevision],
      )).rows[0];},
attributeRoleActive:async(facts)=>{const{roleId,actor}=facts;return (await c.query(
            "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'",
            [roleId, actor.companyId],
          )).rowCount;},
attributeRuleUse:async(facts)=>{const{actor,input}=facts;return (await c.query(
        "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)",
        [actor.companyId, `custom:${input.id}`],
      )).rowCount;},
parentGroupScope:async(facts)=>{const{actor,input,row}=facts;return (await c.query("SELECT um_descendant($1,$2,$3) ok", [
              actor.companyId,
              input.scopeNodeId,
              row.definition.scopeNodeId,
            ])).rows[0];},
childGroupScope:async(facts)=>{const{actor,child,input}=facts;return (await c.query("SELECT um_descendant($1,$2,$3) ok", [
              actor.companyId,
              child.scopeNodeId,
              input.scopeNodeId,
            ])).rows[0];},
memberInGroupScope:async(facts)=>{const{actor,personId,row}=facts;return (await c.query("SELECT um_person_in_unit($1,$2,$3) ok", [
            actor.companyId,
            personId,
            row.definition.scopeNodeId,
          ])).rows[0];},
addMember:async(facts)=>{const{actor,input,personId}=facts;return void await c.query(
          "INSERT INTO um_memberships VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
          [actor.companyId, input.groupId, personId],
        );},
removeMember:async(facts)=>{const{actor,input,personId}=facts;return void await c.query(
          "DELETE FROM um_memberships WHERE company_id=$1 AND group_id=$2 AND person_id=$3",
          [actor.companyId, input.groupId, personId],
        );},
lockValues:async(facts)=>{const{actor,input}=facts;return (await c.query(
        "SELECT * FROM um_values WHERE company_id=$1 AND target=$2 AND object_id=$3 FOR UPDATE",
        [actor.companyId, input.target, input.objectId],
      )).rows[0];},
inheritedValue:async(facts)=>{const{input,actor,fieldId}=facts;return (await c.query(
            `WITH RECURSIVE ancestry AS (SELECT parent_id,ARRAY[id] path,0 depth ${organizationSources.user_management_organization_nodes_from} WHERE id=$1 AND company_id=$2 UNION ALL SELECT n.parent_id,a.path||n.id,a.depth+1 ${organizationSources.user_management_organization_nodes_n_from} JOIN ancestry a ON n.id=a.parent_id WHERE n.company_id=$2 AND NOT n.id=ANY(a.path) AND a.depth<32) SELECT v.definition->$3 value FROM ancestry a JOIN um_values v ON v.object_id=a.parent_id AND v.company_id=$2 AND v.target='unit' WHERE v.definition ? $3 ORDER BY a.depth LIMIT 1`,
            [input.objectId, actor.companyId, fieldId],
          )).rows[0];},
replaceValues:async(facts)=>{const{actor,input,version,values}=facts;return void await c.query(
      "INSERT INTO um_values VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id,target,object_id) DO UPDATE SET version=excluded.version,definition=excluded.definition",
      [
        actor.companyId,
        input.target,
        input.objectId,
        version,
        JSON.stringify(values),
      ],
    );},
appendValueHistory:async(facts)=>{const{actor,input,version,values,observedNow}=facts;return void await c.query(
      "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) VALUES($1,$2,$3,$4,$5,$6)",
      [
        actor.companyId,
        `values:${input.target}`,
        input.objectId,
        version,
        JSON.stringify(values),
        observedNow,
      ],
    );},
exclusionWithinTarget:async(facts)=>{const{actor,excluded,input}=facts;return (await c.query("SELECT um_descendant($1,$2,$3) ok", [
            actor.companyId,
            excluded,
            input.targetId,
          ])).rows[0];},
lockManagement:async(facts)=>{const{companyId}=facts;return void await c.query("SELECT pg_advisory_xact_lock(hashtext($1))", [
    `user-management:${companyId}`,
  ]);},
rejectRevokedCandidates:async(facts)=>{const{companyId}=facts;return void await c.query(
    "UPDATE um_candidates c SET status='rejected' FROM assignments a WHERE c.company_id=$1 AND c.assignment_id=a.id AND c.status='active' AND a.revoked_at IS NOT NULL AND um_attachment_matches(c.company_id,c.person_id,c.attachment_id)",
    [companyId],
  );},
invalidateManagedAccess:async(facts)=>{const{companyId}=facts;return void await c.query("SELECT um_invalidate($1)", [companyId]);},
rejectReviewedCandidates:async(facts)=>{const{companyId}=facts;return void await c.query(
    "UPDATE um_candidates c SET status='rejected' FROM iam_reviews r WHERE c.review_id=r.id AND c.company_id=$1 AND c.status='pending' AND r.status='rejected'",
    [companyId],
  );},
eligiblePolicyMatches:async(facts)=>{const{companyId,observedNow}=facts;return (await c.query(
      `SELECT a.*,p.version policy_version,p.definition policy_data,h.id person_id,h.user_id FROM um_attachments a JOIN um_policies p ON p.company_id=a.company_id AND p.id=a.definition->>'policyId' CROSS ${peoplepayrollSources.user_management_hr_people_h_join} WHERE a.company_id=$1 AND h.company_id=$1 AND h.user_id IS NOT NULL AND h.status='active' AND um_attachment_matches($1,h.id,a.id) AND iam_assignment_active(a.parent_id,$2) ORDER BY a.id,h.id`,
      [companyId, observedNow],
    )).rows;},
policyProposer:async(facts)=>{const{match}=facts;return (await c.query(
        "SELECT a.*,v.grantable_json FROM assignments a JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) JOIN iam_role_revisions v ON v.role_id=r.id AND v.company_id=r.company_id AND v.revision=r.published_revision WHERE a.id=$1 AND a.user_id=$2 AND iam_role_allows(a.role,a.company_id,'iam.manage')",
        [match.parent_id, match.author_id],
      )).rows[0];},
existingCandidate:async(facts)=>{const{companyId,match,index}=facts;return (await c.query(
            "SELECT 1 FROM um_candidates WHERE company_id=$1 AND person_id=$2 AND attachment_id=$3 AND grant_index=$4 AND attachment_version=$5 AND policy_version=$6 AND status IN ('pending','active','rejected','blocked')",
            [
              companyId,
              match.person_id,
              match.id,
              index,
              match.version,
              match.policy_version,
            ],
          )).rowCount;},
recordBlockedCandidate:async(facts)=>{const{generatedId,companyId,match,index,grant,blocked,observedNow}=facts;return void await c.query(
          "INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,attachment_version,policy_version,grant_index,definition,status,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'blocked',$10)",
          [
            generatedId,
            companyId,
            match.person_id,
            match.user_id,
            match.id,
            match.version,
            match.policy_version,
            index,
            JSON.stringify({ ...grant, blockedReason: blocked }),
            observedNow,
          ],
        );},
insertGrantReview:async(facts)=>{const{reviewId,companyId,payload,match,observedNow,grant}=facts;return void await c.query(
        "INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,reason) VALUES($1,$2,'policy.grant',$3,$4,COALESCE((SELECT person_id FROM iam_members WHERE company_id=$2 AND user_id=$4),$4),$5,$6)",
        [
          reviewId,
          companyId,
          JSON.stringify(payload),
          match.author_id,
          observedNow,
          grant.reason,
        ],
      );},
insertGrantCandidate:async(facts)=>{const{id,companyId,match,index,grant,reviewId,observedNow}=facts;return void await c.query(
        "INSERT INTO um_candidates(id,company_id,person_id,user_id,attachment_id,attachment_version,policy_version,grant_index,definition,review_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        [
          id,
          companyId,
          match.person_id,
          match.user_id,
          match.id,
          match.version,
          match.policy_version,
          index,
          JSON.stringify(grant),
          reviewId,
          observedNow,
        ],
      );},
appendGrantProposalAudit:async(facts)=>{const{generatedId,companyId,match,id,payload,observedNow}=facts;return void await c.query(
        "INSERT INTO iam_audit VALUES($1,$2,$3,$4,'policy.grant.proposed',$5,$6,$7)",
        [
          generatedId,
          companyId,
          match.author_id,
          match.parent_id,
          id,
          JSON.stringify(payload),
          observedNow,
        ],
      );},
lockCandidate:async(facts)=>{const{actor,p}=facts;return (await c.query(
      "SELECT * FROM um_candidates WHERE company_id=$1 AND id=$2 FOR UPDATE",
      [actor.companyId, p.candidateId],
    )).rows[0];},
candidateEligible:async(facts)=>{const{row,observedNow}=facts;return (await c.query("SELECT um_candidate_eligible($1,$2) ok", [
        row.id,
        observedNow,
      ])).rows[0];},
activateCandidate:async(facts)=>{const{assignmentId,row}=facts;return void await c.query(
    "UPDATE um_candidates SET assignment_id=$1,status='active' WHERE id=$2",
    [assignmentId, row.id],
  );},
lockCommand:async(facts)=>{const{actor}=facts;return void await c.query("SELECT pg_advisory_xact_lock(hashtext($1))", [
      `user-management:${actor.companyId}`,
    ]);}};clients.set(store,c);return store;}

function source(store:UserManagementStore){const client=clients.get(store);if(!client)throw new Error('USER_MANAGEMENT_TRANSACTION_REQUIRED');return client;}
function application(db?:PostgresDatabase){return createUserManagementApplication({error:(...args)=>new ApiError(...args),now:Date.now,id:()=>crypto.randomUUID(),hash:value=>createHash('sha256').update(JSON.stringify(value)).digest('hex'),authorize:requirePermission,fresh:requireFreshMfa,ruleError:(error):error is Error&{code:string}=>error instanceof ApiError||error instanceof BusinessRuleError,readTransaction:async run=>{const client=await iamDb(db!).pool.connect();try{await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');const result=await run(nativeStore(client));await client.query('COMMIT');return result;}finally{await client.query('ROLLBACK');client.release();}},previewTransaction:async run=>{const client=await iamDb(db!).pool.connect();try{await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');return await run(nativeStore(client));}finally{await client.query('ROLLBACK');client.release();}},command:(actor,key,operation,payload,permission,run)=>iamCommand(iamDb(db!),actor,key,operation,payload,permission,client=>run(nativeStore(client))),workSession:async run=>{const client=await iamDb(db!).pool.connect();try{return await run({companies:async()=>(await client.query<{company_id:string}>('SELECT DISTINCT company_id FROM um_attachments')).rows.map(row=>row.company_id),transaction:async(_companyId,run)=>{await client.query('BEGIN ISOLATION LEVEL SERIALIZABLE');try{await run(nativeStore(client));await client.query('COMMIT');}catch(error){await client.query('ROLLBACK');throw error;}}});}finally{client.release();}},validateGrant:(store,actor,input,parentId)=>validateIamGrant(source(store),actor,input,parentId),insertGrant:(store,actor,input,parentId,kind,sourceId)=>insertIamGrant(source(store),actor,input,parentId,kind,sourceId)});}
async function invoke<Result>(run:()=>Promise<Result>){try{return await run();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function managementSnapshot(client:PoolClient,actor:IamActor){return invoke(()=>application().managementSnapshot(nativeStore(client),actor));}
export function readManagement(db:PostgresDatabase,actor:IamActor){return invoke(()=>application(db).readManagement(actor));}
export function reconcileManagement(client:PoolClient,companyId:string){return invoke(()=>application().reconcileManagement(nativeStore(client),companyId));}
export function applyManagementReview(client:PoolClient,actor:IamActor,payload:Record<string,unknown>){return invoke(()=>application().applyManagementReview(nativeStore(client),actor,payload));}
export function managementCommand(db:PostgresDatabase,actor:IamActor,raw:unknown,key:string,preview:boolean,token?:string){return invoke(()=>application(db).managementCommand(actor,raw,key,preview,token));}
export function managementWork(db:PostgresDatabase){return invoke(()=>application(db).managementWork());}
