import { expect, it } from "vitest";
import { accessProblems, assignmentProblem, safeReturnPath, type AccessAssignment } from "../src/lib/access-problem";
const now = 1000;
const valid: AccessAssignment = { applies: true, validRole: true, revoked_at: null, valid_from: 100, valid_until: 2000 };
it.each([
  ["permission", [valid]],
  ["scope", [{ ...valid, applies: false }]],
  ["expired", [{ ...valid, valid_until: now }]],
  ["revoked", [{ ...valid, revoked_at: 800 }]],
  ["scheduled", [{ ...valid, valid_from: 1500 }]],
  ["role", [{ ...valid, validRole: false }]],
  ["no_access", []],
  ["scheduled", [{ ...valid, valid_until: 900 }, { ...valid, valid_from: 1500 }]],
  ["permission", [{ ...valid, revoked_at: 800 }, valid]],
  ["expired", [{ ...valid, valid_until: 900 }, { ...valid, applies: false }]],
  ["no_access", [{ ...valid, applies: false, valid_until: 900 }]],
] as const)("classifies %s from the account's own applicable assignments", (reason, rows) => {
  expect(assignmentProblem([...rows], now)).toBe(reason);
});
it.each(["https://evil.example", "//evil.example", "/\\evil.example", "/api/v1/delete", "/login", "/access?reason=expired", "/holdings/%2f%2fevil.example", "/holdings/acme/\rhome", "/holdings/acme/%5cfoo", "/holdings/acme/%00home", "/holdings/../../api/auth/sign-out", "/holdings/%2e%2e/access", "/holdings/acme/..", null, "x".repeat(2001)])("rejects unsafe or looping return path %s", value => {
  expect(safeReturnPath(value)).toBeNull();
});
it("accepts scoped application links and retains query parameters and fragments", () => {
  const path = "/holdings/acme/branches/bangkok/departments/operations/home?tag=delivery&filter=a%2Fb#current%2Fitem";
  expect(safeReturnPath(path)).toBe(path);
  expect(safeReturnPath("/platform/home")).toBe("/platform/home");
});
it("gives every supported problem a distinct explanation and a next step", () => {
  expect(new Set(Object.values(accessProblems).map(p => p.title)).size).toBe(Object.keys(accessProblems).length);
  for (const problem of Object.values(accessProblems)) { expect(problem.description.length).toBeGreaterThan(30); expect(problem.next.length).toBeGreaterThan(30); }
});
