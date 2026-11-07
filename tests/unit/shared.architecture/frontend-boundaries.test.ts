import { describe, expect, it } from "vitest";
import { inspectSource } from "../../../scripts/architecture/boundaries";
import { inspectArchitecture } from "../../../scripts/architecture/boundaries";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = process.cwd();
const domain = "packages/feature-finance/src/domain/command-policy.ts";
const application = "packages/feature-finance/src/application/submit.ts";
const rules = (source: string, text: string) => inspectSource(root, source, text).map(issue => issue.rule);

describe("frontend portable interaction boundaries", () => {
  it("rejects React types, Next effects, and browser adapters from frontend cores", () => {
    expect(rules(domain, 'import type { ReactNode } from "react";')).toContain("core-infrastructure");
    expect(rules(application, 'import { useRouter } from "next/navigation";')).toContain("core-infrastructure");
    expect(rules(domain, 'export { financeCommandFetch } from "../adapters/browser/finance-command-fetch";')).toContain("frontend-core-presentation-dependency");
  });

  it("rejects server-owner dependencies even when they are type-only or dynamically loaded", () => {
    for (const text of [
      'import type { Actor } from "@erpx/server-finance/public/finance";',
      'type Actor = import("@erpx/server-finance/public/finance").Actor;',
      'const backend = () => import("@erpx/server-finance/public/finance");',
    ]) expect(rules(application, text)).toContain("frontend-core-server-dependency");
  });

  it("keeps DOM, storage and browser transport types in adapters", () => {
    const found = rules(domain, 'const a = document.activeElement; const b = globalThis["sessionStorage"]; type Result = Response;');
    expect(found).toContain("frontend-core-browser-global");
    expect(found).toContain("frontend-core-browser-type");
    expect(rules(domain.replace(/\.ts$/, ".tsx"), 'const value = <div />;')).toContain("frontend-core-presentation");
  });

  it("allows injected effects and pure retry state without framework imports", () => {
    expect(rules(application, 'type Ports = { fetch(input: string): Promise<string>; storage: { get(key: string): string | null } }; export async function run(ports: Ports) { return ports.fetch(ports.storage.get("draft") ?? ""); }')).toEqual([]);
    expect(rules(domain, 'export function signature(target: string, scope: readonly string[], body: string) { return JSON.stringify([target, scope, body]); }')).toEqual([]);
  });

  it("recognizes explicit frontend public facades while preserving core direction", () => {
    expect(rules("packages/feature-finance/src/components/editor.tsx", 'import { financeCommandFetch } from "@erpx/feature-finance/public/client";')).toEqual([]);
    expect(rules(application, 'import type { FinanceCommandIdentity } from "../public/contracts";')).toEqual([]);
    expect(rules(domain, 'import { run } from "../application/submit";')).toContain("core-dependency-direction");
  });

  it("follows type-only public reexports so they cannot hide React dependencies", () => {
    const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "erpx-frontend-boundaries-"));
    try {
      const source = path.join(temporary, "packages/feature-finance/src");
      fs.mkdirSync(path.join(source, "domain"), { recursive: true });
      fs.mkdirSync(path.join(source, "public"), { recursive: true });
      fs.writeFileSync(path.join(source, "domain/policy.ts"), 'type Label = import("../public/contracts").Label;');
      fs.writeFileSync(path.join(source, "public/contracts.ts"), 'export type { ReactNode as Label } from "react";');
      expect(inspectArchitecture(temporary).some(issue => issue.rule === "core-transitive-dependency" && issue.target.includes("core-infrastructure: react"))).toBe(true);
    } finally {
      fs.rmSync(temporary, { recursive: true, force: true });
    }
  });
});
