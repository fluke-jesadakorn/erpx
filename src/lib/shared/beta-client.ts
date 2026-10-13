
import { workFetch } from "@/lib/work_projection/work-fetch";
export async function betaFetch<T>(
  url: string,
  input?: unknown,
  key: string = crypto.randomUUID(),
): Promise<T> {
  const r = await workFetch(url, {
    method: input === undefined ? "GET" : "POST",
    headers:
      input === undefined
        ? {}
        : { "Content-Type": "application/json", "Idempotency-Key": key },
    body: input === undefined ? undefined : JSON.stringify(input),
  });
  const body = (await r.json()) as { data: T; error?: { message: string } };
  if (!r.ok) {
    const message = body.error?.message ?? "Please try again.";
    const requestId = r.headers.get("X-Request-ID");
    throw Object.assign(new Error(requestId ? `${message} (Reference: ${requestId})` : message), { status:r.status, code:(body.error as {code?:string}|undefined)?.code });
  }
  return body.data;
}
