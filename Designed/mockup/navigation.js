/* Shared presentation and scoped return navigation for the static mockup. */
function postDiscovery(p) {
  const followed = state.following.includes(actorKey(p.id));
  const tools =
    followButton("person", p.by) +
    (p.ref ? followButton("record", p.ref.module + "/" + p.ref.id) : "");
  return `<div class="post-discovery">${freshness() === "Fresh" ? "" : `<span class="freshness-label">${freshness()}</span>`}${(
    p.tags || []
  )
    .filter((t) => visibleTopics().some((x) => x.tag === t))
    .map(
      (t) =>
        `<a href="${route("tags/" + encodeURIComponent(t.slice(1)))}">${esc(t)}</a>`,
    )
    .join(
      "",
    )}<details class="post-follow-menu"><summary>Follow options</summary><div class="follow-options">${tools}<button data-action="follow" data-id="${p.id}" aria-pressed="${followed}">${followed ? "Following discussion" : "Follow discussion"}</button></div></details></div>`;
}
function navigationKey(hash = location.hash) {
  return ctxKey() + ":" + hash;
}
function navigationSnapshot() {
  return {
    hash: location.hash,
    scroll: scrollY,
    filter: state.filter,
    statusFilter: state.statusFilter,
    taskQuery: state.taskQuery,
    taskFilter: state.taskFilter,
    taskScope: state.taskScope,
    taskModule: state.taskModule,
    group: state.group,
    focus: document.activeElement?.id || null,
    focusHref:
      document.activeElement?.closest("a")?.getAttribute("href") || null,
    focusAction: document.activeElement?.dataset?.action || null,
  };
}
function rememberNavigation(target) {
  state.navigationViews ||= {};
  state.navigationReturns ||= {};
  const view = navigationSnapshot();
  state.navigationViews[navigationKey()] = view;
  if (target && target !== location.hash && /\/(tasks|rec)\//.test(target)) {
    const prior = state.navigationReturns[navigationKey()];
    state.navigationReturns[navigationKey(target)] =
      /\/(tasks|rec)\//.test(location.hash) && prior ? prior : view;
  }
}
function restoreNavigationView() {
  const saved = state.navigationViews?.[navigationKey()];
  if (saved)
    for (const key of [
      "filter",
      "statusFilter",
      "taskQuery",
      "taskFilter",
      "taskScope",
      "taskModule",
      "group",
    ])
      state[key] = saved[key];
  else {
    state.filter = "";
    state.statusFilter = "all";
  }
}
function restoreNavigationPosition() {
  const saved = state.navigationViews?.[navigationKey()];
  requestAnimationFrame(() => {
    window.scrollTo(0, saved?.scroll || 0);
    if (saved?.focus || saved?.focusAction)
      (
        document.getElementById(saved.focus) ||
        [...document.querySelectorAll("a")].find(
          (a) => a.getAttribute("href") === saved.focusHref,
        ) ||
        (saved.focusAction &&
          document.querySelector(`[data-action="${saved.focusAction}"]`))
      )?.focus({ preventScroll: true });
    if (
      new URLSearchParams(location.hash.split("?")[1]).get("view") ===
      "discussion"
    )
      focusRecordDiscussion();
  });
}
function backToOrigin() {
  const saved = state.navigationReturns?.[navigationKey()];
  if (saved) {
    state.navigationViews[navigationKey(saved.hash)] = saved;
    location.hash = saved.hash;
  } else go("work");
}
function focusRecordDiscussion() {
  const section = document.querySelector('[data-task-detail="discussion"]');
  if (section) {
    section.open = true;
    section.querySelector("textarea,input")?.focus();
    section.scrollIntoView({ block: "start" });
  } else
    document
      .querySelector('.comment-form input,[data-form="record-comment"] input')
      ?.focus();
}
function liveSearch(event) {
  const el = event.target,
    form = el.closest("form");
  if (!form) return;
  const name = form.dataset.form;
  if (!["task-search", "module-filter", "record-filter"].includes(name)) return;
  const pos = el.selectionStart;
  if (name === "task-search") state.taskQuery = form.elements.query.value;
  else {
    state.filter = form.elements.query.value;
    if (form.elements.group) state.group = form.elements.group.value;
    if (form.elements.status) state.statusFilter = form.elements.status.value;
  }
  render();
  const next = document.querySelector(
    `[data-form="${name}"] [name="${el.name}"]`,
  );
  next?.focus({ preventScroll: true });
  if (next?.setSelectionRange && pos !== null) next.setSelectionRange(pos, pos);
}
document.addEventListener("input", liveSearch);
document.addEventListener("change", liveSearch);
document.addEventListener(
  "click",
  (event) => {
    const anchor = event.target.closest('a[href^="#"]');
    if (anchor) {
      if (!anchor.id)
        anchor.id =
          "return-link-" +
          [...document.querySelectorAll('a[href^="#"]')].indexOf(anchor);
      rememberNavigation(anchor.getAttribute("href"));
      delete state.recordModes[navigationKey(anchor.getAttribute("href"))];
    }
  },
  true,
);
document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-action]");
  if (!button) return;
  const action = button.dataset.action;
  if (action === "task-back") backToOrigin();
  if (action === "task-new-menu")
    openDialog(
      "Start new work",
      `<div class="new-work-list">${role()
        .modules.filter((k) => !moduleReadOnly(k))
        .map(
          (k) =>
            `<button class="choice" data-action="task-new" data-module="${k}">${symbol(k)}<span><strong>${esc(MODULES[k].create)}</strong><small>${esc(SEED.CTX[k].n)}</small></span></button>`,
        )
        .join("")}</div>`,
    );
  if (action === "task-filters")
    openDialog(
      "Filter your work",
      `<label class="field">Module<select id="task-module-choice"><option value="">All modules</option>${role()
        .modules.map(
          (k) =>
            `<option value="${k}" ${state.taskModule === k ? "selected" : ""}>${esc(SEED.CTX[k].n)}</option>`,
        )
        .join("")}</select></label>`,
      `<button class="btn primary" data-action="task-apply-filters">Show tasks</button>`,
    );
  if (action === "task-apply-filters") {
    state.taskModule = $("task-module-choice").value || null;
    closeDialog();
    render();
  }
  if (action === "task-assistant") {
    const r = findRecord(button.dataset.module, button.dataset.record);
    if (r) {
      openDialog("Draft assistant", draftAssistant(r));
      $("dialog").dataset.assistantKey = wfDraftKey(r);
    }
  }
  if (action === "task-details-mode") {
    const r = findRecord(button.dataset.module, button.dataset.record);
    if (r) {
      state.recordModes[navigationKey()] = "details";
      render();
    }
  }
  if (action === "task-action-mode") {
    const r = findRecord(button.dataset.module, button.dataset.record);
    if (r) {
      state.recordModes[navigationKey()] = "action";
      render();
    }
  }
});
document.addEventListener("change", (event) => {
  if (event.target.id === "task-scope") {
    state.taskScope = event.target.value;
    render();
  }
});
// A fixed action bar follows the visual viewport when a phone keyboard opens.
function syncTaskViewport() {
  const viewport = window.visualViewport;
  const inset = viewport
    ? Math.max(0, window.innerHeight - viewport.height - viewport.offsetTop)
    : 0;
  document.documentElement.style.setProperty("--keyboard-inset", inset + "px");
}
window.visualViewport?.addEventListener("resize", syncTaskViewport);
window.visualViewport?.addEventListener("scroll", syncTaskViewport);

function refreshAssistantDialog() {
  const d = $("dialog");
  if (!d?.open || !d.querySelector(".assistant-card")) return;
  const p = parseRoute(),
    r = findRecord(p.key, p.id);
  if (!r || d.dataset.assistantKey !== wfDraftKey(r)) return;
  d.querySelector(".assistant-card").outerHTML = draftAssistant(r);
}
window.ERPX_NAVIGATION = {
  postDiscovery,
  restoreNavigationView,
  restoreNavigationPosition,
  rememberNavigation,
  refreshAssistantDialog,
};

document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape" || $("dialog").open) return;
  const menu = document.activeElement?.closest(
    ".task-tools[open],.section-more[open],.post-follow-menu[open]",
  );
  if (menu) {
    menu.open = false;
    menu.querySelector("summary").focus();
    event.preventDefault();
  }
});
document.addEventListener("click", (event) => {
  document
    .querySelectorAll(
      ".task-tools[open],.section-more[open],.post-follow-menu[open]",
    )
    .forEach((menu) => {
      if (!menu.contains(event.target)) menu.open = false;
    });
});
