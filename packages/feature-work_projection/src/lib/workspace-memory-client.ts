"use client";

let pendingMemoryWrite: Promise<void> = Promise.resolve();

/**
 * Serializes passive workspace-memory writes in this browser document. This
 * prevents an older mounted workspace from overwriting an explicit Company
 * switch that follows it immediately.
 */
export function enqueueWorkspaceMemory(write: () => Promise<unknown>) {
  pendingMemoryWrite = pendingMemoryWrite
    .then(async () => {
      await write();
    })
    .catch(() => {
      // Remembering the last page is best effort; selection endpoints remain
      // authoritative and surface their own errors.
    });
  return pendingMemoryWrite;
}

export async function waitForWorkspaceMemory() {
  await pendingMemoryWrite;
}
