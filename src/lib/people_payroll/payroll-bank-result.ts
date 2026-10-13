/**
 * Synthetic payroll bank-result interchange for staging and acceptance tests.
 * A bank-specific adapter must transform its signed result into this shape;
 * parsing a CSV never establishes that a transaction actually happened.
 */
export type PayrollBankResultRow = {
  reference: string;
  status: "confirmed" | "failed";
  confirmationRef?: string;
  bankTransactionId?: string;
  bankVersion?: number;
};

const HEADER = ["reference", "status", "confirmation_ref", "bank_transaction_id", "bank_version"];
const MAX_BYTES = 2_000_000;
const MAX_ROWS = 5_000;

function csvCells(source: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  let closedQuote = false;
  let atCellStart = true;

  for (let i = 0; i < source.length; i += 1) {
    const char = source[i];
    if (quoted) {
      if (char === '"' && source[i + 1] === '"') {
        cell += '"';
        i += 1;
      } else if (char === '"') {
        quoted = false;
        closedQuote = true;
      } else {
        cell += char;
      }
      continue;
    }
    if (char === '"') {
      if (!atCellStart || closedQuote) throw new Error("Malformed CSV quote.");
      quoted = true;
      atCellStart = false;
      continue;
    }
    if (char === "," || char === "\n" || char === "\r") {
      row.push(cell);
      cell = "";
      closedQuote = false;
      atCellStart = true;
      if (char !== ",") {
        if (char === "\r" && source[i + 1] === "\n") i += 1;
        rows.push(row);
        if (rows.length > MAX_ROWS + 1) throw new Error("Bank result exceeds the row limit.");
        row = [];
      }
      continue;
    }
    if (closedQuote) throw new Error("Malformed CSV quote.");
    cell += char;
    atCellStart = false;
  }
  if (quoted) throw new Error("Unterminated CSV quote.");
  if (cell || row.length || !source.endsWith("\n")) {
    row.push(cell);
    rows.push(row);
  }
  return rows;
}

export function parseSyntheticPayrollBankResultCsv(source: string): { rows: PayrollBankResultRow[] } {
  if (new TextEncoder().encode(source).length > MAX_BYTES) throw new Error("Bank result exceeds the file limit.");
  const raw = csvCells(source.replace(/^\uFEFF/, ""));
  if (!raw.length || raw[0].length !== HEADER.length || raw[0].some((cell, index) => cell !== HEADER[index])) {
    throw new Error("Unexpected synthetic bank-result columns.");
  }
  if (raw.length < 2 || raw.length > MAX_ROWS + 1) throw new Error("Bank result must contain 1–5000 rows.");
  const seenReferences = new Set<string>();
  const seenTransactions = new Set<string>();
  const rows = raw.slice(1).map((cells, index): PayrollBankResultRow => {
    if (cells.length !== HEADER.length) throw new Error(`Bank result row ${index + 2} has the wrong number of columns.`);
    const [reference, status, confirmationRef, bankTransactionId, bankVersion] = cells;
    if (!reference || reference !== reference.trim() || reference.length > 240 || /[\r\n\0]/u.test(reference)) {
      throw new Error(`Invalid payment reference on row ${index + 2}.`);
    }
    if (seenReferences.has(reference)) throw new Error(`Duplicate payment reference on row ${index + 2}.`);
    seenReferences.add(reference);
    if (status !== "confirmed" && status !== "failed") throw new Error(`Invalid result status on row ${index + 2}.`);
    if (status === "failed") {
      if (confirmationRef || bankTransactionId || bankVersion) throw new Error(`Failed row ${index + 2} cannot carry settlement evidence.`);
      return { reference, status };
    }
    if (!confirmationRef || confirmationRef.length > 160 || /[\r\n\0]/u.test(confirmationRef) || !bankTransactionId || bankTransactionId.length > 240 || /[\r\n\0]/u.test(bankTransactionId) || !/^[1-9]\d*$/u.test(bankVersion)) {
      throw new Error(`Confirmed row ${index + 2} requires an exact bank transaction, version and reference.`);
    }
    const version = Number(bankVersion);
    if (!Number.isSafeInteger(version)) throw new Error(`Invalid bank transaction version on row ${index + 2}.`);
    if (seenTransactions.has(bankTransactionId)) throw new Error(`Duplicate bank transaction on row ${index + 2}.`);
    seenTransactions.add(bankTransactionId);
    return { reference, status, confirmationRef, bankTransactionId, bankVersion: version };
  });
  return { rows };
}
