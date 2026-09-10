/* Focused task screens share the owning records, drafts and discussions. */
function taskPerson(id) {
  return personas[id]?.name || "Not assigned";
}
function taskCard(r, mode = "todo") {
  const owner = taskOwner(r),
    last = r.taskEvents
      ?.filter((e) => state.taskScope === "team" || e.person === state.persona)
      .at(-1);
  const title =
    mode === "completed"
      ? last?.action || "Completed work"
      : r.pendingHandoff
        ? "Prepare next draft"
        : r.externalWait
          ? "Follow up with customer"
          : taskAction(r)?.action || "View outcome";
  return `<a class="task-card" href="${taskRoute(r)}" data-task-key="${esc(taskKey(r))}"><span class="task-card-main"><strong>${esc(title)}</strong><span>${esc(r.title)}</span><small>${esc(taskReason(r))}</small><span class="task-card-meta">${mode === "completed" ? "Completed by " + esc(taskPerson(last?.person)) : esc(taskDue(r))}${owner && owner !== state.persona ? ` · ${esc(taskPerson(owner))}` : ""}</span></span>${icon("chevron")}</a>`;
}
function taskShortcuts() {
  const preferred =
    state.holding === "platform"
      ? ["platform_administration", "commercial"]
      : state.role === "finance"
        ? ["finance", "procurement", "people_payroll"]
        : ["people_payroll", "sales", "procurement", "support", "inventory"];
  return `<div class="task-shortcuts" aria-label="Everyday shortcuts">${preferred
    .filter((k) => moduleAllowed(k) && !moduleReadOnly(k))
    .slice(0, 4)
    .map(
      (k) =>
        `<button class="shortcut-pill" data-action="task-new" data-module="${k}">${icon(moduleIcons[k])}${esc(MODULES[k].create)}</button>`,
    )
    .join("")}</div>`;
}
function taskHomePanel() {
  const todos = taskItems();
  return `<section class="card task-home"><div class="between"><h2>My next tasks <span class="badge violet">${todos.length}</span></h2><a class="link" href="${route("work")}">All my work ${icon("arrow")}</a></div><div class="task-home-list">${
    todos
      .slice(0, 3)
      .map((r) => taskCard(r))
      .join("") ||
    '<p class="task-caught-up">You’re caught up. New tasks will appear here.</p>'
  }</div>${taskShortcuts()}</section>`;
}
function taskWorkView() {
  const team = ["owner", "manager", "platform"].includes(role().type),
    mode = state.taskFilter || "todo";
  const query = (state.taskQuery || "").toLowerCase();
  const queue = (kind) =>
    state.taskScope === "team" && team
      ? visibleRecords(state.taskModule)
          .filter(
            (r) =>
              r.workflow &&
              !moduleReadOnly(r.module) &&
              (kind === "completed"
                ? r.taskEvents?.length
                : kind === "waiting"
                  ? !!r.externalWait ||
                    taskWaitingOnWork(r) ||
                    !!r.pendingHandoff
                  : !!taskAction(r) &&
                    !r.optionalAction &&
                    !r.externalWait &&
                    !taskWaitingOnWork(r)),
          )
          .sort((a, b) => a.due - b.due)
      : taskItems(kind, state.taskModule);
  const items = queue(mode).filter((r) =>
    [r.title, taskAction(r)?.action, taskPerson(taskOwner(r)), r.id]
      .join(" ")
      .toLowerCase()
      .includes(query),
  );
  return `${heading("Your work", "", `<button class="btn primary" data-action="task-new-menu">${icon("plus")}New work</button>`)}<div class="work-links"><a class="btn" href="${route("modules")}">${icon("grid")}Modules</a><a class="link" href="${route("journeys")}">Connected journeys ${icon("arrow")}</a></div><section class="work-queue-surface"><div class="queue-toolbar"><div class="task-tabs" aria-label="Task queues">${[
    ["todo", "To do"],
    ["waiting", "Waiting"],
    ["completed", "Completed"],
  ]
    .map(
      ([id, label]) =>
        `<button data-action="task-filter" data-id="${id}" class="${mode === id ? "active" : ""}" aria-pressed="${mode === id}">${label}<span>${queue(id).length}</span></button>`,
    )
    .join(
      "",
    )}</div>${team ? `<label class="queue-scope"><span class="sr-only">Whose work</span><select id="task-scope"><option value="mine">My work</option><option value="team" ${state.taskScope === "team" ? "selected" : ""}>Team work</option></select></label>` : ""}</div><div class="queue-search-row"><form class="task-search" data-form="task-search"><label class="sr-only" for="task-search">Search your work</label><input id="task-search" name="query" value="${esc(state.taskQuery || "")}" placeholder="Find a task, record or person…"></form><button class="btn" data-action="task-filters">${icon("filter")}Filters${state.taskModule ? " · 1" : ""}</button></div>${state.taskModule ? `<div class="task-module-filter">${esc(SEED.CTX[state.taskModule].n)} <button class="link" data-action="task-clear-module">Clear filter ×</button></div>` : ""}<div class="task-queue">${items.map((r) => taskCard(r, mode)).join("") || empty(query ? "No matching tasks" : mode === "waiting" ? "Nothing waiting" : "You’re caught up", query ? "Try another name or clear the search." : "New work appears when it is ready for the responsible person.")}</div></section>`;
}
function taskFieldView(f, r, disabled = false) {
  const value = wfData(r)[f.key];
  const readOnly =
    (f.key === "employee" && r.recordType === "leave_request") ||
    ["expected", "window", "hold", "locked"].includes(f.key);
  return `<div class="task-field" data-field="${esc(f.key)}">${fieldControl(f, value, disabled || readOnly)}${["quantity", "expected"].includes(f.key) && ["inventory", "procurement"].includes(r.module) ? `<p class="field-hint">${f.key === "expected" ? "Expected from the supplier order." : "Record the actual quantity counted. Expected stock is not proof of receipt."}</p>` : ""}<p class="field-error" id="task-error-${esc(f.key)}" aria-live="polite"></p></div>`;
}
function taskFacts(r, values = wfData(r), summary = true) {
  const keys = summary ? taskSummaryKeys(r) : null;
  const combined = [...recordFields(r), ...taskFieldList(r)];
  const fields = combined.filter(
    (f, i) =>
      (!keys || keys.includes(f.key)) &&
      combined.findIndex((x) => x.key === f.key) === i &&
      (!summary ||
        f.key !== "refund" ||
        /refund/i.test(taskAction(r)?.action || "")),
  );
  return `<dl class="task-facts">${fields
    .map(
      (f) =>
        `<div><dt>${esc(f.label)}</dt><dd>${esc(typeof values[f.key] === "boolean" ? (values[f.key] ? "Yes" : "No") : (values[f.key] ?? "Not supplied"))}</dd></div>`,
    )
    .join("")}</dl>`;
}
function taskEvidence(r) {
  const required = taskBundle(r).some((s) => s.gate === "evidence");
  if (!required && !r.evidence.length) return "";
  const pending = state.attachmentDrafts?.[attachmentKey(r, "evidence")]?.length || 0;
  return `<section class="task-evidence"><div><h3>Completion evidence${required ? ' <span class="badge amber">Required</span>' : ""}</h3><p class="small muted">Attach evidence before confirming this work. Saved evidence remains attributed to this record.</p></div>${attachmentList(r.evidence)}${taskCanAct(r) ? `${attachmentComposer(r, "evidence")}<div class="flex wrap"><button type="button" class="btn" data-action="task-evidence" data-module="${r.module}" data-record="${esc(r.id)}">${icon("image")}Use demo evidence</button>${pending ? `<button type="button" class="btn primary" data-action="attachment-save-evidence" data-module="${r.module}" data-record="${esc(r.id)}">${icon("check")}Save ${pending} attachment${pending === 1 ? "" : "s"}</button>` : ""}</div>` : ""}</section>`;
}
function taskSummaryKeys(r) {
  const fields = {
    "Send quote": ["customer", "recipient", "amount", "terms", "notes"],
    "Approve leave": ["employee", "start", "days", "type", "notes"],
    "Submit leave request": ["employee", "start", "days", "type", "notes"],
    "Approve requisition": ["supplier", "item", "quantity", "amount", "notes"],
    "Post journal": ["debit", "credit", "currency", "fx", "notes"],
    "Prepare close": ["reconciled", "notes"],
    "Close period": ["reconciled", "notes"],
    "Record acceptance": ["customer", "amount", "acceptance"],
    "Confirm fulfillment": ["customer", "amount", "delivery"],
    "Confirm refund": ["refund", "notes"],
    "Receive stock": ["sku", "expected", "quantity", "location", "notes"],
    "Accept goods receipt": [
      "supplier",
      "sku",
      "expected",
      "quantity",
      "notes",
    ],
    "Release gate": ["result", "reading", "notes"],
  };
  return fields[taskAction(r)?.action] || null;
}
function decisionSnapshot(r) {
  const key = wfDraftKey(r);
  if (state.taskReviews[key]) return state.taskReviews[key];
  return (state.taskReviews[key] = {
    values: clone(r.fields),
    version: r.version,
    stage: r.stage,
    action: taskAction(r).action,
    context: ctxKey(),
    dirty: false,
    refs: clone((r.refs || []).filter(resolveRef)),
    sources:
      r.taskSource && resolveRef(r.taskSource.ref)
        ? [
            {
              ref: r.taskSource.ref,
              version: resolveRef(r.taskSource.ref).version,
            },
          ]
        : [],
    fieldRefs: {},
    bundle: clone(taskBundle(r)),
  });
}
function taskReviewView(r, review) {
  const decision = taskPolicy(r) === "decision",
    issues = taskIssues(r, review.values);
  return `<section class="card task-review"><h2>${decision ? "Review request" : "Review before confirming"}</h2>${taskFacts(r, review.values)}${/refund/i.test(review.action) ? `<p class="notice">Original settlement: ${money(r.settled || 0)} · Remaining refundable: ${money((r.settled || 0) - (r.refunded || 0))}</p>` : ""}${r.evidence.length ? `<div class="review-evidence"><h3>Evidence</h3>${r.evidence.map((e) => `<a class="reference" href="${e.asset}" target="_blank">${esc(e.name)}</a>`).join("")}</div>` : ""}${review.refs.length ? `<div class="task-review-references">${review.refs.map(referenceChip).join("")}</div>` : ""}<div class="task-consequence"><strong>What happens next</strong><p>${esc(taskConsequence(r, review.action))}</p></div><p class="small muted">${esc(person().name)} · ${esc(role().name)} will confirm this action.</p><p id="task-status" class="form-status" role="status">${issues.length ? esc(issues.join(" ")) : "Required checks are ready. Nothing has been submitted."}</p><div class="task-action-bar">${decision ? `<button class="btn" data-action="task-help" data-module="${r.module}" data-record="${esc(r.id)}">Ask for correction</button>` : `<button class="btn" data-action="task-edit" data-module="${r.module}" data-record="${esc(r.id)}">Back to edit</button>`}<button class="btn primary" data-action="task-confirm" data-module="${r.module}" data-record="${esc(r.id)}">${esc(review.action)}</button></div></section>`;
}
function taskConsequence(r, action) {
  if (action === "Send quote")
    return "The sample quote becomes Sent. You will wait for the customer's response. Nothing is emailed.";
  if (action === "Record acceptance")
    return "Acceptance is recorded and a linked order draft is prepared. The order still needs confirmation.";
  if (action === "Approve requisition")
    return "The request becomes Approved. A purchase-order draft is prepared for review before release.";
  if (action === "Release purchase order")
    return "The sample order becomes Ordered. A receiving task is prepared; actual quantities still need to be counted.";
  if (["Submit leave request", "Submit requisition"].includes(action)) {
    const reviewer = taskOwner({
      ...r,
      stage: (r.stage || 0) + 1,
      finished: false,
    });
    return `The request goes to ${taskPerson(reviewer)} for approval. Follow its progress in Waiting.`;
  }
  if (action === "Close period")
    return "This company period closes. Further posting into the period is blocked.";
  if (action === "Post journal")
    return "This balanced sample journal is posted to its company period. The posted journal becomes read-only.";
  if (action === "Confirm refund")
    return "A bounded sample refund is recorded against the original receipt. No payment is moved.";
  return `This record will be marked ${taskAction(r)?.next || "complete"}. Your change is recorded in its history.`;
}
function taskOutcomeView(r, outcome, compact = false) {
  const next = taskAction(r),
    owner = taskOwner(r),
    handoff = r.taskHandoffRef && resolveRef(r.taskHandoffRef);
  return `<section class="task-outcome ${compact ? "compact-outcome" : "card"}" role="status">${icon("check")}<div><strong>${esc(outcome.action)} completed</strong><p>${esc(r.status)}${r.externalWait ? " · Waiting for the customer" : next && owner !== state.persona ? " · Next: " + esc(taskPerson(owner)) : ""}</p></div></section>${r.pendingHandoff ? `<section class="notice amber task-handoff-error"><div><strong>Your action succeeded. Draft preparation needs a retry.</strong><p>The decision is already recorded.</p><button class="btn" data-action="task-retry-handoff" data-module="${r.module}" data-record="${esc(r.id)}">Retry draft preparation</button></div></section>` : ""}${handoff ? `<a class="prepared-draft-card" href="${taskRoute(handoff)}"><span><strong>Open prepared ${esc(handoff.recordType.replaceAll("_", " "))}</strong><small>${esc(handoff.title)} · ${esc(taskPerson(taskOwner(handoff)))}</small></span>${icon("arrow")}</a>` : ""}${!compact ? `<div class="task-outcome-actions">${next && owner === state.persona && !taskWaitingOnWork(r) && (r.externalWait || r.optionalAction) ? `<button class="btn" data-action="task-continue" data-module="${r.module}" data-record="${esc(r.id)}">${r.externalWait ? "Record customer response" : r.module === "pos" ? "Start a refund" : esc(next.action)}</button>` : ""}<button class="btn" data-action="task-back">Back to work</button></div>` : ""}`;
}
function taskSupportingDetails(r, open = false) {
  return `<details class="task-disclosure" data-task-detail="details" ${open ? "open" : ""}><summary>${icon("file")}Details <span>Progress, connected work & history</span></summary><div class="task-disclosure-body">${followButton("record", r.module + "/" + r.id)}${taskFacts(r, r.fields, false)}<h3>Progress</h3><p>${esc(r.status)}${taskAction(r) ? " · Next: " + esc(taskAction(r).action) : " · Complete"}</p>${referencePanel(r)}${r.evidence.length ? `<h3>Evidence</h3>${r.evidence.map((e) => `<a class="reference" href="${e.asset}" target="_blank">${esc(e.name)}</a>`).join("")}` : ""}<h3>History</h3>${r.history.map((h) => `<div class="activity-item"><strong>${esc(h.by)}</strong><p>${esc(h.text)}</p><small>${esc(h.time)}</small></div>`).join("")}</div></details>`;
}
function taskContextSummary(r) {
  const steps = taskBundle(r), current = r.stage || 0;
  const related = (r.refs || []).map(resolveRef).filter(Boolean).slice(0, 3);
  return `<aside class="task-context-summary"><section class="card card-pad"><div class="between"><h2>Workflow</h2>${badge(r.status)}</div><div class="mini-workflow">${steps.map((step, i) => `<div class="${i < current ? "complete" : i === current ? "current" : ""}"><span>${i < current ? icon("check") : i + 1}</span><p><strong>${esc(step.action)}</strong><small>${i < current ? "Completed" : i === current ? "Current step" : "Next"}</small></p></div>`).join("")}</div></section><section class="card card-pad"><h2>Responsibility</h2><div class="summary-facts vertical"><div><small>Responsible person</small><strong>${esc(taskPerson(taskOwner(r)))}</strong></div><div><small>Due</small><strong>${esc(taskDue(r))}</strong></div><div><small>Workspace</small><strong>${esc(scopeLabel())}</strong></div></div></section>${related.length ? `<section class="card card-pad"><h2>Related records</h2>${related.map(recordLinkCard).join("")}</section>` : ""}</aside>`;
}
function taskFormView(r) {
  const fields = taskFieldList(r),
    action = taskAction(r),
    key = wfDraftKey(r);
  const needsNote =
    taskBundle(r).some((s) => s.gate === "notes") ||
    ["brand", "marketing", "collaboration"].includes(r.module) ||
    [
      "Complete work",
      "Return to service",
      "Resolve ticket",
      "Resolve exception",
    ].includes(action.action);
  const optional = fields.filter((f) => f.type === "textarea" && !needsNote),
    required = fields.filter((f) => !optional.includes(f));
  return `<section class="card task-form-card"><h2>${r.taskPrepared ? "Check the prepared draft" : r.module === "customer_authority" ? "Compare profiles" : "Complete the details"}</h2><p class="task-instruction">${esc(action.help)}</p>${r.taskSource ? `<p class="draft-source">Prepared from ${resolveRef(r.taskSource.ref) ? `<a href="${recordRoute(resolveRef(r.taskSource.ref))}">${esc(resolveRef(r.taskSource.ref).title)}</a>` : "an unavailable source"}.</p>` : ""}${["pos", "finance", "quality", "planning", "learning", "brand", "marketing", "ecommerce", "customer_authority"].includes(r.module) ? `<div class="task-business-preview">${contextPanel({ ...r, fields: wfData(r) })}</div>` : ""}<form data-form="task-draft" data-module="${r.module}" data-record="${esc(r.id)}"><div class="field-grid">${required.map((f) => taskFieldView(f, r, r.module === "customer_authority")).join("")}</div>${optional.length ? `<details class="additional-details" data-task-detail="optional"><summary>Additional details</summary><div class="field-grid">${optional.map((f) => taskFieldView(f, r, r.module === "customer_authority")).join("")}</div></details>` : ""}${taskEvidence(r)}<div class="task-consequence"><strong>What happens next</strong><p>${esc(taskConsequence(r, action.action))}</p></div><div id="task-issues" class="task-issues" aria-live="polite"></div><p id="task-status" class="form-status" role="status">${state.wfDrafts[key] ? "Draft kept in this session · not submitted" : "Nothing submitted yet."}</p><div class="task-action-bar">${r.module === "customer_authority" ? "" : '<button class="btn quiet" type="submit" name="task-intent" value="save">Save draft</button>'}<button class="btn primary" type="submit" name="task-intent" value="review">${esc(taskSubmitLabel(r))}${icon("arrow")}</button></div></form></section>`;
}
function taskView(k, id, viewMode = "action") {
  const r = findRecord(k, id);
  if (!r || !r.workflow)
    return r
      ? legacyRecordView(k, id)
      : empty(
          "Task unavailable",
          "This task is outside the current work context.",
          `<a class="btn" href="${route("work")}">Back to My Work</a>`,
          "lock",
        );
  const key = wfDraftKey(r),
    owner = taskOwner(r),
    action = taskAction(r),
    outcome = state.taskOutcomes[key];
  const mode = moduleReadOnly(k)
      ? "details"
      : state.recordModes[navigationKey()] || viewMode,
    canAct = taskCanAct(r) && !moduleReadOnly(k);
  const resume = state.taskManualOpen?.[key];
  const ready =
    canAct &&
    action &&
    !taskWaitingOnWork(r) &&
    (!(r.externalWait || r.optionalAction) || resume);
  let main = "";
  if (mode === "details")
    main = `<section class="card task-record-summary">${["customer_360", "commercial", "brand", "marketing", "ecommerce", "planning", "learning"].includes(k) ? contextPanel(r, true) : ""}${action && !moduleReadOnly(k) ? `<button class="btn primary" data-action="task-action-mode" data-module="${k}" data-record="${esc(id)}">${esc(canAct ? action.action : "View next action")}</button>` : ""}</section>`;
  else if (ready) {
    if (outcome) main += taskOutcomeView(r, outcome, true);
    main +=
      taskPolicy(r) === "decision"
        ? taskReviewView(r, decisionSnapshot(r))
        : state.taskReviews[key]
          ? taskReviewView(r, state.taskReviews[key])
          : taskFormView(r);
  } else if (outcome) main = taskOutcomeView(r, outcome);
  else {
    const assignment = taskAssignments(r, state.persona).find((a) =>
      taskRoleEligible(r, a.role),
    );
    main = `<section class="card task-waiting"><h2>${!action ? "This work is complete" : r.externalWait ? "Waiting for the customer" : taskWaitingOnWork(r) ? "Waiting for maintenance completion" : owner === state.persona ? "Use your assigned work role" : owner ? "Waiting for " + esc(taskPerson(owner)) : "This task needs an owner"}</h2><p>${esc(taskReason(r))}</p>${taskFacts(r, r.fields)}${owner === state.persona && assignment && !taskWaitingOnWork(r) && !canAct ? `<button class="btn primary" data-action="task-switch-role" data-id="${assignment.role}" data-module="${k}" data-record="${esc(id)}">Switch to ${esc(roles[assignment.role].name)} and return</button>` : ""}${r.externalWait && canAct ? `<button class="btn" data-action="task-continue" data-module="${k}" data-record="${esc(id)}">Record customer response</button>` : ""}${!owner && action && ["owner", "manager", "platform"].includes(role().type) ? `<button class="btn" data-action="task-assign" data-module="${k}" data-record="${esc(id)}">Assign responsible person</button>` : ""}</section>`;
  }
  return `<div class="focused-task-layout"><header class="focused-task-header"><div class="between"><button class="link" data-action="task-back">${icon("back")}Back</button>${!moduleReadOnly(k) ? `<details class="task-tools"><summary>Task tools ${icon("dots")}</summary><div>${canAct && taskPolicy(r) !== "decision" ? `<button class="btn" data-action="task-assistant" data-module="${k}" data-record="${esc(id)}">Draft assistant</button>` : ""}${action && ["owner", "manager", "platform"].includes(role().type) ? `<button class="btn" data-action="task-assign" data-module="${k}" data-record="${esc(id)}">Change assignment</button>` : ""}<button class="btn" data-action="task-help" data-module="${k}" data-record="${esc(id)}">Ask for help</button></div></details>` : ""}</div><h1>${esc(mode === "details" ? r.title : action?.action || r.title)}</h1><p>${mode === "details" ? "" : esc(r.title) + " · "}${esc(r.id)}</p><div class="task-identity-line"><span class="task-stage">Stage ${badge(r.status)}</span><span>${action ? "Responsible: " + esc(taskPerson(owner)) : "Completed"}</span><span>${esc(taskDue(r))}</span>${r.branch !== state.branch ? `<span>${esc(branchName(r.branch))}</span>` : ""}</div></header><div class="task-main stack"><div class="task-content-grid"><div class="task-primary-column stack">${main}${taskSupportingDetails(r, mode === "details")}${moduleReadOnly(k) ? "" : `<details class="task-disclosure" data-task-detail="discussion"><summary>${icon("message")}Discussion <span>${publicComments(r).length} replies</span></summary><div class="task-disclosure-body">${discussionPanel(r)}</div></details>`}</div>${taskContextSummary(r)}</div></div></div>`;
}
function refreshTaskIssues(r, reveal = false) {
  const root = document.querySelector('[data-form="task-draft"]');
  if (!root) return;
  const issues = taskIssues(r).concat(taskDraftRefIssues(root));
  const box = $("task-issues");
  if (box)
    box.innerHTML = issues
      .map((text) => `<p>${icon("info")}<span>${esc(text)}</span></p>`)
      .join("");
  if (reveal && issues.length) {
    box.tabIndex = -1;
    box.focus();
  }
  const matchers = {
    quantity: /quantity|basket/i,
    tender: /tender/i,
    refund: /refund/i,
    notes: /explanation|note/i,
    debit: /debit|balance/i,
    recipient: /email|Send to/i,
    days: /days/i,
    reading: /°C|recheck/i,
    dependent: /dependent/i,
    acceptance: /acceptance/i,
    delivery: /delivery/i,
  };
  root.querySelectorAll(".task-field").forEach((el) => {
    const regex = matchers[el.dataset.field],
      message = regex ? issues.find((x) => regex.test(x)) : null;
    el.querySelector(".field-error").textContent = message || "";
    el.querySelector("input,select,textarea")?.setAttribute(
      "aria-invalid",
      String(!!message),
    );
  });
  return issues;
}

window.ERPX_TASK_VIEWS = {
  taskHomePanel,
  taskWorkView,
  taskView,
  refreshTaskIssues,
};
