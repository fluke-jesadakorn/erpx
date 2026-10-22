/** RFC-style CSV, including escaped quotes, quoted commas and newlines. */
export function financeCsvRows(csv: string): Record<string, string>[] {
  if (csv.length > 5_000_000) throw new Error("CSV is too large.");
  const rows: string[][] = [],
    row: string[] = [];
  let cell = "",
    quoted = false,
    endedQuote = false;
  function pushCell() {
    row.push(cell);
    cell = "";
    endedQuote = false;
  }
  function pushRow() {
    pushCell();
    if (row.some((c) => c !== "")) rows.push([...row]);
    row.length = 0;
  }
  for (let i = 0; i < csv.length; i++) {
    const c = csv[i];
    if (quoted) {
      if (c === '"') {
        if (csv[i + 1] === '"') {
          cell += '"';
          i++;
        } else {
          quoted = false;
          endedQuote = true;
        }
      } else cell += c;
      continue;
    }
    if (c === '"') {
      if (cell || endedQuote) throw new Error("Invalid CSV quote.");
      quoted = true;
    } else if (c === ",") pushCell();
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && csv[i + 1] === "\n") i++;
      pushRow();
    } else {
      if (endedQuote) throw new Error("Unexpected text after a quoted field.");
      cell += c;
    }
  }
  if (quoted) throw new Error("Unclosed CSV quote.");
  if (cell || row.length || endedQuote) pushRow();
  if (rows.length < 2 || rows.length > 10001)
    throw new Error("CSV needs a header and 1–10,000 data rows.");
  const header = rows[0].map((s) => s.replace(/^\uFEFF/, "").trim());
  if (header.some((s) => !s) || new Set(header).size !== header.length)
    throw new Error("CSV headers must be nonempty and unique.");
  return rows.slice(1).map((r, i) => {
    if (r.length !== header.length)
      throw new Error(`CSV row ${i + 2} has the wrong number of columns.`);
    return Object.fromEntries(header.map((h, j) => [h, r[j].trim()]));
  });
}
export function financeCsv(headers: string[], rows: unknown[][], escapeFormulas=true) {
  const cell = (v: unknown) => {
    let text=String(v??"");
    const trimmed=text.trimStart();
    if(escapeFormulas&&(/^[=+@\t\r]/.test(text)||/^[=+@]/.test(trimmed)||(/^[-]/.test(trimmed)&&!/^[-]?\d+(?:\.\d+)?$/.test(trimmed))))text="'"+text;
    return `"${text.replace(/"/g,'""')}"`;
  };
  return (
    [headers, ...rows].map((r) => r.map(cell).join(",")).join("\r\n") + "\r\n"
  );
}
