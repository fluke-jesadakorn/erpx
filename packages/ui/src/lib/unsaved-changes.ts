"use client";

const checks = new Set<() => boolean>();

/** Shared UI registrations let navigation owners ask whether mounted edits remain. */
export function registerUnsavedChanges(check: () => boolean) {
  checks.add(check);
  return () => { checks.delete(check); };
}

export function hasRegisteredUnsavedChanges() {
  return [...checks].some(check => check());
}
