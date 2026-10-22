"use client";

let generation = 0;
let switching = false;
const dirtyChecks = new Set<() => boolean>();
const contextSwitchBlockers = new Set<() => string | null>();

export function registerUnsavedWork(check: () => boolean) {
  dirtyChecks.add(check);
  return () => { dirtyChecks.delete(check); };
}

export function registerContextSwitchBlocker(reason: () => string | null) {
  contextSwitchBlockers.add(reason);
  return () => { contextSwitchBlockers.delete(reason); };
}

export function contextSwitchBlockReason() {
  for (const reason of contextSwitchBlockers) {
    const message = reason();
    if (message) return message;
  }
  return null;
}

export function hasUnsavedWork() {
  return [...dirtyChecks].some(check => check()) ||
    !!document.querySelector('[data-work-dirty="true"]');
}

export function confirmContextSwitch(message = "Change account or role and discard unsaved changes?") {
  return !contextSwitchBlockReason() && (!hasUnsavedWork() || window.confirm(message));
}

export function contextGeneration() { return generation; }
export function contextIsSwitching() { return switching; }

export function setContextSwitching(value: boolean) {
  switching = value;
  document.documentElement.dataset.workSwitching = String(value);
  if (value) {
    ++generation;
    window.dispatchEvent(new Event("erpx:context-switch-started"));
  }
}

export function announceContextSwitch() {
  ++generation;
  // In-flight requests must never populate the old role's mounted components.
  document.querySelectorAll<HTMLElement>("[data-work-stale]").forEach(node => {
    node.dataset.workStale = "true";
  });
  if (typeof BroadcastChannel !== "undefined") {
    const channel = new BroadcastChannel("erpx-work-context");
    channel.postMessage({ changed: true });
    channel.close();
  }
  window.dispatchEvent(new Event("erpx:work-context-changed"));
}
