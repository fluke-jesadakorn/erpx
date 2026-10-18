import type { AccountPreferences, AccountPreferenceSnapshot } from "@/server/modules/identity_access/public/account-preferences";

type Values = Pick<AccountPreferences, "language" | "appearance">;
export type PreferenceState = AccountPreferenceSnapshot & { revision: number; pending: boolean; error: string };
export type PreferenceRefreshOutcome='ok'|'skipped'|'error'|'auth'|'context';
type Ports = {
  read: (signal: AbortSignal) => Promise<AccountPreferenceSnapshot>;
  write: (key: string, version: number, values: Values, signal: AbortSignal) => Promise<AccountPreferenceSnapshot>;
  apply: (preference: AccountPreferences) => void;
  generation: () => number | null;
  changed: () => void;
};
const sameValues = (a: Values, b: Values) => a.language === b.language && a.appearance === b.appearance;
const owned = (key: string) => /^[a-f0-9]{64}$/.test(key);

/** No I/O on construction/seed observation. Only explicit edits, focus after
 * leaving the page, and peer notices may read. Requests never share accounts. */
export function createAccountPreferencesClient(initial: AccountPreferenceSnapshot, ports: Ports) {
  let state: PreferenceState = { ...initial, revision: 0, pending: false, error: "" };
  let saved = initial.preference;
  let desired: Values | null = null, retry: Values | null = null;
  const pendingValues = (): Values | null => desired;
  let flight: AbortController | null = null;
  let reading: Promise<PreferenceRefreshOutcome> | null = null;
  let epoch = 0;
  const listeners = new Set<() => void>();
  const emit = (patch: Partial<PreferenceState>) => {
    state = { ...state, ...patch };
    for (const listener of listeners) listener();
  };
  const present = (preference: AccountPreferences) => {
    ports.apply(preference);
    emit({ preference, revision: state.revision + 1 });
  };
  const current = (ticket: number, generation: number | null, signal: AbortSignal) =>
    !signal.aborted && ticket === epoch && generation !== null && ports.generation() === generation;
  const accept = (snapshot: AccountPreferenceSnapshot) => {
    if (snapshot.key !== state.key || !snapshot.preference || (saved && snapshot.preference.version < saved.version)) return false;
    saved = snapshot.preference.version === 0 && state.preference
      ? { ...state.preference, version: 0 } : snapshot.preference;
    return true;
  };
  async function drain() {
    if (flight || !desired || !owned(state.key) || ports.generation() === null) return;
    const request = new AbortController(), ticket = epoch, generation = ports.generation();
    flight = request;
    emit({ pending: true, error: "" });
    try {
      if (!saved) {
        const snapshot = await ports.read(request.signal);
        if (!current(ticket, generation, request.signal)) return;
        if (!accept(snapshot)) throw new Error("Your account changed. Reload before saving preferences.");
      }
      while (desired && current(ticket, generation, request.signal)) {
        const submitted: Values = desired;
        if (saved!.version > 0 && sameValues(saved!, submitted)) { desired = null; break; }
        const snapshot = await ports.write(state.key, saved!.version, submitted, request.signal);
        if (!current(ticket, generation, request.signal)) return;
        if (!accept(snapshot)) throw new Error("Your account changed. Reload before saving preferences.");
        ports.changed();
        if (desired === submitted) desired = null;
        if (!desired) present(saved!);
      }
      retry = null;
    } catch (error) {
      if (!current(ticket, generation, request.signal)) return;
      retry = desired;
      desired = null;
      // A response can be lost after commit or another tab can win the CAS.
      // Read once to show the saved truth; never silently overwrite it.
      try {
        const snapshot = await ports.read(request.signal);
        if (!current(ticket, generation, request.signal)) return;
        accept(snapshot);
      } catch { /* Keep the last known saved preference during an outage. */ }
      const latest = pendingValues();
      if (saved) present(latest ? { ...saved, ...latest } : saved);
      if (retry && saved?.version && sameValues(saved, retry)) retry = null;
      emit({ error: retry && !desired ? error instanceof Error ? error.message : "Could not save your preferences. Try again." : "" });
    } finally {
      if (flight === request) {
        flight = null;
        emit({ pending: false });
        if (desired) void drain();
      }
    }
  }
  return {
    getSnapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
    observe(snapshot: AccountPreferenceSnapshot) {
      if (snapshot.key !== state.key) return;
      const previous = saved;
      if (accept(snapshot) && saved !== previous && !desired && !flight &&
        (!previous || saved!.version > previous.version || !sameValues(saved!, previous))) present(saved!);
    },
    seedLegacy(values: Values) {
      if ((!saved||saved.version===0) && !flight && !desired) {
        saved = { ...values, version: 0 };
        emit({ preference: saved });
      }
    },
    change(values: Partial<Values>, fallback?: Values) {
      const base = state.preference ?? fallback;
      if (!base || state.key === "unavailable") return;
      const next = { language: base.language, appearance: base.appearance, ...values };
      present({ ...next, version: saved?.version ?? 0 });
      if (!owned(state.key)) return;
      desired = next;
      retry = null;
      void drain();
    },
    retry() {
      if (!retry || flight) return;
      desired = retry;
      if (saved) present({ ...retry, version: saved.version });
      void drain();
    },
    receive(snapshot: AccountPreferenceSnapshot) {
      if (snapshot.key !== state.key || !snapshot.preference || (saved && snapshot.preference.version < saved.version)) return;
      ++epoch; flight?.abort(); flight = null; reading = null; desired = null; retry = null;
      accept(snapshot);
      present(saved!);
      emit({ pending: false, error: "" });
    },
    refresh() {
      if (reading || flight || !owned(state.key) || ports.generation() === null) return reading ?? Promise.resolve<PreferenceRefreshOutcome>('skipped');
      const request = new AbortController(), ticket = epoch, generation = ports.generation();
      flight = request;
      const work = (async ():Promise<PreferenceRefreshOutcome> => {
        try {
          const snapshot = await ports.read(request.signal);
          if(!current(ticket,generation,request.signal)||snapshot.key!==state.key)return 'context';
          if(!accept(snapshot))return 'skipped';
          present(desired ? { ...saved!, ...desired } : saved!);return 'ok';
        } catch(error) { /* A failed optional read never changes the saved truth. */
          if(!current(ticket,generation,request.signal))return 'context';
          return [401,403].includes(Number((error as {status?:number})?.status))?'auth':'error';
        }
        finally {
          if (flight === request) { flight = null; reading = null; if (desired) void drain(); }
        }
      })();
      reading = work;
      return work;
    },
    cancel() {
      ++epoch; flight?.abort(); flight = null; reading = null; desired = null; retry = null;
      emit({ preference: saved, pending: false, error: "" });
    },
  };
}
