export function normalizeVendorName(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[\u0E00-\u0E7F]/g, ch => ch)
    .replace(/[^a-z0-9\u0E00-\u0E7F\s]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

export function tokensOverlap(a: string, b: string): number {
  const ta = new Set(normalizeVendorName(a).split(' ').filter(Boolean));
  const tb = new Set(normalizeVendorName(b).split(' ').filter(Boolean));
  if (ta.size === 0 || tb.size === 0) return 0;
  let inter = 0;
  for (const t of ta) if (tb.has(t)) inter += 1;
  return inter / Math.max(ta.size, tb.size);
}