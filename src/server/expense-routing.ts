import {policyRead} from "./iam-read-snapshot";
import {personIdentityIds} from "./iam-person";
import { organizationRoster } from "./organization";
import { reportingIssues } from "@/lib/organization";
import { spendingTemplate } from "./spending-templates";
import type { SpendingKind } from "@/lib/spending";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import {
  activeStaffSelect,
  activeStaff,
  staffPeople,
  reportingChain,
  type ApprovalRoute,
  type RecordScope,
} from "./staff-policy";
import type { ApprovalInstance, ApprovalStep } from "./approvals";
import { assertion } from "./commands";

// Capture before eligibility reads, then compare inside the committing batch.
// This includes level/routing changes and additions as well as role revocations.
const authoritySnapshotSql = `SELECT jsonb_build_object(
 'organization',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'status',status,'parent',parent_id,'branch',branch_id)) FROM (SELECT * FROM organization_nodes WHERE company_id=? ORDER BY id)),
 'people',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'user',user_id,'status',status,'merged',merged_into_id)) FROM (SELECT * FROM hr_people WHERE company_id=? ORDER BY id)),
 'engagements',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'person',person_id,'version',version,'status',status,'branch',branch_id,'department',department_id,'level',level_id,'manager',manager_person_id,'start',start_date,'end',end_date,'position',position_id)) FROM (SELECT * FROM hr_engagements WHERE company_id=? ORDER BY id)),
 'levels',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'rank',rank,'number',level_number,'active',active)) FROM (SELECT * FROM staff_levels WHERE company_id=? ORDER BY id)),
 'positions',(SELECT erpx_json_array_agg(to_jsonb(p)) FROM (SELECT * FROM organization_positions WHERE company_id=? ORDER BY id) p),
 'template',(SELECT to_jsonb(t) FROM spending_approval_templates t WHERE company_id=?),
 'assignments',(SELECT erpx_json_array_agg(jsonb_build_object('id',id,'user',user_id,'role',role,'scope',scope_kind,'node',scope_node_id)) FROM (SELECT * FROM assignments WHERE company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY id))
) snapshot`;
export async function expenseAuthoritySnapshot(
  db: SqlDatabase,
  companyId: string,
) {
  const args = Array(7).fill(companyId) as string[];
  const row = await db
    .prepare(authoritySnapshotSql)
    .bind(...args)
    .first<{ snapshot: string }>();
  const check = {
    query: `(${authoritySnapshotSql})=?`,
    args: [...args, row!.snapshot],
  };
  return {
    ...check,
    guard: (key: string, label = "expense-authority-snapshot") =>
      assertion(db, key, label, check.query, check.args),
  };
}

export {
  expenseStages,
  expenseStageLabels,
  type ExpenseStage,
} from "@/lib/expense-workflow";

export async function expenseRoleUsers(db:SqlDatabase,scope:RecordScope,role:string|null){
 return (await policyRead(db,JSON.stringify(["role-users",scope.company_id,scope.branch_id,scope.department_id,role]),()=>readExpenseRoleUsers(db,scope,role))).slice();
}
async function readExpenseRoleUsers(
  db: SqlDatabase,
  scope: RecordScope,
  role: string | null,
) {
  const now = Date.now();
  const rows = await db
    .prepare(
      `SELECT staff.user_id FROM (${activeStaffSelect}) staff WHERE EXISTS (
    SELECT 1 FROM assignments a WHERE a.user_id=staff.user_id AND a.company_id=? AND (?::text IS NULL OR iam_role_allows(a.role,a.company_id,'workflow.'||?)) AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND a.valid_from<=? AND a.valid_until>?
    AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?)))
    AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=staff.company_id AND kind='branch' AND status='active'))
    AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=staff.company_id AND kind='department' AND status='active'))
    GROUP BY staff.user_id HAVING COUNT(*)=1 ORDER BY staff.user_id`,
    )
    .bind(
      scope.company_id,
      now,
      now,
      scope.company_id,
      role,
      role,
      now,
      now,
      scope.branch_id,
      scope.department_id,
      scope.branch_id,
      scope.branch_id,
      scope.department_id,
      scope.department_id,
    )
    .all<{ user_id: string }>();
  return rows.results.map((row) => row.user_id);
}

export async function legacyExpenseRoute(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  excluded: string[] = [],
  departmentOnly = false,
): Promise<ApprovalRoute> {
  let scope: RecordScope = {
    company_id: companyId,
    branch_id: null,
    department_id: null,
  };
  try {
    const authoritySnapshot = await expenseAuthoritySnapshot(db, companyId);
    const claimant = await activeStaff(db, companyId, userId);
    scope = claimant;
    if (!claimant.department_id)
      throw new ApiError(
        409,
        "DEPARTMENT_REQUIRED",
        "Set the claimant's department in People before submitting.",
      );
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='department' AND status='active'",
        )
        .bind(claimant.department_id, companyId)
        .first())
    )
      throw new ApiError(
        409,
        "DEPARTMENT_REQUIRED",
        "Select an active department in People before submitting.",
      );
    const levels = (
      await db
        .prepare(
          "SELECT id,rank FROM staff_levels WHERE company_id=? AND active=1",
        )
        .bind(companyId)
        .all<{ id: string; rank: number }>()
    ).results;
    const ranks = new Map(levels.map((l) => [l.id, l.rank]));
    const membersInScope = new Set(await expenseRoleUsers(db, scope, null));
    if (!membersInScope.has(userId))
      throw new ApiError(
        409,
        "CLAIMANT_SCOPE_REQUIRED",
        "The claimant's access must cover their active branch and department. Review their People Access panel.",
      );
    if (!claimant.level_id || !ranks.has(claimant.level_id))
      throw new ApiError(
        409,
        "LEVEL_REQUIRED",
        "Set the claimant's active staff level in People before submitting.",
      );
    const roster = await staffPeople(db, companyId),
      people = roster.filter(
        (p) =>
          roster.filter((other) => other.user_id === p.user_id).length === 1,
      ),
      chain = reportingChain(people, claimant.person_id),
      forbidden = new Set([userId, ...excluded]);
    const department = people.filter(
      (p) =>
        p.department_id === claimant.department_id &&
        p.level_id &&
        ranks.has(p.level_id),
    );
    const highest = Math.max(...department.map((p) => ranks.get(p.level_id!)!));
    let heads = department
      .filter(
        (p) =>
          ranks.get(p.level_id!) === highest &&
          !forbidden.has(p.user_id) &&
          membersInScope.has(p.user_id),
      )
      .map((p) => p.user_id);
    let authority: "named" | "department" | "holding_owner" = "department";
    if (!heads.length) {
      const superior = chain.find(
        (p) => !forbidden.has(p.user_id) && membersInScope.has(p.user_id),
      );
      if (superior) {
        heads = [superior.user_id];
        authority = "named";
      } else {
        heads = (await expenseRoleUsers(db, scope, "holding_owner")).filter(
          (id) => !forbidden.has(id),
        );
        authority = "holding_owner";
      }
    }
    if (!heads.length)
      throw new ApiError(
        409,
        "INDEPENDENT_HEAD_REQUIRED",
        "Configure an independent superior or company owner for this top-level claim.",
      );
    if (departmentOnly) return {
      scope, blockedReason: null,
      steps: [{ userId: heads[0], authority, candidateUserIds: heads, stage: "department_approval",
        snapshot: { departmentId: claimant.department_id, highestRank: highest, excluded } }],
      checks: [authoritySnapshot],
    };
    const [preparers, managers, payers] = await Promise.all(
      ["finance_preparer", "finance_approver", "treasury"].map(async (role) =>
        (await expenseRoleUsers(db, scope, role)).filter(
          (id) => !forbidden.has(id),
        ),
      ),
    );
    if (!preparers.length || !managers.length || !payers.length)
      throw new ApiError(
        409,
        "EXPENSE_TEAM_REQUIRED",
        "Configure Accounts reviewers, Accounts managers, and Treasury for this company or branch.",
      );
    heads = heads.filter((h) =>
      preparers.some(
        (p) =>
          p !== h &&
          managers.some(
            (m) =>
              m !== h && m !== p && payers.some((t) => ![h, p, m].includes(t)),
          ),
      ),
    );
    if (!heads.length)
      throw new ApiError(
        409,
        "EXPENSE_SEPARATION_REQUIRED",
        "Four different people must be available for department approval, Accounts review, Accounts manager approval, and payment.",
      );
    return {
      scope,
      blockedReason: null,
      steps: [
        {
          userId: heads[0],
          authority,
          candidateUserIds: heads,
          stage: "department_approval",
          snapshot: {
            departmentId: claimant.department_id,
            highestRank: highest,
            excluded,
          },
        },
        {
          userId: preparers[0],
          authority: "named",
          candidateUserIds: preparers,
          requiredRole: "finance_preparer",
          stage: "accounts_review",
        },
        {
          userId: managers[0],
          authority: "named",
          candidateUserIds: managers,
          requiredRole: "finance_approver",
          stage: "accounts_approval",
        },
      ],
      checks: [
        authoritySnapshot,
        {
          query:
            "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')",
          args: [claimant.engagement_id, claimant.engagement_version],
        },
      ],
    };
  } catch (error) {
    if (!(error instanceof ApiError)) throw error;
    return { scope, steps: [], blockedReason: error.message };
  }
}

export async function expensePoolCandidates(
  db: SqlDatabase,
  instance: ApprovalInstance,
  step: ApprovalStep,
  steps: ApprovalStep[],
) {
  const claim = await db
    .prepare(
      "SELECT prepared_by,claimant_user_id,accounting_json FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=2",
    )
    .bind(instance.record_id, instance.company_id)
    .first<{
      prepared_by: string;
      claimant_user_id: string;
      accounting_json: string;
    }>();
  if (!claim) return [];
  const snapshot = JSON.parse(step.route_snapshot_json ?? "{}") as {spendingPolicy?:number};
  if (snapshot.spendingPolicy) return spendingStepCandidates(db,instance,claim.claimant_user_id,[claim.prepared_by],{
    snapshot:JSON.parse(step.route_snapshot_json??"{}"),
    stage:step.stage ?? undefined, requiredRole:step.required_role ?? undefined,
    candidateUserIds:JSON.parse(step.approver_pool_json ?? "[]"),
  }, steps.filter(s=>s.ordinal<step.ordinal && s.state==="approved").map(s=>s.approver_user_id));

  const forbidden = new Set(await personIdentityIds(db,instance.company_id,[
    claim.claimant_user_id,
    claim.prepared_by,
    ...steps
      .filter((s) => s.ordinal < step.ordinal && s.state === "approved")
      .map((s) => s.approver_user_id),
  ]));
  if (step.stage === "accounts_approval") {
    const accounting = JSON.parse(claim.accounting_json) as {
      preparedBy?: string;
    };
    if(accounting.preparedBy)for(const id of await personIdentityIds(db,instance.company_id,[accounting.preparedBy]))forbidden.add(id);
  }
  let current: string[] = [];
  if (step.stage === "department_approval") {
    const route = await legacyExpenseRoute(
      db,
      instance.company_id,
      claim.claimant_user_id,
      [claim.prepared_by],
    );
    current = route.steps[0]?.candidateUserIds ?? [];
  } else if (step.required_role)
    current = await expenseRoleUsers(db, instance, step.required_role);
  const saved = await personIdentityIds(db,instance.company_id,JSON.parse(step.approver_pool_json ?? "[]") as string[]);
  const candidates = current.filter(
    (id) => saved.includes(id) && !forbidden.has(id),
  );
  if (step.stage === "department_approval") return candidates;
  const payers = (await expenseRoleUsers(db, instance, "treasury")).filter(
    (id) => !forbidden.has(id),
  );
  if (step.stage === "accounts_approval")
    return candidates.filter((id) => payers.some((p) => p !== id));
  const managers = (
    await expenseRoleUsers(db, instance, "finance_approver")
  ).filter((id) => !forbidden.has(id));
  return candidates.filter((id) =>
    managers.some((m) => m !== id && payers.some((p) => p !== id && p !== m)),
  );
}

/** Employee resource requests reuse department eligibility, without expense finance stages. */
export function resourceRequestRoute(db: SqlDatabase, companyId: string, requester: string, preparer: string) {
  return legacyExpenseRoute(db, companyId, requester, [preparer], true);
}

/** Current employment, mapped level and valid reporting data, evaluated per participant. */
export async function spendingUsers(db:SqlDatabase,scope:RecordScope,role:string|null){
 return (await policyRead(db,JSON.stringify(["spending-users",scope.company_id,scope.branch_id,scope.department_id,role]),()=>readSpendingUsers(db,scope,role))).slice();
}
async function readSpendingUsers(db:SqlDatabase, scope:RecordScope, role:string|null) {
  const roster=await organizationRoster(db,scope.company_id);
  const accounts=new Set(await expenseRoleUsers(db,scope,role));
  const today=new Date().toISOString().slice(0,10);
  return roster.people.filter(p=>p.user_id && accounts.has(p.user_id) && p.status==="active" && p.person_status==="active" && p.start_date<=today && (!p.end_date||p.end_date>=today)
    && !reportingIssues(p,roster.people,roster.positions).length
    && (role!=="finance_approver" || (p.level_number!==null && p.level_number<=4)));
}

export async function expenseRoute(db:SqlDatabase, companyId:string, userId:string, excluded:string[]=[], departmentOnly=false, kind:SpendingKind="expense"):Promise<ApprovalRoute> {
  if(departmentOnly)return (await import("./department-authority")).departmentRoute(db,companyId,userId,excluded);
  let scope:RecordScope={company_id:companyId,branch_id:null,department_id:null};
  try {
    const guard=await expenseAuthoritySnapshot(db,companyId);
    const staff=await activeStaff(db,companyId,userId); scope=staff;
    const roster=await organizationRoster(db,companyId), person=roster.people.find(p=>p.engagement_id===staff.engagement_id);
    if(!person)throw new ApiError(409,"STAFF_LINK_REQUIRED","Complete the requester’s People profile.");
    const issues=reportingIssues(person,roster.people,roster.positions);
    // Explicit missing-supervisor exceptions are considered after base profile validation.
    const template=await spendingTemplate(db,companyId);
    const exception=template.exceptions.filter(e=>(!e.positionId||e.positionId===person.position_id)&&(!e.requestKind||e.requestKind===kind))
      .sort((a,b)=>(Number(!!b.positionId)*2+Number(!!b.requestKind))-(Number(!!a.positionId)*2+Number(!!a.requestKind)))[0];
    const remaining=issues.filter(i=>!exception || i!=="Assign a reporting manager.");
    if(remaining.length)throw new ApiError(409,"REQUESTER_SETUP_REQUIRED",remaining.join(" "));
    const forbidden=new Set([userId,...excluded]);
    const department=await (await import("./department-authority")).departmentRoute(db,companyId,userId,excluded);
    if(department.blockedReason)throw new ApiError(409,"DEPARTMENT_FLOW_REQUIRED",department.blockedReason);
    // The exception may exempt a missing manager, but never requester account/scope access.
    if(!(await expenseRoleUsers(db,scope,null)).includes(userId))throw new ApiError(409,"REQUESTER_SCOPE_REQUIRED","The requester’s account must cover this branch and department.");
    for(const step of department.steps)forbidden.add(step.userId);
    const pools=await Promise.all(["finance_preparer","finance_approver","treasury"].map(async role=>(await spendingUsers(db,scope,role)).map(p=>p.user_id!).filter(id=>!forbidden.has(id))));
    const [preparers,reviewers,payers]=pools;
    const feasible=preparers.some(p=>reviewers.some(r=>r!==p&&payers.some(t=>t!==p&&t!==r)));
    if(!feasible)throw new ApiError(409,"SPENDING_TEAM_REQUIRED","Complete independent Accounting preparer, Accounting approver (Level 1–4), and Finance/Treasury payer profiles and scoped access.");
    const snapshot={spendingPolicy:1,templateVersion:template.version,requestKind:kind,positionId:person.position_id,requesterLevel:person.level_number,exception:exception??null,excluded};
    return {scope,blockedReason:null,checks:[guard,...(department.checks??[])],steps:[
      ...department.steps.map(s=>({...s,snapshot:{...snapshot,...s.snapshot}})),
      {userId:preparers[0],authority:"named",candidateUserIds:preparers,requiredRole:"finance_preparer",stage:"accounts_review",snapshot},
      {userId:reviewers[0],authority:"named",candidateUserIds:reviewers,requiredRole:"finance_approver",stage:"accounts_approval",snapshot},
    ]};
  } catch(error){if(!(error instanceof ApiError))throw error;return {scope,steps:[],blockedReason:error.message};}
}

/** Revalidate saved participants; never resolve a new reporting chain behind an existing decision. */
export async function spendingStepCandidates(db:SqlDatabase, scope:RecordScope, requester:string, preparers:string[], step:{stage?:string;requiredRole?:string;candidateUserIds?:string[];snapshot?:Record<string,unknown>}, decisions:string[]) {
  if(step.stage==="department_approval" && step.snapshot && "departmentPolicyVersion" in step.snapshot){
    const id=step.candidateUserIds?.[0];
    return id && ![requester,...preparers,...decisions].includes(id) && await (await import("./department-authority")).departmentStepEligible(db,scope.company_id,id,step.snapshot)?[id]:[];
  }
  const forbidden=new Set(await personIdentityIds(db,scope.company_id,[requester,...preparers,...decisions]));
  const saved=await personIdentityIds(db,scope.company_id,step.candidateUserIds??[]);
  const members=await spendingUsers(db,scope,step.requiredRole??(step.stage==="department_approval"?"people_manager":null));
  const claimant=await activeStaff(db,scope.company_id,requester).catch(()=>null);
  if(!claimant?.level_number)return [];
  const roster=await organizationRoster(db,scope.company_id),person=roster.people.find(p=>p.engagement_id===claimant.engagement_id);
  const exception=step.snapshot?.exception as {action?:string}|undefined;
  if(!person||reportingIssues(person,roster.people,roster.positions).some(i=>!(i==="Assign a reporting manager."&&exception?.action)))return [];
  const candidates=members.filter(p=>saved.includes(p.user_id!)&&!forbidden.has(p.user_id!)&&(step.stage!=="department_approval"||p.level_number!<claimant.level_number!)).map(p=>p.user_id!);
  const payers=(await spendingUsers(db,scope,"treasury")).map(p=>p.user_id!).filter(id=>!forbidden.has(id));
  if(step.stage==="accounts_approval")return candidates.filter(id=>payers.some(p=>p!==id));
  const reviewers=(await spendingUsers(db,scope,"finance_approver")).map(p=>p.user_id!).filter(id=>!forbidden.has(id));
  if(step.stage==="accounts_review")return candidates.filter(id=>reviewers.some(r=>r!==id&&payers.some(p=>p!==id&&p!==r)));
  return candidates;
}
