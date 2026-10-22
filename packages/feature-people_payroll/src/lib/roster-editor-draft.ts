export type RosterEditorAttempt = {
  url: string; body: string; scope: string; key: string;
  editor: number; edit: number; pending: boolean; uncertain: boolean;
};

/** Acknowledgements belong to the submitted editor revision. A later local
 * edit must survive an earlier save, and an uncertain creation cannot silently
 * become a different command with another identity. */
export function createRosterEditorDraft(nextKey: () => string = () => crypto.randomUUID()) {
  let editor = 0, edit = 0, dirty = false, attempt: RosterEditorAttempt | null = null;
  return {
    open() { editor++; edit = 0; dirty = false; attempt = null; return editor; },
    changed() { edit++; dirty = true; },
    isDirty() { return dirty; },
    revision() { return edit; },
    pending() { return attempt?.pending === true; },
    begin(url: string, body: string, scope: string) {
      if (attempt?.pending) return null;
      const same = attempt?.url === url && attempt.body === body && attempt.scope === scope;
      if (attempt?.uncertain && !same) throw new Error("Retry the submitted change before saving a different draft. Your newer edits are retained.");
      if (!same) attempt = {url, body, scope, key: nextKey(), editor, edit, pending: false, uncertain: false};
      dirty = true;
      attempt!.pending = true;
      return attempt!;
    },
    failed(submitted: RosterEditorAttempt, uncertain: boolean) {
      if (submitted !== attempt) return;
      submitted.pending = false; submitted.uncertain = uncertain;
    },
    acknowledged(submitted: RosterEditorAttempt, currentScope = submitted.scope) {
      if (submitted !== attempt || submitted.editor !== editor) return false;
      if (submitted.scope !== currentScope)
        throw Object.assign(new Error("Your account, Company or role changed. Return to the original roster role before retrying."), {code: "WORK_CONTEXT_CHANGED"});
      submitted.pending = false;
      attempt = null;
      const unchanged = submitted.edit === edit;
      if (unchanged) dirty = false;
      return unchanged;
    },
  };
}

export function rosterCommandAcknowledgement(value: unknown): {id: string; version: number} {
  if (!value || typeof value !== "object" || !("id" in value) || typeof value.id !== "string" || !value.id ||
    !("version" in value) || !Number.isSafeInteger(value.version) || Number(value.version) <= 0)
    throw new Error("The save acknowledgement is incomplete. Retry the submitted change safely.");
  return {id: value.id, version: Number(value.version)};
}

export function currentRosterSnapshot<T extends {asOf: number}>(current: T, next: T) {
  return next.asOf >= current.asOf ? next : current;
}
