/** Share an in-flight read; a different key replaces it without publishing stale data. */
export function createLatestRead<T>(timeoutMs = 20_000) {
  let pending: { key: string; controller: AbortController; promise: Promise<void> } | null = null;
  function cancel() {
    pending?.controller.abort();
    pending = null;
  }
  function read(key: string, load: (signal: AbortSignal) => Promise<T>, publish: (value: T) => void): Promise<void> {
    if (pending?.key === key) return pending.promise;
    cancel();
    const controller = new AbortController();
    const signal = AbortSignal.any([controller.signal, AbortSignal.timeout(timeoutMs)]);
    const request = { key, controller, promise: Promise.resolve() };
    pending = request;
    request.promise = (async () => {
      try {
        const value = await load(signal);
        if (pending === request) {
          // Body decoding or another loader may finish after its deadline even
          // when it does not observe abort; never publish that expired result.
          signal.throwIfAborted();
          publish(value);
        }
      } catch (error) {
        if (!controller.signal.aborted) throw error;
      } finally {
        if (pending === request) pending = null;
      }
    })();
    return request.promise;
  }
  return { read, cancel };
}
