import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { parseSyntheticPayrollBankResultCsv } from "../src/lib/payroll-bank-result";

const fixture = readFileSync(new URL("../fixtures/payroll-thai/synthetic-bank-result-v1.csv", import.meta.url), "utf8");

describe("synthetic payroll bank result", () => {
  it("parses a confirmed row and a rejected row without treating either as bank evidence", () => {
    expect(parseSyntheticPayrollBankResultCsv(fixture).rows).toEqual([
      { reference: "PAY-synthetic-run-item-001", status: "confirmed", confirmationRef: "TX-SYNTHETIC-001", bankTransactionId: "bank-tx-synthetic-001", bankVersion: 1 },
      { reference: "PAY-synthetic-run-item-002", status: "failed" },
    ]);
  });

  it("accepts quoted commas and CRLF in a single field", () => {
    const csv = 'reference,status,confirmation_ref,bank_transaction_id,bank_version\r\n"PAY,001",confirmed,TX-1,bank-1,2\r\n';
    expect(parseSyntheticPayrollBankResultCsv(csv).rows[0].reference).toBe("PAY,001");
  });

  it("rejects duplicate references and transaction matches", () => {
    const header = "reference,status,confirmation_ref,bank_transaction_id,bank_version\n";
    expect(() => parseSyntheticPayrollBankResultCsv(header + "PAY-1,failed,,,\nPAY-1,failed,,,\n")).toThrow("Duplicate payment reference");
    expect(() => parseSyntheticPayrollBankResultCsv(header + "PAY-1,confirmed,TX-1,bank-1,1\nPAY-2,confirmed,TX-2,bank-1,1\n")).toThrow("Duplicate bank transaction");
  });

  it("rejects evidence-free confirmations and evidence on failed rows", () => {
    const header = "reference,status,confirmation_ref,bank_transaction_id,bank_version\n";
    expect(() => parseSyntheticPayrollBankResultCsv(header + "PAY-1,confirmed,TX-1,,1\n")).toThrow("requires an exact bank transaction");
    expect(() => parseSyntheticPayrollBankResultCsv(header + "PAY-1,failed,TX-1,,\n")).toThrow("cannot carry settlement evidence");
  });

  it("rejects malformed CSV and unexpected columns", () => {
    expect(() => parseSyntheticPayrollBankResultCsv('reference,status\nPAY-1,failed\n')).toThrow("Unexpected");
    expect(() => parseSyntheticPayrollBankResultCsv('reference,status,confirmation_ref,bank_transaction_id,bank_version\n"PAY-1,failed,,,\n')).toThrow("Unterminated");
  });
});
