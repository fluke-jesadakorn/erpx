import type { SqlDatabase, FileStore } from "../../src/server/runtime/types";
import type { WorkContext } from "../../src/server/authorization";
const key = () => crypto.randomUUID();
export async function seedExpenseCompany(db: SqlDatabase, files: FileStore) {
  await db.prepare("INSERT INTO beta_bootstrap(singleton,holding_id,owner_user_id,created_at) VALUES(1,'h001','fixture',1) ON CONFLICT(singleton) DO NOTHING").run();
  const company = `expense-${key()}`,
    branch = `${company}-branch`,
    department = `${company}-team`,
    accountsDepartment = `${company}-accounts`,
    chart = `${company}-chart`,
    period = `${company}-period`;
  await db.batch([
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Expense test',?)",
      )
      .bind(company, company, company),
    db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Branch',?,?,?)",
      )
      .bind(branch, branch, company, company, branch),
    ...[department, accountsDepartment].map((id) =>
      db
        .prepare(
          "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Department',?,?,?)",
        )
        .bind(id, id, branch, company, branch),
    ),
    ...[1, 5, 10].map((rank) =>
      db
        .prepare(
          "INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,?,?,?,1,1)",
        )
        .bind(
          `${company}-level-${rank}`,
          company,
          String(rank),
          `Level ${rank}`,
          rank,
        ),
    ),
    db
      .prepare(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Chart','THB','fixture',1,1)",
      )
      .bind(chart, company),
    db
      .prepare(
        "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'2026','2026-01-01','2026-12-31',1)",
      )
      .bind(period, company),
  ]);
  const accounts = {
    expense: `${company}-expense`,
    tax: `${company}-tax`,
    payable: `${company}-payable`,
    cash: `${company}-cash`,
    revenue: `${company}-revenue`,
  };
  await db.batch(
    Object.entries(accounts).map(([name, id]) =>
      db
        .prepare(
          "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)",
        )
        .bind(
          id,
          chart,
          company,
          name,
          name,
          name === "expense"
            ? "expense"
            : name === "revenue"
              ? "revenue"
              : name === "payable"
                ? "liability"
                : "asset",
        ),
    ),
  );
  await db
    .prepare("INSERT INTO commerce_settings VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
    .bind(
      company,
      chart,
      "THB",
      accounts.cash,
      accounts.payable,
      accounts.revenue,
      accounts.cash,
      accounts.payable,
      accounts.expense,
      accounts.tax,
      accounts.payable,
      accounts.cash,
    )
    .run();
  async function member(
    name: string,
    role = "employee",
    rank = 1,
    dept = department,
    profile = true,
  ) {
    const userId = `${company}-${name}`,
      assignmentId = `grant-${userId}`;
    await db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')",
      )
      .bind(assignmentId, userId, role, company)
      .run();
    if (profile)
      await db.batch([
        db
          .prepare(
            "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)",
          )
          .bind(`person-${userId}`, userId, company, name),
        db
          .prepare(
            "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Team','THB','fixture',1,1,?)",
          )
          .bind(
            `eng-${userId}`,
            `person-${userId}`,
            company,
            branch,
            dept,
            `${company}-level-${rank}`,
          ),
      ]);
    return {
      userId,
      assignmentId,
      role,
      companyId: company,
      branchId: null,
      departmentId: null,
      holdingId: "h001",
      holdingSlug: "test",
      holdingName: "Test",
      name,
      email: `${name}@example.test`,
      assignmentScope: "holding",
      inherited: false,
      modules: [],
      assignments: [],
      workspaces: [],
      scope: {
        kind: "holding",
        holdingId: "h001",
        holdingSlug: "test",
        holdingName: "Test",
        companyId: company,
      },
    } as WorkContext;
  }
  const claimant = await member("claimant"),
    head = await member("head", "people_manager", 5),
    tie = await member("tie", "people_manager", 5),
    reviewer = await member("reviewer", "employee", 1, accountsDepartment),
    manager = await member("manager", "employee", 1, accountsDepartment),
    payer = await member("payer", "employee", 1, accountsDepartment),
    owner = await member("owner", "holding_owner", 10, accountsDepartment),
    it = await member("it", "employee", 1, accountsDepartment);
  await db
    .prepare("UPDATE hr_engagements SET manager_person_id=? WHERE id=? AND company_id=?")
    .bind(`person-${head.userId}`, `eng-${claimant.userId}`, company)
    .run();
  async function grant(
    actor: WorkContext,
    role: string,
    scope = "holding",
    node: string | null = null,
  ) {
    const id = key();
    await db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,scope_node_id) VALUES(?,?,?,'[]',?,0,9999999999999,?,?)",
      )
      .bind(id, actor.userId, role, company, scope, node)
      .run();
    return id;
  }
  reviewer.assignmentId = await grant(reviewer, "finance_preparer");
  reviewer.role = "finance_preparer";
  manager.assignmentId = await grant(manager, "finance_approver");
  manager.role = "finance_approver";
  payer.assignmentId = await grant(payer, "treasury");
  payer.role = "treasury";
  it.assignmentId = await grant(it, "it_admin");
  it.role = "it_admin";
  await grant(owner, "people_manager");
  async function attach(
    id: string,
    ownerType = "expense_claim",
    checksum = key(),
    type = "image/png",
  ) {
    const upload = key(),
      attachment = `attachment-${upload}`,
      object = `test-evidence/${upload}`;
    const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
    await files.put(object, bytes);
    await db.batch([
      db
        .prepare(
          "INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id,detected_media_type) VALUES(?,?,?,'evidence','receipt.png',?,8,?,'ready',?,9999999999999,1,1,?,?)",
        )
        .bind(upload, ownerType, id, type, object, claimant.userId, company, type),
      db
        .prepare(
          "INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES(?,?,?,?,'evidence',?,1,'receipt.png',?,8,?,?,1,?,?,'available',1,1)",
        )
        .bind(
          attachment,
          upload,
          ownerType,
          id,
          object,
          type,
          checksum,
          claimant.userId,
          company,
          type,
        ),
    ]);
    return attachment;
  }
  return {
    db,
    company,
    branch,
    department,
    accountsDepartment,
    chart,
    period,
    accounts,
    claimant,
    head,
    tie,
    reviewer,
    manager,
    payer,
    owner,
    it,
    member,
    grant,
    attach,
  };
}
