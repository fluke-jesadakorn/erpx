"use client";

let generation = 0;
let switching = false;
const dirtyChecks = new Set<() => boolean>();

export function registerUnsavedWork(check: () => boolean) {
  dirtyChecks.add(check);
  return () => { dirtyChecks.delete(check); };
}

export function hasUnsavedWork() {
  return [...dirtyChecks].some(check => check()) ||
    !!document.querySelector('[data-work-dirty="true"]');
}

export function confirmContextSwitch() {
  return !hasUnsavedWork() || window.confirm("Change account or role and discard unsaved changes?");
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
