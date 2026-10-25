import { get } from "node:http";
import { pathToFileURL } from "node:url";

// Execute this transport only with the pinned Node binary: Bun 1.2.17's native
// HTTP Client crashes during these probes on the current macOS runtime.
export async function waitForBrowserReadiness(origin: string, pid: number, options: { deadlineMs?: number; requestMs?: number; intervalMs?: number; settleMs?: number } = {}) {
  const url = new URL(origin);
  if (url.protocol !== "http:" || url.hostname !== "127.0.0.1" || !Number.isInteger(pid) || pid < 1) throw new Error("BROWSER_READINESS_ARGUMENTS_REQUIRED");
  const until = Date.now() + (options.deadlineMs ?? 120_000);
  const pause = (ms: number) => new Promise<void>(done => setTimeout(done, ms));
  while (Date.now() < until) {
    try { process.kill(pid, 0); }
    catch { throw new Error("BROWSER_COHORT_PROCESS_EXITED"); }
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), Math.min(options.requestMs ?? 3_000, until - Date.now()));
    try {
      const statuses = await Promise.all(["health", "ready"].map(path => new Promise<number>((done, reject) => {
        const request = get(new URL(`/api/${path}`, url), { signal: controller.signal }, response => {
          response.once("error", reject);
          response.once("aborted", () => reject(new Error("BROWSER_READINESS_RESPONSE_ABORTED")));
          response.once("end", () => done(response.statusCode ?? 0));
          response.resume();
        });
        request.once("error", reject);
      })));
      if (statuses.every(status => status === 200) && Date.now() < until) {
        await pause(options.settleMs ?? 250);
        try { process.kill(pid, 0); }
        catch { throw new Error("BROWSER_COHORT_PROCESS_EXITED"); }
        return;
      }
    } catch (error) {
      if (error instanceof Error && error.message === "BROWSER_COHORT_PROCESS_EXITED") throw error;
      // Network and response details stay private; the caller retains cohort logs.
    } finally { clearTimeout(timeout); controller.abort(); }
    await pause(Math.min(options.intervalMs ?? 500, Math.max(0, until - Date.now())));
  }
  throw new Error("BROWSER_COHORT_NOT_READY");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.version !== "v24.20.0") throw new Error("PINNED_TOOLCHAIN_REQUIRED");
    await waitForBrowserReadiness(process.argv[2], Number(process.argv[3]));
  } catch (error) {
    const code = error instanceof Error && /^(BROWSER_|PINNED_TOOLCHAIN_REQUIRED)/.test(error.message) ? error.message : "BROWSER_READINESS_FAILED";
    console.error(code);
    process.exitCode = 1;
  }
}
