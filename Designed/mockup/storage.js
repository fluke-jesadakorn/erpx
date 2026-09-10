/* Versioned IndexedDB persistence for the browser-local mockup. */
const ERPX_DB_NAME = "erpx-connected-mockup";
const ERPX_DB_VERSION = 2;
const ERPX_SNAPSHOT_KEY = "workspace";
let erpxLoadedRevision = 0;
let erpxSaveTimer = null;
let erpxSaveChain = Promise.resolve();
const erpxChannel = "BroadcastChannel" in window ? new BroadcastChannel("erpx-connected-mockup") : null;

function erpxOpenDB() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(ERPX_DB_NAME, ERPX_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains("snapshots")) db.createObjectStore("snapshots");
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
async function erpxReadSnapshot() {
  const db = await erpxOpenDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("snapshots", "readonly"), req = tx.objectStore("snapshots").get(ERPX_SNAPSHOT_KEY);
    req.onsuccess = () => resolve(req.result || null);
    req.onerror = () => reject(req.error);
    tx.oncomplete = () => db.close();
  });
}
function erpxSnapshot(revision) {
  return { schema: ERPX_DB_VERSION, revision, savedAt: Date.now(), state, records, posts, stories };
}
async function erpxPersist(force = false) {
  const db = await erpxOpenDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction("snapshots", "readwrite"), store = tx.objectStore("snapshots"), req = store.get(ERPX_SNAPSHOT_KEY);
    req.onsuccess = () => {
      const current = req.result;
      if (!force && current?.revision > erpxLoadedRevision) {
        state.storageConflict = true;
        render();
        notify("Another tab saved newer changes. Reload them or keep this tab’s version.");
        tx.abort();
        resolve(false);
        return;
      }
      const revision = Math.max(erpxLoadedRevision, current?.revision || 0) + 1;
      store.put(erpxSnapshot(revision), ERPX_SNAPSHOT_KEY);
      erpxLoadedRevision = revision;
      state.storageConflict = false;
      erpxChannel?.postMessage({ revision });
    };
    tx.oncomplete = () => { db.close(); resolve(true); };
    tx.onerror = () => { db.close(); reject(tx.error); };
  });
}
function erpxScheduleSave(delay = 750) {
  clearTimeout(erpxSaveTimer);
  erpxSaveTimer = setTimeout(() => {
    erpxSaveChain = erpxSaveChain.then(() => erpxPersist()).catch(() => {
      state.storageError = true;
      render();
      notify("Changes could not be saved in this browser. Your current page is still available.");
    });
  }, delay);
}
async function erpxRestore() {
  try {
    const saved = await erpxReadSnapshot();
    if (!saved || saved.schema > ERPX_DB_VERSION) return false;
    Object.assign(state, saved.state || {});
    records = saved.records || records;
    posts = saved.posts || posts;
    stories = saved.stories || stories;
    state.capabilityRuns ||= {};
    state.attachmentDrafts ||= {};
    state.taskCommentDrafts ||= {};
    state.taskDetails ||= {};
    state.storageConflict = false;
    state.storageError = false;
    erpxLoadedRevision = saved.revision || 0;
    return true;
  } catch {
    state.storageError = true;
    return false;
  }
}
async function erpxClearStorage() {
  const db = await erpxOpenDB();
  await new Promise((resolve, reject) => {
    const tx = db.transaction("snapshots", "readwrite");
    tx.objectStore("snapshots").delete(ERPX_SNAPSHOT_KEY);
    tx.oncomplete = resolve;
    tx.onerror = () => reject(tx.error);
  });
  db.close();
  erpxLoadedRevision = 0;
}
erpxChannel?.addEventListener("message", (event) => {
  if (event.data?.revision > erpxLoadedRevision) {
    state.storageConflict = true;
    render();
  }
});
for (const eventName of ["click", "change", "submit"]) document.addEventListener(eventName, (event) => {
  if (eventName === "click" && event.target.closest('[data-action="reset"]')) erpxClearStorage().catch(() => {});
  erpxScheduleSave(eventName === "submit" ? 1100 : 800);
});
document.addEventListener("input", () => erpxScheduleSave(900));
document.addEventListener("click", async (event) => {
  const action = event.target.closest("[data-storage-action]")?.dataset.storageAction;
  if (action === "reload") { await erpxRestore(); render(); notify("Newer browser changes loaded."); }
  if (action === "overwrite") { await erpxPersist(true); render(); notify("This tab’s version is now saved."); }
});
window.ERPX_STORAGE = { restore: erpxRestore, save: erpxPersist, schedule: erpxScheduleSave, clear: erpxClearStorage };
