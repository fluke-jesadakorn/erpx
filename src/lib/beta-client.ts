export async function betaFetch<T>(
  url: string,
  input?: unknown,
  key: string = crypto.randomUUID(),
): Promise<T> {
  const r = await fetch(url, {
    method: input === undefined ? "GET" : "POST",
    headers:
      input === undefined
        ? {}
        : { "Content-Type": "application/json", "Idempotency-Key": key },
    body: input === undefined ? undefined : JSON.stringify(input),
  });
  const body = (await r.json()) as { data: T; error?: { message: string } };
  if (!r.ok) throw new Error(body.error?.message ?? "Please try again.");
  return body.data;
}
