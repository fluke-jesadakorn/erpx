import { businessDate as evaluateBusinessDate } from "../domain/pos";
/** Compatibility default; injected applications pass their own clock value. */
export function businessDate(timezone = "Asia/Bangkok", date = new Date()) { return evaluateBusinessDate(timezone, date); }
