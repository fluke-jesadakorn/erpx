/* Explicit task actions, draft recovery and shared-surface integration. */
function taskCurrentForm(r) {
  return document.querySelector(
    `[data-form="task-draft"][data-record="${CSS.escape(r.id)}"]`,
  );
}
function taskNavigate(r) {
  closeDialog();
  const target = taskRoute(r);
  rememberNavigation(target);
  state.recordModes[navigationKey(target)] = "action";
  go(`tasks/${r.module}/${encodeURIComponent(r.id)}`);
}
function taskRefresh() {
  render();
  const result = document.querySelector(".task-review,.task-outcome,.task-waiting");
  result?.setAttribute("tabindex", "-1");
  result?.focus({ preventScroll: true });
  result?.scrollIntoView({ block: "start" });
}
function taskStartReview(r, form) {
  taskStoreDraft(form, r);
  const issues = refreshTaskIssues(r, true);
  if (!taskCanAct(r) || issues?.length) return;
  const key = wfDraftKey(r);
  if (state.taskDraftVersions[key] !== r.version) {
    showTaskConflict(r);
    return;
  }
  state.taskReviews[key] = {
    values: clone(wfData(r)),
    version: r.version,
    stage: r.stage,
    action: taskAction(r).action,
    context: ctxKey(),
    dirty: JSON.stringify(wfData(r)) !== JSON.stringify(r.fields),
    refs: clone(formReferences(form)),
    sources:
      r.taskSource && resolveRef(r.taskSource.ref)
        ? [
            {
              ref: r.taskSource.ref,
              version: resolveRef(r.taskSource.ref).version,
            },
          ]
        : [],
    fieldRefs: Object.fromEntries(
      [...form.querySelectorAll("[data-enhanced]")].map((el) => [
        el.name,
        clone(editorRefsFor(el.id)),
      ]),
    ),
    bundle: clone(taskBundle(r)),
  };
  if (taskNeedsReview(r)) taskRefresh();
  else confirmTaskReview(r, form.querySelector('button[value="review"]'));
}
function showTaskConflict(r) {
  const values = wfData(r);
  openDialog(
    "The record changed",
    `<p>Your draft is still here. Compare the current values before reviewing again.</p><div class="task-conflict-values">${
      recordFields(r)
        .filter((f) => values[f.key] !== r.fields[f.key])
        .map(
          (f) =>
            `<div><strong>${esc(f.label)}</strong><p>Current: ${esc(r.fields[f.key])}</p><p>Your draft: ${esc(values[f.key])}</p></div>`,
        )
        .join("") ||
      "<p>The record history or evidence changed. Your field values are preserved.</p>"
    }</div>`,
    `<button class="btn" data-action="close-dialog">Keep editing</button><button class="btn primary" data-action="task-rebase" data-module="${r.module}" data-record="${esc(r.id)}">Keep my edits and review again</button>`,
  );
}
document.addEventListener("input", (event) => {
  const comment = event.target.closest('[data-form="wf-comment"]');
  if (comment && ["tasks", "rec"].includes(parseRoute().page)) {
    const host = findRecord(comment.dataset.module, comment.dataset.record);
    if (host)
      state.taskCommentDrafts[wfDraftKey(host)] = {
        text: comment.elements.text.value,
        visibility: comment.elements.visibility?.value || "public",
      };
  }

  const form = event.target.closest('[data-form="task-draft"]');
  if (!form) return;
  const r = findRecord(form.dataset.module, form.dataset.record);
  if (!r) return;
  taskStoreDraft(form, r);
  $("task-status").textContent = "Draft kept in this session · not submitted";
  refreshTaskIssues(r);
});
document.addEventListener("change", (event) => {
  const form = event.target.closest('[data-form="task-draft"]');
  if (form && event.target.name === "coverage") {
    const r = findRecord(form.dataset.module, form.dataset.record);
    taskStoreDraft(form, r);
    render();
    document.querySelector('[name="coverage"]')?.focus();
  }
});
document.addEventListener("submit", (event) => {
  const form = event.target;
  if (form.dataset.form === "task-search") {
    event.preventDefault();
    state.taskQuery = form.elements.query.value;
    render();
    return;
  }
  if (form.dataset.form === "task-assignment") {
    event.preventDefault();
    const r = findRecord(form.dataset.module, form.dataset.record),
      who = form.elements.person.value;
    if (
      !r ||
      !["owner", "manager", "platform"].includes(role().type) ||
      !taskEligible(r, who)
    )
      return;
    r.taskAssignee = who;
    r.taskAssignedAction = taskAction(r).action;
    r.taskUnassigned = false;
    wfHistory(
      r,
      "Assigned " + taskAction(r).action + " to " + taskPerson(who) + ".",
    );
    closeDialog();
    render();
    notify("Task assigned to " + taskPerson(who) + ".");
    return;
  }
  if (form.dataset.form !== "task-draft") return;
  event.preventDefault();
  const r = findRecord(form.dataset.module, form.dataset.record);
  if (!r || !taskCanAct(r)) return;
  if (event.submitter?.value !== "save") {
    taskStartReview(r, form);
    return;
  }
  taskStoreDraft(form, r);
  const key = wfDraftKey(r),
    expected = state.taskDraftVersions[key],
    context = ctxKey();
  if (expected !== r.version) {
    showTaskConflict(r);
    return;
  }
  if (taskDraftRefIssues(form).length) {
    refreshTaskIssues(r, true);
    return;
  }
  runSave($("task-status"), event.submitter, () => {
    if (context !== ctxKey() || !taskCanAct(r) || r.version !== expected)
      return;
    retainFieldReferences(r, form);
    r.fields = clone(wfData(r));
    wfHistory(r, "Saved task draft; no business action submitted.");
    delete state.wfDrafts[key];
    delete state.taskDraftVersions[key];
    delete state.taskDraftBases[key];
    render();
    notify("Draft saved. No action submitted.");
  });
});
document.addEventListener("click", (event) => {
  const b = event.target.closest('[data-action^="task-"]');
  if (!b) return;
  const a = b.dataset.action,
    r =
      b.dataset.module && b.dataset.record
        ? findRecord(b.dataset.module, b.dataset.record)
        : null;
  if (a === "task-module") {
    state.taskModule = b.dataset.id;
    state.taskFilter = "todo";
    state.workTab = "tasks";
    go("work");
    return;
  }
  if (a === "task-clear-module") {
    state.taskModule = null;
    render();
    return;
  }
  if (a === "task-filter") {
    state.taskFilter = b.dataset.id;
    state.taskQuery = "";
    render();
    return;
  }
  if (a === "task-new") {
    const k = b.dataset.module;
    if (!moduleAllowed(k) || moduleReadOnly(k)) return;
    const newRecord = createWorkflow(
      k,
      state.holding,
      state.branch === "all"
        ? holding()?.branches[0]?.id || "all"
        : state.branch,
      "-TASK-" + ++state.sequence,
    );
    newRecord.createdBy = state.persona;
    newRecord.taskEvents = [];
    if (k === "people_payroll") newRecord.fields.employee = person().name;
    if (k === "sales")
      Object.assign(newRecord.fields, {
        recipient: "",
        acceptance: false,
        delivery: false,
      });
    records.unshift(newRecord);
    taskNavigate(newRecord);
    return;
  }
  if (!r) return;
  const key = wfDraftKey(r);
  if (a === "task-demo-change") {
    const field = recordFields(r).find((f) => f.type === "number");
    if (field) r.fields[field.key] = Number(r.fields[field.key]) + 1;
    r.version++;
    r.history.unshift({
      by: "Demo colleague",
      text: "Updated the record in a simulated second session.",
      time: "Just now",
    });
    closeDialog();
    render();
    notify("Another session changed the record. Your draft is preserved.");
    return;
  }
  if (a === "task-demo-unassigned") {
    r.taskUnassigned = true;
    closeDialog();
    render();
    notify("Demo: the task now needs an eligible assignee.");
    return;
  }
  if (a === "task-demo-window") {
    r.fields.window = false;
    r.version++;
    closeDialog();
    render();
    notify("Demo: the enrollment window has closed.");
    return;
  }
  if (a === "task-open") {
    taskNavigate(r);
    return;
  }
  if (a === "task-edit" || a === "task-continue") {
    state.taskManualOpen ||= {};
    state.taskManualOpen[key] = true;
    delete state.taskReviews[key];
    delete state.taskOutcomes[key];
    render();
    return;
  }
  if (a === "task-rebase") {
    const base = state.taskDraftBases[key] || r.fields,
      draft = wfData(r);
    const edits = Object.fromEntries(
      Object.entries(draft).filter(
        ([field, value]) =>
          JSON.stringify(value) !== JSON.stringify(base[field]),
      ),
    );
    state.wfDrafts[key] = { ...r.fields, ...edits };
    state.taskDraftBases[key] = clone(r.fields);
    state.taskDraftVersions[key] = r.version;
    delete state.taskReviews[key];
    closeDialog();
    render();
    notify(
      "Draft preserved. Check the current details before reviewing again.",
    );
    return;
  }
  if (a === "task-switch-role") {
    const target = taskAssignments(r, state.persona).find(
      (x) => x.role === b.dataset.id,
    );
    if (!target) return;
    setRole(target.role);
    taskNavigate(r);
    return;
  }
  if (a === "task-evidence" && taskCanAct(r)) {
    const form = taskCurrentForm(r);
    if (form) taskStoreDraft(form, r);
    r.evidence.push({
      name: "Completion evidence " + (r.evidence.length + 1),
      asset: "assets/warehouse.svg",
      by: person().name,
    });
    wfHistory(r, "Attached demo completion evidence.");
    state.taskDraftVersions[key] = r.version;
    render();
    notify("Evidence attached to this record.");
    return;
  }
  if (a === "task-help") {
    const details = document.querySelector('[data-task-detail="discussion"]');
    if (!details) return;
    document.querySelector(".task-tools")?.removeAttribute("open");
    details.open = true;
    state.taskDetails[ctxKey() + ":" + location.hash + ":discussion"] = true;
    const input = details.querySelector("textarea");
    input.value =
      "Could you help with " +
      (taskAction(r)?.action.toLowerCase() || "this record") +
      "? ";
    state.editorRefs[editorKey(input.id)] = [refOf(r, "needs help with")];
    input.focus();
    input.dispatchEvent(new Event("input", { bubbles: true }));
    notify("Help message prepared. Edit it and add the comment when ready.");
    return;
  }
  if (
    a === "task-assign" &&
    ["owner", "manager", "platform"].includes(role().type)
  ) {
    const eligible = Object.keys(personas).filter((id) => taskEligible(r, id));
    openDialog(
      "Assign responsible person",
      `<form data-form="task-assignment" data-module="${r.module}" data-record="${esc(r.id)}"><p>${esc(taskAction(r)?.action)} · ${esc(branchName(r.branch))}</p><label class="field">Responsible person<select name="person">${eligible.map((id) => `<option value="${id}" ${taskOwner(r) === id ? "selected" : ""}>${esc(taskPerson(id))}</option>`).join("")}</select></label><p class="small muted">Only people with an eligible assignment in this scope are listed.</p><button class="btn primary" type="submit" ${eligible.length ? "" : "disabled"}>Assign task</button></form>`,
    );
    return;
  }
  if (a === "task-retry-handoff" && r.pendingHandoff?.by === state.persona) {
    if (r.pendingHandoff.sourceVersion !== r.version) {
      openDialog(
        "Review the current handoff source",
        `<p>The original action is complete. The source has changed since the draft preparation attempt.</p>${taskFacts(r, r.fields)}<p>Prepare the next draft from these current values?</p>`,
        `<button class="btn" data-action="close-dialog">Back</button><button class="btn primary" data-action="task-retry-current" data-module="${r.module}" data-record="${esc(r.id)}">Prepare from this version</button>`,
      );
      return;
    }
    if (["offline", "degraded"].includes(state.demoState)) {
      notify("Reconnect before preparing the next draft.");
      return;
    }
    prepareTaskHandoff(r, r.pendingHandoff);
    if (state.demoState === "handoff-failure") state.demoState = "normal";
    render();
    notify("Next draft prepared. The original action was not repeated.");
    return;
  }
  if (a === "task-retry-current" && r.pendingHandoff?.by === state.persona) {
    if (["offline", "degraded", "expired"].includes(state.demoState)) return;
    r.pendingHandoff.sourceVersion = r.version;
    prepareTaskHandoff(r, r.pendingHandoff);
    closeDialog();
    render();
    return;
  }
  if (a !== "task-confirm") return;
  confirmTaskReview(r, b);
});
document.addEventListener(
  "toggle",
  (event) => {
    const d = event.target;
    if (!d.matches?.("[data-task-detail]")) return;
    state.taskDetails[
      ctxKey() + ":" + location.hash + ":" + d.dataset.taskDetail
    ] = d.open;
  },
  true,
);
function enhanceTasks() {
  const p = parseRoute();
  if (["tasks", "rec"].includes(p.page)) {
    document.querySelectorAll("[data-task-detail]").forEach((d) => {
      const saved =
        state.taskDetails[
          ctxKey() + ":" + location.hash + ":" + d.dataset.taskDetail
        ];
      if (saved !== undefined) d.open = saved;
    });
    const r = findRecord(p.key, p.id);
    if (r) {
      refreshTaskIssues(r);
      const comment = document.querySelector('[data-form="wf-comment"]'),
        saved = state.taskCommentDrafts[wfDraftKey(r)];
      if (comment && saved) {
        comment.elements.text.value = saved.text;
        if (comment.elements.visibility)
          comment.elements.visibility.value = saved.visibility;
      }
    }
  }
  if (
    new URLSearchParams(location.hash.split("?")[1]).get("view") ===
    "discussion"
  )
    requestAnimationFrame(focusRecordDiscussion);
  syncTaskViewport();
}
window.ERPX_TASKS = {
  taskItems,
  taskView,
  taskOwner,
  taskCanAct,
  taskBundle,
  enhanceTasks,
  taskIssues,
  taskRoute,
};

function confirmTaskReview(r, b) {
  const key = wfDraftKey(r);
  const review = state.taskReviews[key];
  if (!review || review.context !== ctxKey()) return;
  if (review.version !== r.version || review.stage !== r.stage) {
    showTaskConflict(r);
    return;
  }
  if (!taskCanAct(r)) {
    delete state.taskReviews[key];
    render();
    notify("This task is no longer assigned to your active role.");
    return;
  }
  const issues = taskIssues(r, review.values);
  if (
    review.sources.some(
      (x) => !resolveRef(x.ref) || resolveRef(x.ref).version !== x.version,
    )
  )
    issues.push(
      "A source changed during review. Return to the task and check it again.",
    );
  if (!referencesVisible(review.refs))
    issues.push("A review reference is no longer available.");
  if (issues.length) {
    $("task-status").textContent = issues.join(" ");
    return;
  }
  runSave($("task-status"), b, () => {
    if (
      review.context !== ctxKey() ||
      review.version !== r.version ||
      review.stage !== r.stage ||
      !taskCanAct(r) ||
      review.sources.some(
        (x) => !resolveRef(x.ref) || resolveRef(x.ref).version !== x.version,
      ) ||
      taskIssues(r, review.values).length
    ) {
      if ($("task-status"))
        $("task-status").textContent =
          "The task changed. Return to the task and review again.";
      return;
    }
    r.fields = clone(review.values);
    r.fieldRefs = { ...r.fieldRefs, ...review.fieldRefs };
    for (const ref of review.refs)
      if (
        !r.refs.some(
          (x) =>
            x.type === ref.type && x.module === ref.module && x.id === ref.id,
        )
      )
        r.refs.push(ref);
    delete state.wfDrafts[key];
    delete state.taskDraftVersions[key];
    delete state.taskDraftBases[key];
    completeTask(r, review.bundle);
    delete state.taskReviews[key];
    state.taskOutcomes[key] = { action: review.action, at: Date.now() };
    if (state.taskManualOpen) delete state.taskManualOpen[key];
    taskRefresh();
  });
}

function taskDemoControls() {
  const grid = document.querySelector(".demo-state-grid"),
    p = parseRoute();
  if (!grid || !["tasks", "rec"].includes(p.page)) return;
  const r = findRecord(p.key, p.id);
  if (!r) return;
  for (const [a, label] of [
    ["task-demo-change", "Record changes in another session"],
    ["task-demo-unassigned", "Assignee becomes unavailable"],
    ...(r.module === "benefits"
      ? [["task-demo-window", "Close enrollment window"]]
      : []),
  ]) {
    grid.insertAdjacentHTML(
      "beforeend",
      `<button class="btn" data-action="${a}" data-module="${r.module}" data-record="${esc(r.id)}">${label}</button>`,
    );
  }
}
window.ERPX_TASKS.taskDemoControls = taskDemoControls;
