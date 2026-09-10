/* Shared browser-local attachments for record threads and completion evidence. */
const ATTACHMENT_LIMIT = 10;
const ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;
const ATTACHMENT_TYPES = ["image/jpeg", "image/png", "image/webp", "application/pdf", "text/csv"];
const attachmentUrls = new WeakMap();

function attachmentKey(r, purpose = "discussion") { return `${wfDraftKey(r)}:${purpose}`; }
function attachmentURL(a) {
  if (!a?.blob) return a?.asset || "";
  if (!attachmentUrls.has(a.blob)) attachmentUrls.set(a.blob, URL.createObjectURL(a.blob));
  return attachmentUrls.get(a.blob);
}
function attachmentIcon(a) { return a.type?.startsWith("image/") ? "image" : a.type === "text/csv" ? "chart" : "file"; }
function attachmentList(items = [], removable = false, key = "") {
  return items.length ? `<div class="attachment-grid">${items.map((a, i) => `<article class="attachment-card">${a.type?.startsWith("image/") ? `<img src="${attachmentURL(a)}" alt="${esc(a.caption || a.name)}">` : `<span class="attachment-file">${icon(attachmentIcon(a))}</span>`}<span class="grow"><strong>${esc(a.name)}</strong><small>${Math.max(1, Math.round(a.size / 1024))} KB${a.caption ? " · " + esc(a.caption) : ""}</small></span><a class="icon-btn" href="${attachmentURL(a)}" target="_blank" download="${esc(a.name)}" aria-label="Preview or download ${esc(a.name)}">${icon("arrow")}</a>${removable ? `<button type="button" class="icon-btn" data-action="attachment-remove" data-key="${esc(key)}" data-index="${i}" aria-label="Remove ${esc(a.name)}">${icon("close")}</button>` : ""}</article>`).join("")}</div>` : "";
}
function attachmentComposer(r, purpose = "discussion") {
  const key = attachmentKey(r, purpose), items = state.attachmentDrafts?.[key] || [];
  return `<div class="attachment-drop" data-attachment-drop="${esc(key)}"><input type="file" id="attachment-${esc(slug(key))}" data-attachment-input="${esc(key)}" accept="image/jpeg,image/png,image/webp,application/pdf${purpose === "import" ? ",text/csv,.csv" : ""}" multiple><label class="btn" for="attachment-${esc(slug(key))}">${icon("paperclip")}Attach files</label><span class="tiny muted">Images${purpose === "import" ? ", PDF or CSV" : " or PDF"} · up to 10 MB each · drop files here</span></div><p class="attachment-status tiny muted" data-attachment-status="${esc(key)}"></p>${attachmentList(items, true, key)}`;
}
function takeAttachmentDraft(r, purpose = "discussion") {
  const key = attachmentKey(r, purpose), items = state.attachmentDrafts?.[key] || [];
  if (state.attachmentDrafts) delete state.attachmentDrafts[key];
  return items.map((a) => ({ ...a, by: person().name, time: nowLabel }));
}
function addAttachmentFiles(key, files) {
  state.attachmentDrafts ||= {};
  const current = state.attachmentDrafts[key] || [], accepted = [];
  let error = "";
  for (const file of files) {
    if (current.length + accepted.length >= ATTACHMENT_LIMIT) { error = "A submission can contain up to 10 files."; break; }
    if (!ATTACHMENT_TYPES.includes(file.type) && !file.name.toLowerCase().endsWith(".csv")) { error = `${file.name} is not a supported image, PDF or CSV.`; continue; }
    if (file.size > ATTACHMENT_MAX_BYTES) { error = `${file.name} is larger than 10 MB.`; continue; }
    accepted.push({ id: `att-${Date.now()}-${accepted.length}`, name: file.name, type: file.type || "text/csv", size: file.size, caption: "", blob: file, version: 1 });
  }
  state.attachmentDrafts[key] = [...current, ...accepted];
  return error;
}
document.addEventListener("change", (event) => {
  const input = event.target.closest("[data-attachment-input]");
  if (!input) return;
  const error = addAttachmentFiles(input.dataset.attachmentInput, [...input.files]);
  render();
  const status = [...document.querySelectorAll("[data-attachment-status]")].find((el) => el.dataset.attachmentStatus === input.dataset.attachmentInput);
  if (status) status.textContent = error || `${input.files.length} file${input.files.length === 1 ? "" : "s"} ready to attach.`;
});
document.addEventListener("dragover", (event) => { if (event.target.closest("[data-attachment-drop]")) event.preventDefault(); });
document.addEventListener("drop", (event) => {
  const drop = event.target.closest("[data-attachment-drop]");
  if (!drop) return;
  event.preventDefault();
  addAttachmentFiles(drop.dataset.attachmentDrop, [...event.dataTransfer.files]);
  render();
});
document.addEventListener("click", (event) => {
  const b = event.target.closest('[data-action="attachment-remove"]');
  if (b) {
    state.attachmentDrafts?.[b.dataset.key]?.splice(Number(b.dataset.index), 1);
    render();
    return;
  }
  const save = event.target.closest('[data-action="attachment-save-evidence"]');
  if (!save) return;
  const r = findRecord(save.dataset.module, save.dataset.record);
  if (!r || !taskCanAct(r)) return;
  const files = takeAttachmentDraft(r, "evidence");
  if (!files.length) return;
  r.evidence.push(...files);
  wfHistory(r, `Attached ${files.length} completion evidence file${files.length === 1 ? "" : "s"}.`);
  render();
  notify("Evidence saved to this record. It can now satisfy the completion gate.");
});
window.ERPX_ATTACHMENTS = { attachmentComposer, attachmentList, takeAttachmentDraft, attachmentURL };
