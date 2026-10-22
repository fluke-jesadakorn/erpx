import 'server-only';import type {PoolClient} from 'pg';import type {TechnologyStore} from '@erpx/server-identity_access/public/technology-contracts';import type {TechnologyData,AppliedPackageFact,ProvisionEngagement,ProvisionGrant,ProvisionOperation} from '@erpx/server-identity_access/public/technology-contracts';
import {technologyPendingReviewProjection} from '@erpx/server-identity_access/public/technology-provisioning-projection';export function technologyProvisioningFacts(c:PoolClient):Pick<TechnologyStore,'applications'|'operations'|'activeApplications'|'engagement'|'accessCutoff'|'restorationPlacement'|'positionPackage'|'packageRevision'|'personInProgress'|'createOperation'|'applyPackage'|'createGrant'|'saveSteps'|'lockedGrant'|'reviewerIsPerson'|'grantUserIsPerson'|'bindGrant'|'dueOperations'|'lockedOperation'|'recordChildrenState'|'lockedPlacement'|'effectiveCutoff'|'restoredPlacement'|'endPriorPackages'|'restoreCutoff'|'completeAccessPreparation'|'endNewApplication'|'endReplacedApplications'|'createPreparationTask'|'completeOperation'|'recordBlockedOperation'|'retryOperation'|'queueRetry'>{return {applications:async(facts)=>{const{actor,observedNow}=facts;return (await c.query<AppliedPackageFact>(
      "SELECT ap.*,people_application_active(ap.id,$2) active FROM people_applied_packages ap WHERE company_id=$1 ORDER BY created_at DESC",
      [actor.companyId, observedNow],
    )).rows;},
operations:async(facts)=>{const{actor}=facts;return (await c.query<TechnologyData["operations"][number]>(
      `SELECT op.id,op.status,op.version,op.last_error,op.steps,op.created_at,(SELECT count(*)::int FROM people_provision_grants g ${technologyPendingReviewProjection()}) AS "reviewsPending" FROM people_provision_operations op WHERE company_id=$1 ORDER BY created_at DESC LIMIT 200`,
      [actor.companyId],
    )).rows;},
activeApplications:async(facts)=>{const{actor}=facts;return (await c.query<AppliedPackageFact>(
      "SELECT ap.* FROM people_applied_packages ap WHERE company_id=$1 AND ended_at IS NULL ORDER BY id",
      [actor.companyId],
    )).rows;},
engagement:async(facts)=>{const{engagementId,actor}=facts;return (await c.query<ProvisionEngagement>(
        "SELECT e.*,p.legal_name,p.user_id,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2 AND p.merged_into_id IS NULL",
        [engagementId, actor.companyId],
      )).rows[0];},
accessCutoff:async(facts)=>{const{actor,e,observedNow}=facts;return (await c.query(
        "SELECT 1 FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
        [actor.companyId, e.person_id, observedNow],
      )).rowCount;},
restorationPlacement:async(facts)=>{const{actor,e}=facts;return (await c.query(
            "SELECT 1 FROM people_lifecycle_cases c WHERE c.company_id=$1 AND c.engagement_id=$2 AND c.status='completed' AND c.workflow='placement' AND c.kind IN ('onboarding','transfer') AND c.effective_at>(SELECT max(effective_at) FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$3 AND cancelled_at IS NULL AND restored_at IS NULL)",
            [actor.companyId, e.id, e.person_id],
          )).rowCount;},
positionPackage:async(facts)=>{const{actor,e}=facts;return (await c.query<{package_id:string;revision:number}>(
            "SELECT package_id,revision FROM people_position_packages WHERE company_id=$1 AND position_id=$2",
            [actor.companyId, e.position_id],
          )).rows[0];},
packageRevision:async(facts)=>{const{actor,binding}=facts;return (await c.query<{definition:unknown}>(
        "SELECT r.definition FROM people_package_revisions r WHERE r.company_id=$1 AND r.package_id=$2 AND r.revision=$3",
        [actor.companyId, binding.package_id, binding.revision],
      )).rows[0];},
personInProgress:async(facts)=>{const{actor,plan}=facts;return (await c.query(
              "SELECT 1 FROM people_provision_operations op, jsonb_array_elements(op.evidence->'plans') p WHERE op.company_id=$1 AND op.status IN ('queued','running','retry','pending_review') AND p->>'personId'=$2",
              [actor.companyId, plan.personId],
            )).rowCount;},
createOperation:async(facts)=>{const{id,actor,input,p,now}=facts;return void await c.query(
        "INSERT INTO people_provision_operations(id,company_id,definition,evidence,status,created_by,assignment_id,session_id,created_at,updated_at) VALUES($1,$2,$3,$4,'queued',$5,$6,$7,$8,$8)",
        [
          id,
          actor.companyId,
          JSON.stringify(input),
          JSON.stringify(p.evidence),
          actor.userId,
          actor.assignmentId,
          actor.sessionId,
          now,
        ],
      );},
applyPackage:async(facts)=>{const{applicationId,actor,plan,input,id,now}=facts;return void await c.query(
          "INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,position_id,package_id,package_revision,definition,source_kind,scope,operation_id,authorizing_assignment_id,valid_from,valid_until,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$13)",
          [
            applicationId,
            actor.companyId,
            plan.personId,
            plan.id,
            plan.positionId,
            plan.packageId,
            plan.revision,
            JSON.stringify(plan.definition),
            input.sourceKind,
            JSON.stringify({
              exclude: input.exclude,
              branchId: plan.branchId,
              departmentId: plan.departmentId,
            }),
            id,
            actor.assignmentId,
            now,
            input.validUntil,
          ],
        );},
createGrant:async(facts)=>{const{grantId,id,applicationId,grant,review}=facts;return void await c.query(
              "INSERT INTO people_provision_grants(id,operation_id,application_id,definition,review_id) VALUES($1,$2,$3,$4,$5)",
              [grantId, id, applicationId, JSON.stringify(grant), review.id],
            );},
saveSteps:async(facts)=>{const{id,steps}=facts;return void await c.query(
        "UPDATE people_provision_operations SET steps=$2 WHERE id=$1",
        [id, JSON.stringify(steps)],
      );},
lockedGrant:async(facts)=>{const{p,actor}=facts;return (await c.query<ProvisionGrant>(
      "SELECT g.*,ap.person_id,ap.company_id,ap.ended_at,op.status FROM people_provision_grants g JOIN people_applied_packages ap ON ap.id=g.application_id JOIN people_provision_operations op ON op.id=g.operation_id WHERE g.id=$1 AND ap.company_id=$2 FOR UPDATE OF g",
      [p.grantId, actor.companyId],
    )).rows[0];},
reviewerIsPerson:async(facts)=>{const{actor,g}=facts;return (await c.query<{same:boolean}>("SELECT people_person_user($1,$2,$3) same", [
        actor.companyId,
        g.person_id,
        actor.userId,
      ])).rows[0];},
grantUserIsPerson:async(facts)=>{const{actor,g,grant}=facts;return (await c.query<{ok:boolean}>("SELECT people_person_user($1,$2,$3) ok", [
        actor.companyId,
        g.person_id,
        grant.userId,
      ])).rows[0];},
bindGrant:async(facts)=>{const{g,assignment}=facts;return void await c.query(
    "UPDATE people_provision_grants SET assignment_id=$2 WHERE id=$1",
    [g.id, assignment],
  );},
dueOperations:async()=>{return (await c.query<{id:string}>(
        "SELECT id FROM people_provision_operations WHERE status IN ('queued','running','retry') ORDER BY created_at LIMIT 30",
      )).rows;},
lockedOperation:async(facts)=>{const{row}=facts;return (await c.query<ProvisionOperation>(
          "SELECT * FROM people_provision_operations WHERE id=$1 AND status IN ('queued','running','retry') FOR UPDATE SKIP LOCKED",
          [row.id],
        )).rows[0];},
recordChildrenState:async(facts)=>{const{operationId,status,lastError,steps}=facts;return void await c.query(
          "UPDATE people_provision_operations SET status=$2,steps=$3,last_error=$4 WHERE id=$1",
          [
            operationId,status,JSON.stringify(steps),lastError,
          ],
        );},
lockedPlacement:async(facts)=>{const{plan,op}=facts;return (await c.query<{version:number;status:string;person_status:string}>(
            "SELECT e.version,e.status,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=$1 AND e.company_id=$2 FOR UPDATE OF e",
            [plan.id, op.company_id],
          )).rows[0];},
effectiveCutoff:async(facts)=>{const{op,plan,observedNow}=facts;return (await c.query<{at:number|null}>(
            "SELECT max(effective_at) at FROM people_access_cutoffs WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
            [op.company_id, plan.personId, observedNow],
          )).rows[0];},
restoredPlacement:async(facts)=>{const{op,plan,cutoff}=facts;return (await c.query(
                "SELECT 1 FROM people_lifecycle_cases WHERE company_id=$1 AND engagement_id=$2 AND status='completed' AND workflow='placement' AND kind IN ('onboarding','transfer') AND effective_at>$3",
                [op.company_id, plan.id, cutoff],
              )).rowCount;},
endPriorPackages:async(facts)=>{const{op,plan,observedNow}=facts;return void await c.query(
            "UPDATE people_applied_packages SET ended_at=$3 WHERE company_id=$1 AND person_id=$2 AND operation_id IS DISTINCT FROM $4 AND ended_at IS NULL",
            [op.company_id, plan.personId, observedNow, op.id],
          );},
restoreCutoff:async(facts)=>{const{op,plan,observedNow}=facts;return void await c.query(
            "UPDATE people_access_cutoffs SET restored_at=$3 WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3",
            [op.company_id, plan.personId, observedNow],
          );},
completeAccessPreparation:async(facts)=>{const{op,plan,now}=facts;return void await c.query(
            "UPDATE people_preparation_tasks t SET status='completed',version=t.version+1,updated_at=$3 FROM people_lifecycle_cases lc WHERE t.case_id=lc.id AND lc.company_id=$1 AND lc.engagement_id=$2 AND lc.workflow='placement' AND lc.status='completed' AND lc.kind IN ('onboarding','transfer') AND t.title='Prepare access for employee placement' AND t.status='pending'",
            [op.company_id, plan.id, now],
          );},
endNewApplication:async(facts)=>{const{applicationId,now}=facts;return void await c.query(
            "UPDATE people_applied_packages SET ended_at=$2 WHERE id=$1",
            [applicationId, now],
          );},
endReplacedApplications:async(facts)=>{const{plan,now}=facts;return void await c.query(
          "UPDATE people_applied_packages SET ended_at=$2 WHERE id=ANY($1::text[]) AND ended_at IS NULL",
          [plan.oldIds, now],
        );},
createPreparationTask:async(facts)=>{const{applicationId,index,op,task,privileged,now}=facts;return void await c.query(
            "INSERT INTO people_preparation_tasks(id,company_id,kind,title,updated_at,privileged,application_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
            [
              applicationId + ":task:" + index,
              op.company_id,
              task.kind,
              task.label,
              now,
              privileged,
              applicationId,
            ],
          );},
completeOperation:async(facts)=>{const{op,steps,observedNow}=facts;return void await c.query(
        "UPDATE people_provision_operations SET status='completed',steps=$2,version=version+1,updated_at=$3,last_error=NULL WHERE id=$1",
        [op.id, JSON.stringify(steps), observedNow],
      );},
recordBlockedOperation:async(facts)=>{const{row,reason,observedNow}=facts;return void await c.query(
        "UPDATE people_provision_operations SET status='blocked',last_error=$2,version=version+1,updated_at=$3 WHERE id=$1",
        [
          row.id,
          reason,
          observedNow,
        ],
      );},
retryOperation:async(facts)=>{const{id,actor}=facts;return (await c.query<ProvisionOperation>(
          "SELECT * FROM people_provision_operations WHERE id=$1 AND company_id=$2 FOR UPDATE",
          [id, actor.companyId],
        )).rows[0];},
queueRetry:async(facts)=>{const{id,actor,observedNow}=facts;return void await c.query(
        "UPDATE people_provision_operations SET status='queued',session_id=$2,version=version+1,last_error=NULL,updated_at=$3 WHERE id=$1",
        [id, actor.sessionId, observedNow],
      );}};}
