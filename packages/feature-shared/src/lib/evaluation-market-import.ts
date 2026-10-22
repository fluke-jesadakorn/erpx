import { benchmarkSchema } from "@erpx/feature-people_payroll/lib/people-evaluation";

/** RFC-style quoted cells, including commas/newlines; reject malformed imports before saving. */
export function parseEvaluationMarketCsv(input: string) {
  const rows: string[][] = [], row: string[] = [];
  let cell = "", quoted = false, closed = false;
  const data = input.replace(/^\uFEFF/, "");
  for (let i = 0; i < data.length; i++) {
    const c = data[i];
    if (quoted) { if (c === '"') { if (data[i + 1] === '"') { cell += '"'; i++; } else { quoted = false; closed = true; } } else cell += c; continue; }
    if (c === '"') { if (cell || closed) throw new Error("Invalid CSV quoting."); quoted = true; }
    else if (c === "," || c === "\n" || c === "\r") {
      row.push(cell); cell = ""; closed = false;
      if (c !== ",") { if (c === "\r" && data[i + 1] === "\n") i++; if (row.some(c => c.trim())) rows.push([...row]); row.length = 0; }
    } else { if (closed && c.trim()) throw new Error("Unexpected text after a quoted cell."); if (!closed) cell += c; }
  }
  if (quoted) throw new Error("An imported CSV cell has no closing quote.");
  if (cell || row.length) { row.push(cell); rows.push([...row]); }
  const header = rows.shift()?.map(c => c.trim());
  const required = ["name", "family", "levelId", "location", "currency", "minimum", "midpoint", "maximum", "source", "observedOn", "validUntil"];
  if (!header || header.length !== required.length || new Set(header).size !== required.length || required.some(k => !header.includes(k))) throw new Error(`CSV columns must be: ${required.join(", ")}`);
  if (!rows.length || rows.length > 100) throw new Error("Import between 1 and 100 market rows.");
  return rows.map((row, index) => {
    if (row.length !== header.length) throw new Error(`Row ${index + 2}: column count does not match.`);
    const r = Object.fromEntries(header.map((h, i) => [h, row[i].trim()]));
    const amount = (name: string) => { if (!/^\d+(\.\d{1,2})?$/.test(r[name])) throw new Error(`Row ${index + 2}: ${name} must be a nonnegative currency amount.`); return Math.round(Number(r[name]) * 100); };
    return benchmarkSchema.parse({ name: r.name, family: r.family, levelId: r.levelId, location: r.location, currency: r.currency, source: r.source, observedOn: r.observedOn, validUntil: r.validUntil, minimumMinor: amount("minimum"), midpointMinor: amount("midpoint"), maximumMinor: amount("maximum"), basis: "monthly_fte", confirmed: false });
  });
}
