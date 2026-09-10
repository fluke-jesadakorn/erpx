/* Task-first interaction model. Everything here is temporary demo state. */
const TASK_PREVIEWS = new Set([
  "Preview identity",
  "Review catalog",
  "Review renewal",
  "Review placement",
  "Review retry",
  "Review journal",
  "Review tender",
  "Review refund",
  "Preview campaign",
  "Review receipt",
  "Review contract",
  "Preview listing",
  "Validate coverage",
  "Compare profiles",
]);
const TASK_FIELD_GROUPS = {
  "Resolve ticket": ["notes"],
  "Record acceptance": ["acceptance"],
  "Confirm fulfillment": ["delivery"],
  "Close period": ["reconciled", "notes"],
  "Prepare close": ["reconciled", "notes"],
  "Review refund": ["refund", "notes"],
  "Confirm refund": ["refund", "notes"],
  "Start work": ["notes"],
  "Complete work": ["notes"],
  "Return to service": ["notes"],
  "Release gate": ["result", "reading", "notes"],
};
function taskKey(r) {
  return [r.holding, r.module, r.id].join(":");
}
function taskRoute(r) {
  return route(`tasks/${r.module}/${encodeURIComponent(r.id)}`);
}
function taskBundle(r) {
  const steps = flowSteps(r),
    start = r.stage || 0;
  if (r.finished || !steps[start]) return [];
  if (steps[start].action === "Compare profiles") return [steps[start + 1]];
  if (r.module === "operations" && steps[start]?.action === "Escalate to lead")
    return [steps[start + 1]];
  return TASK_PREVIEWS.has(steps[start].action) && steps[start + 1]
    ? steps.slice(start, start + 2)
    : [steps[start]];
}
function taskAction(r) {
  return taskBundle(r).at(-1);
}
function taskAssignments(r, who) {
  return (personas[who]?.assignments || []).filter(
    (a) =>
      a.holding === r.holding &&
      roles[a.role].modules.includes(r.module) &&
      (a.branches.includes("all") || a.branches.includes(r.branch)),
  );
}
function taskRoleEligible(r, roleId, s = taskAction(r)) {
  if (!s || !roles[roleId]?.modules.includes(r.module)) return false;
  if (r.recordType === "period") return ["owner", "finance"].includes(roleId);
  if (
    /Approve|Publish|Activate|Renew assignment|Merge|Release gate|Release purchase|Close period|billing/i.test(
      s.action,
    )
  )
    return roles[roleId].type !== "staff";
  return true;
}
function taskEligible(r, who, s = taskAction(r)) {
  if (!s) return false;
  if (
    s.gate === "independent" &&
    [r.preparedBy, r.closePreparedBy].includes(who)
  )
    return false;
  if (/Approve (leave|requisition)/.test(s.action) && r.createdBy === who)
    return false;
  return taskAssignments(r, who).some((a) => taskRoleEligible(r, a.role, s));
}
function taskOwner(r) {
  const s = taskAction(r);
  if (!s) return null;
  if (r.taskUnassigned) return null;
  if (r.taskAssignedAction === s.action && taskEligible(r, r.taskAssignee))
    return r.taskAssignee;
  if (r.recordType === "purchase_order" && taskEligible(r, "nisa"))
    return "nisa";
  const homePerson =
    r.holding === "platform"
      ? "fluke"
      : r.holding === "northstar"
        ? "arun"
        : r.branch === "warehouse"
          ? "dao"
          : "mali";
  const candidates =
    s.gate === "independent"
      ? ["pim"]
      : /Approve (leave|requisition)/.test(s.action)
        ? ["nisa", homePerson]
        : [
              "finance",
              "brand",
              "commercial",
              "legal",
              "talent",
              "agent",
              "identity_access",
              "organization",
              "marketing",
            ].includes(r.module)
          ? ["nisa", "fluke"]
          : [r.createdBy, homePerson, "nisa", "fluke"];
  return (
    candidates.find((id) => taskEligible(r, id)) ||
    Object.keys(personas).find((id) => taskEligible(r, id)) ||
    null
  );
}
function taskCanAct(r) {
  return (
    isVisible(r) &&
    !recordReadOnly(r) &&
    taskOwner(r) === state.persona &&
    !taskWaitingOnWork(r) &&
    taskRoleEligible(r, state.role) &&
    !["expired", "denied"].includes(state.demoState)
  );
}
function taskReason(r) {
  const s = taskAction(r);
  if (!s) return "The outcome is recorded in history.";
  if (taskWaitingOnWork(r))
    return "Waiting for connected work to finish before return-to-service review.";
  if (r.externalWait)
    return "Waiting for the customer. Follow up when the response is due.";
  if (r.pendingHandoff)
    return "The action succeeded; preparation of the next draft needs a retry.";
  if (!taskOwner(r)) return "An eligible person needs to be assigned.";
  if (s.gate === "independent")
    return "An independent finance reviewer must check this company period.";
  if (/Approve/.test(s.action))
    return "Your decision is needed before the next person can continue.";
  if (r.taskPrepared)
    return "Prepared from connected work. Check the copied details before continuing.";
  return s.help;
}
function taskDue(r) {
  const date = new Date(r.due);
  return (
    (date < new Date() ? "Overdue · " : "Due ") +
    date.toLocaleDateString("en-US", { month: "short", day: "numeric" }) +
    ", " +
    date.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" })
  );
}
function taskItems(mode = "todo", module = null) {
  if (state.demoState === "empty") return [];
  return visibleRecords(module)
    .filter((r) => {
      if (!r.workflow || moduleReadOnly(r.module)) return false;
      const owner = taskOwner(r),
        participated =
          r.createdBy === state.persona ||
          r.taskEvents?.some((e) => e.person === state.persona);
      if (mode === "completed")
        return !!r.taskEvents?.some((e) => e.person === state.persona);
      if (r.optionalAction) return false;
      if (taskWaitingOnWork(r) && mode === "todo") return false;
      if (mode === "team")
        return (
          ["owner", "manager", "platform"].includes(role().type) &&
          (!!taskAction(r) || !!r.pendingHandoff)
        );
      if (mode === "waiting")
        return (
          participated &&
          ((owner && owner !== state.persona) ||
            r.externalWait ||
            r.pendingHandoff ||
            taskWaitingOnWork(r))
        );
      return (
        (taskAction(r) &&
          owner === state.persona &&
          (!r.externalWait || r.due <= Date.now())) ||
        (r.pendingHandoff && r.pendingHandoff.by === state.persona)
      );
    })
    .sort((a, b) =>
      mode === "completed"
        ? (b.taskEvents?.at(-1)?.at || 0) - (a.taskEvents?.at(-1)?.at || 0)
        : a.due - b.due,
    );
}
function seedTasks() {
  state.workTab = "tasks";
  state.taskFilter = "todo";
  state.taskQuery = "";
  state.taskReviews = {};
  state.taskOutcomes = {};
  state.taskDraftVersions = {};
  state.taskDraftBases = {};
  state.taskDetails = {};
  state.taskCommentDrafts = {};
  state.taskModule = null;
  state.taskScope = "mine";
  state.navigationViews = {};
  state.navigationReturns = {};
  state.recordModes = {};
  state.taskManualOpen = {};
  for (const r of records) {
    if (!r.workflow) continue;
    if (r.holding === "northstar") r.createdBy = "arun";
    if (r.holding === "platform") r.createdBy = "fluke";
    if (
      [
        "finance",
        "brand",
        "legal",
        "talent",
        "agent",
        "identity_access",
        "organization",
        "marketing",
      ].includes(r.module) &&
      r.holding !== "platform"
    )
      r.createdBy = "nisa";
    r.taskEvents = [];
    if (r.module === "people_payroll")
      r.fields.employee = personas[r.createdBy].name;
    if (r.module === "sales" && r.recordType === "quote") {
      r.fields.recipient = "buyer@riverstone.example";
      r.fields.acceptance = false;
      r.fields.delivery = false;
    }
  }
}
function taskFieldList(r) {
  const s = taskAction(r),
    values = wfData(r);
  let fields = recordFields(r);
  if (r.module === "sales" && r.recordType === "quote")
    fields = [
      ...fields,
      specField("recipient", "Send to", "text", ""),
      specField(
        "acceptance",
        "Customer acceptance is recorded",
        "checkbox",
        false,
      ),
    ];
  if (r.recordType === "sales_order")
    fields = [
      ...fields,
      specField(
        "delivery",
        "Delivery completion is verified",
        "checkbox",
        false,
      ),
    ];
  if (r.module === "sales" && r.recordType === "quote") {
    const order = [
      "customer",
      "recipient",
      "amount",
      "terms",
      "notes",
      "acceptance",
    ];
    fields.sort((a, b) => order.indexOf(a.key) - order.indexOf(b.key));
  }
  const keys = TASK_FIELD_GROUPS[s?.action];
  if (keys) fields = fields.filter((f) => keys.includes(f.key));
  else {
    fields = fields.filter(
      (f) => !["acceptance", "delivery", "refund"].includes(f.key),
    );
    if (r.module === "finance")
      fields = fields.filter((f) => f.key !== "reconciled");
  }
  if (r.module === "benefits" && values.coverage !== "Employee + dependent")
    fields = fields.filter((f) => !["dependent", "validated"].includes(f.key));
  return fields;
}
function taskIssues(r, values = wfData(r)) {
  const bundle = taskBundle(r),
    issues = [];
  if (taskAction(r) && taskPolicy(r) === "unsupported")
    issues.push("This action has no configured demo policy.");
  for (const s of bundle) issues.push(...prerequisites(r, s, values, true));
  if (r.module === "sales" && taskAction(r)?.action === "Send quote") {
    if (!String(values.customer || "").trim())
      issues.push("Choose a customer before sending the quote.");
    if (!(values.amount > 0))
      issues.push("Enter a quote total greater than zero.");
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(values.recipient || ""))
      issues.push("Enter the customer's email address in Send to.");
  }
  if (taskAction(r)?.action === "Record acceptance" && !values.acceptance)
    issues.push("Record the customer's acceptance before preparing the order.");
  if (taskAction(r)?.action === "Confirm fulfillment" && !values.delivery)
    issues.push("Verify delivery completion before confirming fulfillment.");
  if (
    r.module === "people_payroll" &&
    r.recordType === "leave_request" &&
    (!values.start || !(values.days > 0))
  )
    issues.push("Choose the first day and a positive number of leave days.");
  return [...new Set(issues)];
}
function taskReadForm(form, r) {
  const values = { ...wfData(r) };
  for (const el of form.elements)
    if (el.name && el.name !== "task-intent")
      values[el.name] =
        el.type === "checkbox"
          ? el.checked
          : el.type === "number"
            ? Number(el.value)
            : el.value.trim();
  return values;
}
function taskStoreDraft(form, r) {
  const key = wfDraftKey(r);
  state.taskDraftVersions[key] ??= r.version;
  state.taskDraftBases[key] ??= clone(r.fields);
  state.wfDrafts[key] = taskReadForm(form, r);
}
function taskDraftRefIssues(form) {
  return referencesVisible(formReferences(form))
    ? []
    : [
        "A linked reference is unavailable. Remove it or return to its permitted context.",
      ];
}
function taskNewDraft(source, module, type, fields, title, steps = null) {
  if (source.taskHandoffRef) {
    const previous = records.find(
      (x) =>
        x.holding === source.holding &&
        x.module === source.taskHandoffRef.module &&
        x.id === source.taskHandoffRef.id,
    );
    if (previous) return previous;
  }
  const child = childRecord(source, module, type, "Draft", title);
  child.taskPrepared = true;
  child.taskSteps = steps;
  child.fields = { ...child.fields, ...fields };
  child.stage = 0;
  child.finished = false;
  child.status =
    steps?.[0]?.status || MODULES[module].steps[0]?.status || "Draft";
  child.taskSource = { ref: refOf(source), version: source.version };
  child.taskEvents = [];
  child.createdBy = module === "finance" ? "nisa" : source.createdBy;
  source.taskHandoffRef = refOf(child, "prepared draft");
  source.handoff = source.taskHandoffRef;
  return child;
}
function taskHandoffSpec(r, action) {
  if (r.module === "assets" && action === "Begin maintenance")
    return "maintenance";
  if (r.module === "sales" && action === "Record acceptance")
    return "sales_order";
  if (r.module === "procurement" && action === "Approve requisition")
    return "purchase_order";
  if (r.recordType === "purchase_order" && action === "Release purchase order")
    return "goods_receipt";
  return null;
}
function prepareTaskHandoff(r, descriptor) {
  if (r.taskHandoffRef)
    return records.find(
      (x) =>
        x.holding === r.holding &&
        x.module === r.taskHandoffRef.module &&
        x.id === r.taskHandoffRef.id,
    );
  const f = r.fields,
    kind = descriptor.kind;
  let child;
  if (kind === "sales_order")
    child = taskNewDraft(
      r,
      "sales",
      "sales_order",
      { ...f, delivery: false },
      r.title + " · order draft",
      [
        step(
          "Draft",
          "Confirm order",
          "Confirmed",
          "Check the copied customer, items and amount before confirming the order.",
        ),
        ...ORDER_STEPS,
      ],
    );
  else if (kind === "purchase_order")
    child = taskNewDraft(
      r,
      "procurement",
      "purchase_order",
      { ...f },
      r.title + " · purchase-order draft",
      [
        step(
          "Draft",
          "Release purchase order",
          "Ordered",
          "Review the supplier, quantities and total before releasing the sample order.",
        ),
      ],
    );
  else if (kind === "goods_receipt")
    child = taskNewDraft(
      r,
      "procurement",
      "goods_receipt",
      {
        sku: f.sku || "TOTE-CANVAS-01",
        expected: f.quantity,
        quantity: 0,
        notes: "Enter the actual quantity counted when the delivery arrives.",
      },
      r.title + " · expected delivery",
      RECEIPT_STEPS,
    );
  else if (kind === "people_payroll")
    child = taskNewDraft(
      r,
      "people_payroll",
      "employee_onboarding",
      { employee: f.candidate || "New colleague", notes: f.notes },
      "New colleague · onboarding draft",
      [
        step(
          "Draft",
          "Confirm employee handoff",
          "Active",
          "Review the approved offer and employee details before confirming the handoff.",
          "notes",
        ),
      ],
    );
  else {
    const m = kind,
      fields = { notes: f.notes || "Prepared from connected work." };
    if (m === "sales")
      Object.assign(fields, {
        customer: f.account || f.customer || "Riverstone Market",
        amount: f.value || f.amount || 0,
        recipient: "buyer@riverstone.example",
        acceptance: false,
      });
    if (m === "inventory")
      Object.assign(fields, {
        sku: f.sku || "TOTE-CANVAS-01",
        expected: f.quantity,
        quantity: f.quantity,
      });
    if (m === "finance")
      Object.assign(fields, {
        debit: f.amount || 0,
        credit: f.amount || 0,
        reconciled: false,
      });
    if (m === "procurement")
      Object.assign(fields, {
        quantity:
          f.shortage || Math.max(1, (f.demand || 100) - (f.available || 0)),
      });
    child = taskNewDraft(
      r,
      m,
      MODULES[m].kind.toLowerCase().replaceAll(" ", "_"),
      fields,
      r.title + " · " + MODULES[m].kind.toLowerCase() + " draft",
    );
  }
  if (kind === "maintenance") {
    r.taskWaitFor = refOf(child);
    child.fields.asset = r.title;
    child.fields.notes = r.fields.notes;
  }
  r.pendingHandoff = null;
  r.taskHandoffComplete = true;
  return child;
}
function completeTask(r, bundle) {
  const action = bundle.at(-1),
    before = r.status,
    versionBefore = r.version;
  let target = r;
  for (const s of bundle) target = commitWorkflow(r, s, false);
  r.externalWait = r.module === "sales" && action.action === "Send quote";
  r.optionalAction =
    (r.module === "pos" && action.action === "Complete sale") ||
    (r.module === "legal" && action.action === "Activate contract");
  if (r.externalWait) r.due = Date.now() + 2 * 86400000;
  const kind =
    taskHandoffSpec(r, action.action) ||
    ([
      "finance",
      "inventory",
      "sales",
      "procurement",
      "people_payroll",
    ].includes(action.handoff)
      ? action.handoff
      : null);
  r.taskEvents ||= [];
  r.taskEvents.push({
    person: state.persona,
    action: action.action,
    at: Date.now(),
    before,
    after: r.status,
    versionBefore,
  });
  if (r.holding !== "platform")
    publishRecordUpdate(r, action.action + ". " + r.status + ".");
  if (kind) {
    if (kind !== "maintenance") r.finished = true;
    r.pendingHandoff = { kind, by: state.persona, sourceVersion: r.version };
    if (state.demoState !== "handoff-failure")
      target = prepareTaskHandoff(r, r.pendingHandoff);
  }
  if (target !== r && !target.taskEvents) target.taskEvents = [];
  if (target !== r && !r.taskHandoffRef) {
    r.taskHandoffRef = refOf(target);
    r.handoff = refOf(target);
  }
  if (
    r.module === "maintenance" &&
    r.finished &&
    r.taskSource?.ref.module === "assets"
  ) {
    const asset = records.find(
      (x) =>
        x.holding === r.holding &&
        x.module === "assets" &&
        x.id === r.taskSource.ref.id,
    );
    if (asset) {
      asset.evidence = [
        ...asset.evidence,
        ...r.evidence.map((e) => ({ ...e, source: refOf(r) })),
      ];
      wfHistory(
        asset,
        "Linked maintenance completed. Return to service still needs review.",
      );
    }
  }
  return target;
}
function taskWaitingOnWork(r) {
  if (!r.taskWaitFor) return false;
  const target = records.find(
    (x) =>
      x.holding === r.holding &&
      x.module === r.taskWaitFor.module &&
      x.id === r.taskWaitFor.id,
  );
  return !target?.finished;
}
window.ERPX_TASK_MODEL = {
  taskKey,
  taskRoute,
  taskCanAct,
  taskReason,
  taskDue,
  taskItems,
  seedTasks,
  taskFieldList,
  taskIssues,
  taskStoreDraft,
  taskDraftRefIssues,
  completeTask,
};

const TASK_ROUTINE_ACTIONS = new Set([
  "Start work",
  "Acknowledge exception",
  "Qualify opportunity",
  "Record next step",
  "Schedule order",
  "Schedule visit",
  "Run scenario",
  "Record attendance",
  "Record interview",
  "Start assignment",
  "Link dependency",
  "Submit leave request",
  "Submit requisition",
  "Triage ticket",
  "Record failed inspection",
  "Record inspection",
  "Record recheck",
  "Prepare close",
]);
const TASK_DECISION_ACTIONS = new Set(["Approve leave", "Approve requisition"]);
const TASK_REVIEW_ACTIONS = new Set([
  "Accept as draft",
  "Accept goods receipt",
  "Activate branch",
  "Activate contract",
  "Approve dependency",
  "Approve offer",
  "Approve version",
  "Archive contract",
  "Begin maintenance",
  "Close exception",
  "Close period",
  "Complete course",
  "Complete milestone",
  "Complete order",
  "Complete sale",
  "Complete visit",
  "Complete work",
  "Confirm enrollment",
  "Confirm fulfillment",
  "Confirm merge",
  "Confirm order",
  "Confirm refund",
  "Confirm employee handoff",
  "Create purchase order",
  "Create sales order",
  "Create sample order",
  "Dispatch order",
  "Escalate to lead",
  "Hand off to Sales",
  "Inspect sample checkout",
  "Mark answered",
  "Post journal",
  "Publish catalog",
  "Publish discussion",
  "Publish listing",
  "Publish version",
  "Receive stock",
  "Record acceptance",
  "Release billing",
  "Release gate",
  "Release order",
  "Release purchase order",
  "Release recommendation",
  "Renew assignment",
  "Report output",
  "Reserve materials",
  "Resolve exception",
  "Resolve ticket",
  "Retry provisioning",
  "Return to service",
  "Review application",
  "Review recommendation",
  "Schedule campaign",
  "Send quote",
  "Verify readiness",
]);
const TASK_ACTION_POLICIES = Object.freeze(
  Object.fromEntries([
    ...[...TASK_ROUTINE_ACTIONS].map((a) => [a, "direct"]),
    ...[...TASK_DECISION_ACTIONS].map((a) => [a, "decision"]),
    ...[...TASK_REVIEW_ACTIONS].map((a) => [a, "review"]),
  ]),
);
function taskPolicy(r) {
  return TASK_ACTION_POLICIES[taskAction(r)?.action] || "unsupported";
}
function taskNeedsReview(r) {
  return taskPolicy(r) !== "direct";
}
function taskSubmitLabel(r) {
  const action = taskAction(r)?.action || "Continue";
  if (!taskNeedsReview(r)) return action;
  const labels = {
    "Send quote": "Review quote",
    "Submit leave request": "Review request",
    "Approve leave": "Review request",
    "Approve requisition": "Review request",
    "Receive stock": "Review receipt",
    "Accept goods receipt": "Review receipt",
    "Post journal": "Review journal",
    "Confirm refund": "Review refund",
  };
  return (
    labels[action] ||
    "Review and " + action.charAt(0).toLowerCase() + action.slice(1)
  );
}
window.ERPX_TASK_MODEL.taskNeedsReview = taskNeedsReview;
window.ERPX_TASK_MODEL.taskSubmitLabel = taskSubmitLabel;

window.ERPX_TASK_MODEL.taskWaitingOnWork = taskWaitingOnWork;
