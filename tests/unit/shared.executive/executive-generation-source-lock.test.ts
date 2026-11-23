import { describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const read = (path: string) => readFileSync(join(root, path), "utf8");

const sourceLock = "packages/server-platform_administration/src/adapters/executive-generation-source-lock.ts";
const sourceLockPublic = "packages/server-platform_administration/src/public/executive-generation-source-lock.ts";
const generation = "packages/server-runtime/src/adapters/executive/generation.ts";
const registry = "packages/server-runtime/src/adapters/native_command_sql.rs";
const platformRoute = "packages/server-platform_administration/src/adapters/platform_executive_http.rs";

describe("Platform executive generation-lock read is native", () => {
  it("deletes the last platform executive SELECT and its exports", () => {
    expect(existsSync(join(root, sourceLock))).toBe(false);
    expect(existsSync(join(root, sourceLockPublic))).toBe(false);
    expect(read("packages/server-platform_administration/package.json")).not.toContain("executive-generation-source-lock");
  });

  it("switches runtime generation to the native source-lock statement", () => {
    const source = read(generation);
    expect(source).not.toContain("executiveGenerationSourceLock");
    expect(source).toContain("nativeRuntimeStatement('runtime_stmt_23'");
    expect(source).toContain("{ companyId: company, now: Date.now(), actor: undefined }).bind(company)");
  });

  it("admits the retired read literal into the native statement registry", () => {
    const source = read(registry);
    expect(source).toContain('kind: "runtime_stmt_23"');
    expect(source).toContain(
      "SELECT company_id FROM engineering_verification_contexts WHERE company_id=$1 FOR SHARE",
    );
  });

  it("keeps the platform executive native route authority", () => {
    expect(read(platformRoute)).toContain('/api/v1/platform/executive');
  });
});
