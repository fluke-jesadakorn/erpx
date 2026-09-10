/* Temporary business workflows and their visible prerequisites. */
const ORDER_STEPS = [
  step(
    "Confirmed",
    "Confirm fulfillment",
    "Fulfilled",
    "Confirm the delivery and open the finance handoff.",
    "",
    "finance",
  ),
];
const RECEIPT_STEPS = [
  step(
    "Expected",
    "Accept goods receipt",
    "Received",
    "Check the quantities and receipt evidence.",
    "evidence",
    "inventory",
  ),
];
const PERIOD_STEPS = [
  step(
    "Close review",
    "Close period",
    "Closed",
    "A different person reviews reconciliation for this company.",
    "independent",
  ),
];
function moduleReadOnly(k) {
  return (
    k === "customer_360" || (k === "commercial" && state.holding !== "platform")
  );
}
function recordReadOnly(r) {
  return !r || moduleReadOnly(r.module) || r.readOnly === true;
}
function flowSteps(r) {
  if (r.taskSteps) return r.taskSteps;
  return r.recordType === "sales_order"
    ? ORDER_STEPS
    : r.recordType === "goods_receipt"
      ? RECEIPT_STEPS
      : r.recordType === "period"
        ? PERIOD_STEPS
        : MODULES[r.module]?.steps || [];
}
function nextStep(r) {
  return r.finished ? null : flowSteps(r)[r.stage || 0];
}
function workflowRecord(k) {
  return (
    visibleRecords(k).find((r) => r.workflow && !r.parentId) ||
    visibleRecords(k).find((r) => r.workflow)
  );
}
function refOf(r, relation = "related to") {
  return {
    type: "record",
    holding: r.holding,
    module: r.module,
    id: r.id,
    relation,
  };
}
function resolveRef(ref) {
  return ref?.type === "record" ? findRecord(ref.module, ref.id) : null;
}
function referencesVisible(refs = []) {
  return refs.every(
    (ref) =>
      ref.type !== "record" ||
      (ref.holding === state.holding && !!resolveRef(ref)),
  );
}
function publicComments(r) {
  return r.comments.filter(
    (c) => c.visibility !== "internal" && referencesVisible(c.refs),
  );
}
function primaryCompany(r) {
  return (
    HOLDINGS[r.holding]?.branches.find((b) => b.id === r.branch)?.company ||
    r.branch
  );
}
function recordFields(r) {
  if (r.recordType === "employee_onboarding")
    return [
      specField("employee", "Employee"),
      specField("notes", "Onboarding details", "textarea"),
    ];
  if (r.recordType === "sales_order")
    return MODULES.sales.fields.filter((f) => f.key !== "terms");
  return r.recordType === "goods_receipt"
    ? [...MODULES.inventory.fields]
    : r.recordType === "period"
      ? MODULES.finance.fields.filter((f) =>
          ["reconciled", "notes"].includes(f.key),
        )
      : MODULES[r.module].fields;
}
function createWorkflow(k, hid, branch, suffix = "") {
  const c = MODULES[k],
    id = `WF-${k.toUpperCase()}${suffix}`;
  return {
    id,
    module: k,
    holding: hid,
    branch,
    department: ["finance", "commercial"].includes(k)
      ? "Finance"
      : ["people_payroll", "talent", "benefits", "learning"].includes(k)
        ? "People"
        : "Operations",
    title: c.title,
    subtitle: c.kind,
    recordType: c.kind.toLowerCase().replaceAll(" ", "_"),
    workflow: true,
    ...(k === "quality" && suffix
      ? {
          taskSteps: [
            step(
              "Open",
              "Record inspection",
              "Passed",
              "Record the observed result and temperature.",
              "observation",
            ),
            ...c.steps.slice(1),
          ],
        }
      : {}),
    stage: 0,
    status: c.steps[0]?.status || "Active",
    version: 1,
    created: Date.now(),
    due: Date.now() + (k === "support" ? 1 : k === "sales" ? 48 : 72) * 3600000,
    fields: Object.fromEntries(c.fields.map((f) => [f.key, f.value])),
    comments: [],
    refs: [],
    history: [
      {
        by: "Demo preparation",
        text: "Prepared a fictional " + c.kind.toLowerCase() + ".",
        time: "Today",
      },
    ],
    evidence: [],
    createdBy: branch === "warehouse" ? "dao" : "mali",
  };
}
function seedExpansion() {
  state.sequence = 0;
  state.targets = [];
  state.wfDrafts = {};
  state.editorRefs = {};
  state.suggestions = {};
  state.periods = {};
  state.customTopics = [];
  state.capabilityRuns ||= {};
  state.attachmentDrafts ||= {};
  state.taskCommentDrafts ||= {};
  state.taskDetails ||= {};
  state.organizationSeed = clone(
    Object.fromEntries(
      Object.entries(HOLDINGS).map(([id, h]) => [
        id,
        h.branches.filter((b) => !b.demoCreated),
      ]),
    ),
  );
  for (const [id, branches] of Object.entries(state.organizationSeed))
    HOLDINGS[id].branches = branches;
  personas.pim = {
    name: "Pim Wong",
    av: "PW",
    tone: "green",
    description: "Independent finance reviewer",
    assignments: [
      {
        holding: "acme",
        role: "finance",
        branches: ["all", "bkk", "cnx", "warehouse"],
        departments: ["Finance"],
      },
      {
        holding: "northstar",
        role: "finance",
        branches: ["all", "hq", "east"],
        departments: ["Finance"],
      },
    ],
  };
  for (const hid of ["acme", "northstar", "platform"]) {
    const keys =
      hid === "platform"
        ? ["platform_administration", "commercial"]
        : Object.keys(MODULES).filter((k) => k !== "platform_administration");
    for (const k of keys) {
      const r = createWorkflow(
        k,
        hid,
        hid === "platform" ? "all" : HOLDINGS[hid].branches[0].id,
      );
      if (hid === "northstar") {
        r.title = r.title
          .replaceAll("Riverstone", "Harbor")
          .replaceAll("Bangkok", "Head office");
        for (const [key, value] of Object.entries(r.fields))
          if (typeof value === "string")
            r.fields[key] = value
              .replaceAll("Riverstone", "Harbor")
              .replaceAll("Acme Retail Co", "Northstar Supply Co");
      }
      if (k === "organization" && hid !== "platform")
        r.fields.company = HOLDINGS[hid].companies[0].name;
      records.push(r);
      if (hid === "acme" && roles.warehouse.modules.includes(k))
        records.push(createWorkflow(k, hid, "warehouse", "-WH"));
    }
  }
  for (const hid of ["acme", "northstar"]) {
    const branch = HOLDINGS[hid].branches[0].id;
    for (const [module, type, title, status] of [
      [
        "sales",
        "sales_order",
        "Store essentials · confirmed order",
        "Confirmed",
      ],
      [
        "procurement",
        "purchase_order",
        "Store essentials · supplier order",
        "Ordered",
      ],
      [
        "procurement",
        "goods_receipt",
        "Store essentials · awaiting receipt",
        "Expected",
      ],
    ]) {
      const r = createWorkflow(module, hid, branch, "-" + type.toUpperCase());
      r.recordType = type;
      r.title = title;
      r.status = status;
      r.parentId = "WF-" + module.toUpperCase();
      r.refs = [
        {
          type: "record",
          holding: hid,
          module,
          id: r.parentId,
          relation: "created from",
        },
      ];
      if (type === "purchase_order") {
        r.finished = true;
        r.readOnly = true;
      }
      if (type === "goods_receipt")
        r.fields = {
          sku: "TOTE-CANVAS-01",
          quantity: 100,
          expected: 100,
          notes: "Check cartons against the supplier order.",
        };
      records.push(r);
    }
  }
  for (const r of records) {
    if (r.module === "people_payroll" && r.workflow && r.branch === "warehouse")
      r.fields.employee = "Dao Arun";
    r.refs ||= [];
    r.evidence ||= [];
    r.created ||= Date.now() - 7200000;
    r.due ||= Date.now() + 86400000;
    r.createdBy ||= "mali";
    if (r.workflow) {
      const journey = Object.values(JOURNEYS).find((j) =>
        j.modules.includes(r.module),
      );
      for (const mk of journey?.modules || [
        "projects",
        "procurement",
        "inventory",
      ]) {
        const target = records.find(
          (x) =>
            x.workflow &&
            x.holding === r.holding &&
            x.branch === r.branch &&
            x.module === mk,
        );
        if (target && target.id !== r.id)
          r.refs.push(
            refOf(
              target,
              journey?.edges[Math.max(0, journey.modules.indexOf(r.module))] ||
                "related to",
            ),
          );
      }
      if (r.module === "customer_360")
        for (const mk of ["crm", "sales", "support"]) {
          const target = records.find(
            (x) =>
              x.workflow &&
              x.holding === r.holding &&
              x.branch === r.branch &&
              x.module === mk,
          );
          if (target) r.refs.push(refOf(target));
        }
    } else {
      for (const [ref, relation] of SEED.EDGES[r.module] || []) {
        const [mk, id] = ref.split("/");
        const target = records.find(
          (x) =>
            x.holding === r.holding &&
            x.module === mk &&
            x.id === (r.holding === "northstar" ? "NS-" : "") + id,
        );
        if (target) r.refs.push(refOf(target, relation));
      }
    }
  }
  for (const hid of ["acme", "northstar"])
    posts.push({
      id: hid + "-broadcast",
      holding: hid,
      branch: null,
      department: null,
      kind: "system",
      by: "Workspace service",
      av: "WS",
      tone: "violet",
      text: "The sample workspace recovery check completed. Your team can continue working.",
      time: "Today · 08:30",
      likes: 0,
      comments: [],
      tags: ["#workspace"],
    });
  posts.forEach((p, i) => {
    p.created = Date.now() - (i + 1) * 1800000;
    p.tags ||= (SEED.CTX[p.module]?.tags || ["#team"]).slice(0, 2);
    p.refs ||= [];
  });
  // A few follow defaults let the chronological feed explain itself immediately.
  state.targets.push(
    "acme:mali:person:Nisa Chen",
    "acme:mali:tag:#quote",
    "acme:nisa:tag:#month-close",
  );
  seedTasks();
}
function wfDraftKey(r) {
  return ctxKey() + ":" + r.id;
}
function wfData(r) {
  return state.wfDrafts[wfDraftKey(r)] || r.fields;
}
function wfFieldsLocked(r) {
  return (
    recordReadOnly(r) ||
    r.finished ||
    (r.module === "finance" &&
      r.recordType !== "period" &&
      ["Posted", "Close review", "Closed"].includes(r.status))
  );
}
function wfPermission(r) {
  if (recordReadOnly(r) || !isVisible(r) || state.demoState === "expired")
    return false;
  if (!r.workflow) return canEdit(r);
  return taskRoleEligible(r, state.role) && taskEligible(r, state.persona);
}
function prerequisites(r, s = nextStep(r), v = r.fields, fromTask = false) {
  const issues = [];
  if (!wfPermission(r))
    issues.push("This action requires an assigned reviewer role.");
  if (
    [
      "offline",
      "degraded",
      "stale",
      "source-conflict",
      "expired",
      "prerequisite",
    ].includes(state.demoState)
  )
    issues.push(
      "Resolve the " + state.demoState + " demo scenario before continuing.",
    );
  if (!fromTask && state.wfDrafts[wfDraftKey(r)])
    issues.push("Save your draft details before reviewing this action.");
  if (!s) return issues;
  if (s.gate === "brand" && v.locked)
    issues.push(
      "This category is locked. Clear the sample category lock before publishing.",
    );
  if (s.gate === "renewal" && (!(v.days > 0) || v.days > 90))
    issues.push(
      "Renewal must be between 1 and 90 days, inside the parent assignment.",
    );
  if (s.gate === "notes" && !String(v.notes || "").trim())
    issues.push("Add a useful explanation before continuing.");
  if (s.gate === "balance") {
    if (
      !(v.debit > 0) ||
      Math.round(v.debit * 100) !== Math.round(v.credit * 100)
    )
      issues.push("Debits and credits must balance and be greater than zero.");
    if (v.currency !== "USD" && !(v.fx > 0))
      issues.push("A positive exchange rate is required.");
    if (state.periods[r.holding + ":" + primaryCompany(r)] === "Closed")
      issues.push("This company period is closed. Posting is blocked.");
  }
  if (s.gate === "reconciled" && !v.reconciled)
    issues.push("Complete bank and subledger reconciliation first.");
  if (s.gate === "independent") {
    if (!v.reconciled)
      issues.push("Complete bank and subledger reconciliation before closing.");
    if (r.preparedBy === state.persona || r.closePreparedBy === state.persona)
      issues.push(
        "The preparer cannot close this period. Preview Pim Wong for independent review.",
      );
    if (!["finance", "owner"].includes(state.role))
      issues.push("An assigned finance reviewer is required.");
  }
  const total = Number(v.quantity || 0) * Number(v.unitPrice || 0);
  if (
    s.gate === "tender" &&
    (!(v.quantity > 0) || !(v.unitPrice > 0) || v.tender < total)
  )
    issues.push(
      "Add a positive basket quantity and enough tender to cover " +
        money(total) +
        ".",
    );
  if (
    s.gate === "refund" &&
    (!(v.refund > 0) ||
      v.refund > Number(r.settled || total) - Number(r.refunded || 0))
  )
    issues.push("Refund must be positive and within the original settlement.");
  if (
    (s.gate === "quantity" || r.recordType === "goods_receipt") &&
    (!(v.quantity > 0) || v.quantity > v.expected)
  )
    issues.push(
      "Received quantity must be positive and within the expected amount.",
    );
  if (s.gate === "evidence" && !r.evidence.length)
    issues.push("Attach completion evidence before this action.");
  if (
    s.gate === "observation" &&
    (!Number.isFinite(v.reading) ||
      (v.result === "Pass" && (v.reading < 0 || v.reading > 5)))
  )
    issues.push("A passing inspection must measure between 0°C and 5°C.");
  if (s.gate === "failed-check" && v.result !== "Fail")
    issues.push(
      "Record the initial failed inspection before entering its recheck.",
    );
  if (
    s.gate === "quality" &&
    (v.result !== "Pass" || v.reading < 0 || v.reading > 5)
  )
    issues.push("A passing recheck between 0°C and 5°C is required.");
  if (s.gate === "hold" && v.hold)
    issues.push(
      "An active legal hold prevents archiving. Resolve the sample hold first.",
    );
  if (s.gate === "verified" && !v.verified)
    issues.push("Verify the sample channel before scheduling.");
  if (s.gate === "materials" && !v.materials)
    issues.push("Required materials are unavailable.");
  if (
    s.gate === "manufacturing" &&
    (!v.quality || !(v.produced > 0) || v.produced > v.quantity)
  )
    issues.push("Approve the quality disposition and enter valid good output.");
  if (s.gate === "reference" && !r.refs.some((x) => resolveRef(x)))
    issues.push("Link an accessible dependency first.");
  if (s.gate === "progress" && v.progress !== 100)
    issues.push("Set the completed milestone progress to 100%.");
  if (s.gate === "stock" && !(v.stock > 0))
    issues.push("Sample checkout needs available stock.");
  if (
    s.gate === "enrollment" &&
    (!v.window ||
      (v.coverage === "Employee + dependent" &&
        (!v.dependent.trim() || !v.validated)))
  )
    issues.push(
      "The enrollment window must be open; dependent details must be present and verified.",
    );
  if (s.gate === "attended" && !v.attended)
    issues.push("Record sample lesson attendance first.");
  if (s.gate === "score" && (!(v.score >= 70) || v.score > 100))
    issues.push("A result between 70% and 100% is required.");
  return issues;
}
function money(n) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  }).format(Number(n) || 0);
}
function wfHistory(r, text) {
  r.history.unshift({
    by: person().name + " · " + role().name,
    text,
    time: "Just now",
  });
  r.version++;
  r.created = Date.now();
}
function childRecord(r, module, type, status, title) {
  const child = createWorkflow(
    module,
    r.holding,
    r.branch,
    "-" + type + "-" + ++state.sequence,
  );
  child.recordType = type;
  child.status = status;
  child.title = title;
  child.parentId = r.id;
  child.refs = [refOf(r, "created from")];
  child.fields = { ...child.fields, ...(module === r.module ? r.fields : {}) };
  child.createdBy = state.persona;
  wfHistory(child, "Created from " + r.id + ".");
  records.push(child);
  r.refs.push(refOf(child, "created"));
  return child;
}
function commitWorkflow(r, s, publish = true) {
  let target = r;
  r.stage = flowSteps(r).findIndex((x) => x.action === s.action) + 1;
  r.status = s.next;
  if (s.gate === "observation") {
    r.status = r.fields.result === "Pass" ? "Passed" : "Failed";
    r.stage = r.fields.result === "Pass" ? 2 : 1;
  }
  r.finished = r.stage >= flowSteps(r).length;
  if (r.module === "finance" && s.action === "Prepare close") {
    target = childRecord(
      r,
      "finance",
      "period",
      "Close review",
      "September · company period close",
    );
    target.preparedBy = state.persona;
    target.fields.reconciled = true;
    r.status = "Posted";
    r.finished = true;
  }
  if (s.gate === "independent")
    state.periods[r.holding + ":" + primaryCompany(r)] = "Closed";
  if (r.module === "pos" && s.action === "Complete sale")
    r.settled = r.fields.quantity * r.fields.unitPrice;
  if (r.module === "pos" && s.action === "Confirm refund")
    r.refunded = (r.refunded || 0) + r.fields.refund;
  if (r.module === "identity_access" && s.next === "Active")
    r.validUntil = new Date(
      Date.now() + r.fields.days * 86400000,
    ).toLocaleDateString("en-US");
  if (r.module === "procurement" && s.action === "Create purchase order") {
    const po = childRecord(
      r,
      "procurement",
      "purchase_order",
      "Ordered",
      r.title + " · purchase order",
    );
    po.finished = true;
    target = childRecord(
      po,
      "procurement",
      "goods_receipt",
      "Expected",
      r.title + " · goods receipt",
    );
    target.fields = {
      ...r.fields,
      sku: "TOTE-CANVAS-01",
      expected: r.fields.quantity,
    };
    target.stage = 0;
    r.finished = true;
    r.status = "Approved";
  }
  if (s.handoff === "sales-order") {
    target = childRecord(
      r,
      "sales",
      "sales_order",
      "Confirmed",
      r.title + " · sales order",
    );
    target.fields.amount = r.fields.amount || r.fields.price || 2400;
    target.stage = 0;
    r.finished = true;
    r.status = r.module === "sales" ? "Accepted" : "Ordered";
  }
  if (s.handoff === "organization") {
    const h = HOLDINGS[r.holding],
      company =
        h.companies.find((c) => c.name === r.fields.company) || h.companies[0];
    if (!h.branches.some((b) => b.id === "demo-" + r.id))
      h.branches.push({
        id: "demo-" + r.id,
        name: r.fields.name,
        company: company.id,
        demoCreated: true,
      });
  }
  const handoff = [
    "finance",
    "inventory",
    "sales",
    "procurement",
    "people_payroll",
  ].includes(s.handoff)
    ? records.find(
        (x) =>
          x.workflow &&
          x.holding === r.holding &&
          x.branch === r.branch &&
          x.module === s.handoff &&
          !x.parentId,
      )
    : null;
  if (handoff) {
    if (!r.refs.some((x) => x.id === handoff.id))
      r.refs.push(refOf(handoff, "hands off to"));
    if (!handoff.refs.some((x) => x.id === r.id))
      handoff.refs.push(refOf(r, "created from"));
    r.handoff = refOf(handoff);
  }
  wfHistory(
    r,
    s.action + " → " + r.status + ". Recorded by " + person().name + ".",
  );
  if (publish && r.holding !== "platform")
    publishRecordUpdate(r, s.action + " completed for " + r.id + ".");
  return target;
}
function readWorkflowForm(form, r) {
  const data = {};
  for (const f of recordFields(r)) {
    const el = form.elements[f.key];
    if (el)
      data[f.key] =
        f.type === "checkbox"
          ? el.checked
          : f.type === "number"
            ? Number(el.value)
            : el.value.trim();
  }
  return data;
}
function draftAssistant(r) {
  const key = wfDraftKey(r),
    s = state.suggestions[key];
  const sources = (r.refs || []).map(resolveRef).filter(Boolean).slice(0, 3);
  const editable = r.workflow
    ? recordFields(r).some((f) => f.type === "textarea" || f.type === "text")
    : (SEED.FIELDS[r.module] || []).some((f) => f[1] === "text");
  return `<section class="card card-pad assistant-card"><div class="between"><h2>${icon("sparkle")} Draft assistant</h2><span class="badge gray">Simulated</span></div><p class="small muted">A proposal you can inspect before using.</p>${s ? `<div class="suggestion-state">${badge(s.status)}</div>${s.status === "Ready" || s.status === "Applied to draft" || s.status === "Conflicted" ? `<div class="diff-value"><small>Current</small><p>${esc(s.before || "Empty")}</p><small>Proposed</small><p>${esc(s.after)}</p></div>` : ""}<p class="tiny muted">${esc(s.message || "Prepared from pinned sample records. No model was called.")}</p>` : ""}<div class="evidence-list"><strong class="small">Source evidence</strong>${sources.map((x) => `<a href="${recordRoute(x)}" class="reference">${icon("link")}<span>${esc(x.title)}<small>Version ${x.version} · ${esc(x.id)}</small></span></a>`).join("") || '<p class="small muted">Evidence gap: link accessible records to support a draft.</p>'}${(r.refs || []).length > sources.length ? '<p class="tiny muted">Some sources are unavailable or outside this preview. Their content is omitted.</p>' : ""}</div>${wfFieldsLocked(r) || !editable ? '<p class="notice">This surface is read-only. Open the owning record to change its details.</p>' : `<div class="flex wrap"><button class="btn soft" data-action="wf-suggest" data-module="${r.module}" data-record="${esc(r.id)}">${s ? "Request again" : "Suggest a draft"}</button>${s?.status === "Ready" ? `<button class="btn primary" data-action="wf-apply-suggestion" data-module="${r.module}" data-record="${esc(r.id)}">Apply to draft</button>` : ""}${s ? `<button class="btn" data-action="wf-reject-suggestion" data-module="${r.module}" data-record="${esc(r.id)}">Reject</button>` : ""}</div>`}</section>`;
}
function requestSuggestion(r) {
  const key = wfDraftKey(r),
    context = ctxKey(),
    refs = r.refs.map(resolveRef).filter(Boolean).slice(0, 3),
    field = r.workflow
      ? recordFields(r).find((f) => f.key === "notes" || f.type === "textarea")
      : null;
  const fieldKey =
    field?.key || SEED.FIELDS[r.module]?.find((f) => f[1] === "text")?.[0];
  if (!fieldKey || wfFieldsLocked(r)) return;
  const s = {
    status: "Queued",
    field: fieldKey,
    before: (r.workflow ? wfData(r) : r.fields)[fieldKey],
    hostVersion: r.version,
    sources: refs.map((x) => ({ ref: refOf(x), version: x.version })),
    after: refs.length
      ? `${MODULES[r.module].kind}: ${refs.map((x) => x.title).join("; ")}. Confirm the next handoff with the responsible team.`
      : "Please add source records and confirm the next handoff before publishing.",
    message: refs.length
      ? "Deterministic sample · source versions captured."
      : "Evidence gap: this generic draft has no pinned sources.",
  };
  state.suggestions[key] = s;
  render();
  setTimeout(() => {
    if (ctxKey() !== context || state.suggestions[key] !== s) return;
    s.status = "Preparing";
    render();
  }, 250);
  setTimeout(() => {
    if (ctxKey() !== context || state.suggestions[key] !== s) return;
    s.status =
      state.demoState === "failure"
        ? "Failed"
        : state.demoState === "source-conflict"
          ? "Conflicted"
          : "Ready";
    render();
  }, 700);
}
document.addEventListener("click", (event) => {
  const b = event.target.closest('[data-action^="wf-"]');
  if (!b) return;
  const { action, module: k, record: id } = b.dataset,
    r = k && id ? findRecord(k, id) : null;
  if (action === "wf-new") {
    if (!moduleAllowed(k) || moduleReadOnly(k)) return;
    const newRecord = createWorkflow(
      k,
      state.holding,
      state.branch === "all"
        ? holding()?.branches[0]?.id || "all"
        : state.branch,
      "-" + Date.now().toString().slice(-8),
    );
    newRecord.createdBy = state.persona;
    if (k === "people_payroll") newRecord.fields.employee = person().name;
    records.unshift(newRecord);
    taskNavigate(newRecord);
    notify("New sample draft. Edit the details before continuing.");
    return;
  }
  if (!r) return;
  if (action === "wf-review") {
    taskNavigate(r);
    return;
  }
  if (action === "wf-evidence" && wfPermission(r)) {
    r.evidence.push({
      name: "Verified work evidence " + (r.evidence.length + 1),
      by: person().name,
      asset: "assets/warehouse.svg",
    });
    wfHistory(r, "Attached local sample evidence.");
    render();
  }
  if (action === "wf-suggest") requestSuggestion(r);
  if (action === "wf-reject-suggestion") {
    if (state.suggestions[wfDraftKey(r)])
      state.suggestions[wfDraftKey(r)].status = "Rejected";
    render();
  }
  if (action === "wf-apply-suggestion") {
    const s = state.suggestions[wfDraftKey(r)];
    if (!s || s.status !== "Ready" || wfFieldsLocked(r)) return;
    if (
      !wfPermission(r) ||
      r.version !== s.hostVersion ||
      state.demoState === "source-conflict" ||
      s.sources.some(
        (x) => !resolveRef(x.ref) || resolveRef(x.ref).version !== x.version,
      )
    ) {
      s.status = "Conflicted";
      s.message = "A source or permission changed. Request a new draft.";
      render();
      return;
    }
    if (r.workflow)
      state.wfDrafts[wfDraftKey(r)] = { ...wfData(r), [s.field]: s.after };
    else
      state.recordDrafts[ctxKey() + ":" + r.id] = {
        ...r.fields,
        [s.field]: s.after,
      };
    s.status = "Applied to draft";
    render();
    notify("Applied to your unsaved draft. Review and save explicitly.");
  }
  if (action === "wf-confirm") {
    const s = nextStep(r),
      status = $("wf-status");
    if (!s) return;
    if (!$("wf-confirm").checked) {
      status.textContent = "Confirm that you reviewed the proposed outcome.";
      return;
    }
    if (r.version !== Number(b.dataset.version)) {
      status.textContent = "The record changed. Reopen the review.";
      return;
    }
    const issues = prerequisites(r, s);
    if (issues.length) {
      status.textContent = issues.join(" ");
      return;
    }
    const context = ctxKey(),
      versionAtReview = r.version,
      note = $("wf-decision").value;
    runSave(status, b, () => {
      if (
        context !== ctxKey() ||
        versionAtReview !== r.version ||
        prerequisites(r, s).length
      ) {
        status.textContent = "Context or record changed. Reopen the review.";
        return;
      }
      const target = commitWorkflow(r, s);
      if (note) r.history[0].text += " " + note;
      closeDialog();
      history.pushState(null, "", recordRoute(target));
      render();
      window.scrollTo(0, 0);
      openDialog(
        "Action complete",
        `<div class="result"><div class="success-symbol">${icon("check")}</div><h3>${esc(s.action)} complete</h3><p>${esc(r.title)} · ${esc(r.status)}</p><span class="badge gray">Sample outcome · recorded in history</span></div>${r.handoff && resolveRef(r.handoff) ? `<a class="handoff-card" href="${recordRoute(resolveRef(r.handoff))}">${icon("arrow")}Continue in ${esc(SEED.CTX[r.handoff.module].n)}<strong>${esc(resolveRef(r.handoff).title)}</strong></a>` : ""}`,
        `<button class="btn primary" data-action="close-dialog">Back to work</button>`,
      );
    });
  }
});
document.addEventListener("input", (event) => {
  const form = event.target.closest('[data-form="wf-save"]');
  if (!form) return;
  const r = findRecord(form.dataset.module, form.dataset.record);
  if (r) {
    state.wfDrafts[wfDraftKey(r)] = readWorkflowForm(form, r);
    $("wf-save-status").textContent =
      "Unsaved draft · kept in this work context";
  }
});
document.addEventListener("submit", (event) => {
  const f = event.target;
  if (f.dataset.form === "wf-save") {
    event.preventDefault();
    const r = findRecord(f.dataset.module, f.dataset.record);
    if (!r || !wfPermission(r) || wfFieldsLocked(r)) return;
    const values = readWorkflowForm(f, r),
      expected = r.version;
    state.wfDrafts[wfDraftKey(r)] = values;
    runSave($("wf-save-status"), f.querySelector('[type="submit"]'), () => {
      if (!wfPermission(r) || r.version !== expected) return;
      if (!referencesVisible(formReferences(f))) {
        $("wf-save-status").textContent =
          "A field reference is unavailable. Review it before saving.";
        return;
      }
      retainFieldReferences(r, f);
      r.fields = values;
      wfHistory(r, "Saved draft details.");
      delete state.wfDrafts[wfDraftKey(r)];
      render();
      notify("Sample details saved.");
    });
  }
  if (f.dataset.form === "wf-comment") {
    event.preventDefault();
    const r = findRecord(f.dataset.module, f.dataset.record);
    if (!r) return;
    const v = new FormData(f),
      text = String(v.get("text") || "").trim();
    if (!text) return;
    r.comments.push({
      id: "message-" + Date.now(),
      by: person().name,
      text,
      refs: editorRefsFor(f.querySelector("textarea").id),
      visibility: v.get("visibility") || "public",
      attachments: takeAttachmentDraft(r),
    });
    wfHistory(
      r,
      "Added a " +
        (v.get("visibility") === "internal"
          ? "private internal note"
          : "discussion reply") +
        ".",
    );
    delete state.editorRefs[editorKey(f.querySelector("textarea").id)];
    delete state.taskCommentDrafts[wfDraftKey(r)];
    render();
    notify("Comment added to the owning record.");
  }
});

window.ERPX_WORKFLOWS = {
  workflowRecord,
  publicComments,
  seedExpansion,
  draftAssistant,
};
