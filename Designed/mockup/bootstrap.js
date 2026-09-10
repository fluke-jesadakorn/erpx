(async function bootstrap() {
  seed();
  await window.ERPX_STORAGE?.restore();
  document.documentElement.classList.toggle("dark", prefs().theme === "dark");
  try { localStorage.removeItem("erpx-me"); } catch {}
  if (!location.hash) history.replaceState(null, "", route());
  render();
  window.ERPX_READY = true;
  document.dispatchEvent(new CustomEvent("erpx-ready"));
})();
// Read-only diagnostics for the static-browser verification script.
window.ERPX_DEMO = {
  snapshot: () =>
    clone({
      state,
      recordCount: records.length,
      postCount: posts.length,
      visibleRecordIds: visibleRecords().map((r) => r.id),
      visiblePostIds: visiblePosts().map((p) => p.id),
      moduleIds: Object.keys(SEED.CTX),
    }),
  assertScope: () =>
    visiblePosts().every(
      (p) =>
        p.holding === state.holding &&
        (!p.ref || !!findRecord(p.ref.module, p.ref.id)),
    ),
};
