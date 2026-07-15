export function safeJson(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return { _raw: raw };
  }
}

export function sanitizeHeaders(h: Headers, drop: string[] = []): Record<string, string> {
  const dropLower = new Set(drop.map((d) => d.toLowerCase()));
  const out: Record<string, string> = {};
  h.forEach((v, k) => {
    if (!dropLower.has(k.toLowerCase())) out[k] = v;
  });
  return out;
}