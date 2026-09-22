import type { SqlDatabase } from "./runtime/types";
import { assertion } from "./commands";
import { z } from "zod";
import {
  resourceSlots,
  registeredResources,
  type RegistryEnv,
} from "../lib/holding-registry";
import { betaLimits, type BetaEnvironment } from "./beta-config";
import { ApiError } from "./http";

export const onboardingSchema = z.object({
  companyName: z.string().trim().min(2).max(100),
  branchName: z.string().trim().min(2).max(100),
  slug: z.string().regex(/^[a-z][a-z0-9-]{2,39}$/),
});
export type BetaUser = {
  id: string;
  name: string;
  email: string;
  emailVerified: boolean;
};
type Env = RegistryEnv & BetaEnvironment & { CONTROL_DB: SqlDatabase };
export function requireVerified(
  user: BetaUser | undefined,
): asserts user is BetaUser {
  if (!user) throw new ApiError(401, "UNAUTHENTICATED", "Sign in to continue.");
  if (!user.emailVerified)
    throw new ApiError(
      403,
      "VERIFY_EMAIL",
      "Verify your email before joining or creating a company.",
    );
}
export function memberStatements(
  db: SqlDatabase,
  company: string,
  branch: string,
  user: BetaUser,
  owner: boolean,
  invitation: string | null,
  grant?: { parentId: string; validUntil: number },
) {
  const now = Date.now(),
    person = `person-${user.id}`,
    department = `department-${branch}`;
  return [
    db
      .prepare(
        "INSERT INTO beta_members(user_id,invitation_id,joined_at) VALUES(?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(user.id, invitation, now),
    db
      .prepare(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'TH',?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(person, user.id, company, user.name, user.email, user.id, now, now),
    db
      .prepare(
        "INSERT INTO employees(id,user_id,company_id,branch_id,department_id,display_name,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        `employee-${user.id}`,
        user.id,
        company,
        branch,
        department,
        user.name,
        now,
      ),
    db
      .prepare(
        "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,?,'employee','salaried','active',to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'),'Team member','THB',?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        `engagement-${user.id}`,
        person,
        company,
        branch,
        department,
        user.id,
        now,
        now,
        `level-${company}`,
      ),
    db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) VALUES(?,?,?,'[]',?,?,?,? ,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        `beta-assignment-${user.id}`,
        user.id,
        owner ? "holding_owner" : "employee",
        company,
        owner ? null : branch,
        now,
        grant?.validUntil ?? now + 90 * 86400000,
        owner ? "holding" : "branch",
        owner ? null : branch,
        grant?.parentId ?? null,
        grant ? "manual" : "legacy",
      ),
  ];
}
export function branchStatements(
  db: SqlDatabase,
  company: string,
  id: string,
  name: string,
  slug: string,
) {
  return [
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(id, slug, name, company, company, id),
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department','team','Team',?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(`department-${id}`, id, company, id),
  ];
}
type SlotRow = {
  holding_id: string;
  slug: string;
  company_name: string;
  branch_name: string;
  state: string;
};
export async function onboard(
  env: Env,
  user: BetaUser,
  input: z.infer<typeof onboardingSchema>,
) {
  requireVerified(user);
  if (env.ERPX_PUBLIC_BETA !== "true")
    throw new ApiError(403, "BETA_CLOSED", "Public registration is closed.");
  const data = onboardingSchema.parse(input),
    db = env.CONTROL_DB,
    now = Date.now();
  let slot = await db
    .prepare("SELECT * FROM beta_slots WHERE owner_user_id=?")
    .bind(user.id)
    .first<SlotRow>();
  if (!slot) {
    if (
      await db
        .prepare("SELECT 1 FROM holding_bindings WHERE slug=?")
        .bind(data.slug)
        .first()
    )
      throw new ApiError(
        409,
        "SLUG_UNAVAILABLE",
        "Choose another company URL.",
      );
    try {
      slot = await db
        .prepare(
          `UPDATE beta_slots SET owner_user_id=?,slug=?,company_name=?,branch_name=?,state='provisioning',updated_at=? WHERE holding_id=(SELECT holding_id FROM beta_slots WHERE state='available' ORDER BY holding_id FOR UPDATE SKIP LOCKED LIMIT 1) AND state='available' AND NOT EXISTS(SELECT 1 FROM beta_slots WHERE owner_user_id=?) RETURNING *`,
        )
        .bind(
          user.id,
          data.slug,
          data.companyName,
          data.branchName,
          now,
          user.id,
        )
        .first<SlotRow>();
    } catch {
      slot = await db
        .prepare("SELECT * FROM beta_slots WHERE owner_user_id=?")
        .bind(user.id)
        .first<SlotRow>();
      if (!slot)
        throw new ApiError(
          409,
          "SLUG_UNAVAILABLE",
          "Choose another company URL.",
        );
    }
    if (!slot)
      slot = await db
        .prepare("SELECT * FROM beta_slots WHERE owner_user_id=?")
        .bind(user.id)
        .first<SlotRow>();
    if (!slot) {
      await db
        .prepare(
          "INSERT INTO beta_waitlist(user_id,created_at) VALUES(?,?) ON CONFLICT DO NOTHING",
        )
        .bind(user.id, now)
        .run();
      return { state: "waitlisted", href: null };
    }
  }
  if (
    slot.slug !== data.slug ||
    slot.company_name !== data.companyName ||
    slot.branch_name !== data.branchName
  )
    throw new ApiError(
      409,
      "ONBOARDING_ALREADY_STARTED",
      "Resume using the original company details.",
    );
  const resource = resourceSlots(env).find(
    (s) => s.id === slot.holding_id && s.beta,
  );
  if (!resource)
    throw new ApiError(
      503,
      "SLOT_UNAVAILABLE",
      "Your reserved company is being prepared. Try again shortly.",
    );
  const { write } = registeredResources(env, slot.holding_id),
    company = `company-${slot.holding_id}`,
    branch = `branch-${slot.holding_id}`,
    limits = betaLimits(env);
  // The holding transaction is the authoritative bootstrap boundary. The
  // control-plane activation below is deliberately retriable: a completed
  // receipt means the graph is safe to expose even when binding activation
  // previously failed.
  const bootstrapKey = `bootstrap-${slot.holding_id}`;
  const receipt = await write
    .prepare("SELECT status FROM company_provisioning WHERE company_id=? AND holding_id=?")
    .bind(company, slot.holding_id)
    .first<{ status: string }>();
  if (receipt?.status !== "active") {
    await write.batch([
      assertion(
        write,
        bootstrapKey,
        "bootstrap-owner",
        "NOT EXISTS(SELECT 1 FROM beta_bootstrap WHERE holding_id<>? OR owner_user_id<>?) AND (EXISTS(SELECT 1 FROM beta_bootstrap WHERE holding_id=? AND owner_user_id=?) OR NOT EXISTS(SELECT 1 FROM organization_nodes))",
        [slot.holding_id, user.id, slot.holding_id, user.id],
      ),
      write
        .prepare(
          "INSERT INTO company_provisioning(company_id,holding_id,owner_user_id,status,version,requested_at,updated_at) VALUES(?,?,?,'provisioning',1,?,?) ON CONFLICT(company_id) DO NOTHING",
        )
        .bind(company, slot.holding_id, user.id, now, now),
      write
        .prepare(
          "INSERT INTO company_provisioning_events(id,company_id,holding_id,action,version,actor_id,detail_json,created_at) VALUES(?,?,?,'started',1,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          `company-provisioning-started-${slot.holding_id}`,
          company,
          slot.holding_id,
          user.id,
          JSON.stringify({ companyName: slot.company_name, branchName: slot.branch_name, slug: slot.slug }),
          now,
        ),
      write
        .prepare(
          "INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(slot.holding_id, user.id, now),
      write
        .prepare(
          "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(company, slot.slug, slot.company_name, company),
      ...branchStatements(write, company, branch, slot.branch_name, "main"),
      write
        .prepare(
          "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,created_at,updated_at) VALUES(?,?,'member','Team member',1,NULL,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(`level-${company}`, company, now, now),
      ...[
        [1, "CEO"],
        [2, "Executive / Director"],
        [3, "Branch / Department Manager"],
        [4, "Supervisor"],
        [5, "Senior Staff"],
        [6, "Staff"],
      ].map(([number, name]) =>
        write
          .prepare(
            "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(company_id,code) DO UPDATE SET name=excluded.name,rank=excluded.rank,level_number=excluded.level_number,active=1,updated_at=excluded.updated_at",
          )
          .bind(
            `org-level-${company}-${number}`,
            company,
            `ORG_L${number}`,
            name,
            -Number(number),
            Number(number),
            now,
            now,
          ),
      ),
      write
        .prepare(
          "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,'Chief Executive Officer',?,NULL,1,1,1,?,?,?) ON CONFLICT(id) DO UPDATE SET level_id=excluded.level_id,is_ceo=1,active=1,version=organization_positions.version+1,updated_at=excluded.updated_at",
        )
        .bind(`company-ceo-${company}`, company, `org-level-${company}-1`, user.id, now, now),
      write
        .prepare(
          "INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          company,
          limits.members,
          limits.branches,
          limits.ai,
          limits.storage,
          now,
        ),
      ...memberStatements(write, company, branch, user, true, null),
      write
        .prepare(
          "UPDATE hr_engagements SET level_id=?,position_id=?,job_title='Chief Executive Officer',updated_at=? WHERE id=? AND company_id=?",
        )
        .bind(`org-level-${company}-1`, `company-ceo-${company}`, now, `engagement-${user.id}`, company),
      write
        .prepare(
          "UPDATE company_provisioning SET status='active',version=2,completed_at=?,updated_at=?,last_error=NULL WHERE company_id=? AND holding_id=?",
        )
        .bind(now + 1, now + 1, company, slot.holding_id),
      write
        .prepare(
          "INSERT INTO company_provisioning_events(id,company_id,holding_id,action,version,actor_id,detail_json,created_at) VALUES(?,?,?,'completed',2,?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(
          `company-provisioning-completed-${slot.holding_id}`,
          company,
          slot.holding_id,
          user.id,
          JSON.stringify({ levels: 6, ceoPositions: 1, branches: 1, departments: 1, owner: user.id }),
          now + 1,
        ),
      write
        .prepare("DELETE FROM command_assertions WHERE command_id=?")
        .bind(bootstrapKey),
    ]);
  }
  const completed = await write
    .prepare("SELECT status FROM company_provisioning WHERE company_id=? AND holding_id=? AND status='active'")
    .bind(company, slot.holding_id)
    .first<{ status: string }>();
  if (!completed) throw new ApiError(503, "PROVISIONING_INCOMPLETE", "Company setup is still being prepared. Try again shortly.");
  await db.batch([
    db
      .prepare(
        "INSERT INTO principals(id,user_id,display_name,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(`principal-${user.id}`, user.id, user.name, now),
    db
      .prepare(
        "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?,'active') ON CONFLICT(holding_id) DO NOTHING",
      )
      .bind(
        slot.holding_id,
        slot.slug,
        slot.company_name,
        resource.write,
        resource.read,
        resource.files,
      ),
    db
      .prepare(
        "INSERT INTO beta_billing(holding_id,updated_at) VALUES(?,?) ON CONFLICT DO NOTHING",
      )
      .bind(slot.holding_id, now),
    db
      .prepare(
        "UPDATE beta_slots SET state='active',updated_at=? WHERE holding_id=? AND owner_user_id=?",
      )
      .bind(now, slot.holding_id, user.id),
  ]);
  return { state: "active", href: `/holdings/${slot.slug}/home` };
}
