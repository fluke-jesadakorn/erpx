"use client";
import { Button } from "./button";
import { mergeAiDraft, type RecoverableDraft, type Citation } from "@/lib/shared/ai-draft";
import { useEffect, useState } from "react";

/** Browser recovery is tied to the server fields, so an upload can refresh safely
 * while a concurrent edit requires the human to review before overwriting. */
export function useRecoverableDraft<T extends Record<string, string>>(
  key: string,
  initial: T,
  label = "record",
) {
  const baseline = JSON.stringify(initial);
  type Draft = RecoverableDraft<T>;
  const [state, setState] = useState<Draft>({
    fields: initial,
    baseline,
    ai: {},
  });
  const conflict = state.baseline !== baseline;
  useEffect(() => {
    let restored: Draft = {
      fields: JSON.parse(baseline) as T,
      baseline,
      ai: {},
    };
    try {
      const transferred = JSON.parse(sessionStorage.getItem(`${key}:touched`) ?? "[]") as string[];
      restored.touched = transferred;
      const raw = sessionStorage.getItem(key);
      if (raw) {
        const saved = JSON.parse(raw) as Draft;
        if (saved.fields && typeof saved.baseline === "string")
          restored = {
            ...saved,
            baseline: JSON.stringify(saved.fields) === baseline ? baseline : saved.baseline,
            ai: saved.ai ?? {},
            touched: [...new Set([...transferred, ...(saved.touched ?? [])])],
          };
      }
    } catch {}
    // Hydrate this user's browser recovery after the server render.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setState(restored);
  }, [key, baseline]);
  function changeDraft(change: (previous: Draft) => Draft) {
    setState((previous) => {
      const next = change(previous);
      if (next === previous) return previous;
      try {
        sessionStorage.setItem(key, JSON.stringify(next));
      } catch {}
      return next;
    });
  }
  function update(change: (previous: T) => T, touchedNames: string[] = []) {
    changeDraft((previous) => {
      const fields = change(previous.fields),
        ai = { ...previous.ai };
      const changed = new Set(touchedNames);
      for (const name of Object.keys(fields))
        if (fields[name] !== previous.fields[name]) changed.add(name);
      const touched = new Set([...(previous.touched ?? []), ...changed]);
      for (const name of changed) delete ai[name];
      return { ...previous, fields, ai, touched: [...touched] };
    });
  }
  function receiveAi(event: {
    fields: { name: string; value: string | null; citations?: Citation[] }[];
    overwrite: boolean;
    proposalId?: string | null;
    invalidate?: boolean;
  }) {
    changeDraft((previous) => mergeAiDraft(previous, baseline, event));
  }

  function clear() {
    try {
      sessionStorage.removeItem(key);
    } catch {}
    setState((previous) => ({
      ...previous,
      baseline: JSON.stringify(previous.fields),
      ai: {},
    }));
  }
  function accept() {
    changeDraft((previous) => ({ ...previous, baseline }));
  }
  function discard() {
    clear();
    setState({ fields: JSON.parse(baseline) as T, baseline, ai: {} });
  }
  return {
    fields: state.fields,
    ai: state.ai,
    touched: state.touched ?? [],
    update,
    receiveAi,
    clear,
    discard,
    dirty: JSON.stringify(state.fields) !== state.baseline,
    conflict,
    notice: conflict ? (
      <div className="notice" role="alert">
        <p>
          This {label} changed while you were editing. Your unsaved fields are preserved. Review
          them against the current {label} before saving.
        </p>
        <div className="form-actions">
          <Button type="button" variant="secondary" onClick={accept}>
            I reviewed my saved edits
          </Button>
          <Button type="button" variant="secondary" onClick={discard}>
            Use current server fields
          </Button>
        </div>
      </div>
    ) : null,
  };
}
