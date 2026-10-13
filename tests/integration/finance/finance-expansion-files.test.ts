import { afterAll, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import { setFinanceCompanyTool } from "../../../src/server/modules/finance/public/finance-company-tools";
import {
  prepareBudgetEdition,
  decideFinancePlan,
} from "../../../src/server/modules/finance/public/finance-planning";
import {
  createAttachmentSet,
  reserveAttachmentFile,
  completeAttachmentUpload,
  drainAttachmentScans,
  bindRecordAttachmentSet,
  listAttachmentSets,
} from "../../../src/server/modules/files/public/attachments/service";
import { sha256 } from "../../../src/server/modules/runtime/adapters/crypto";
import {
  prepareBookRule,
  decideBookRule,
  bookRuleSchema,
} from "../../../src/server/modules/finance/public/finance-books";
import {
  prepareBusinessContract,
  decideBusinessContract,
  businessContractSchema,
} from "../../../src/server/modules/finance/public/finance-business-contracts";
import {
  prepareInstrument,
  decideInstrument,
  instrumentEditionSchema,
} from "../../../src/server/modules/finance/public/finance-instruments";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
const png = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=",
  "base64",
);
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  vi.stubEnv("ERPX_ATTACHMENTS_V2", "true");
  vi.stubEnv("ERPX_ATTACHMENT_UPLOAD_FREEZE", "false");
});
afterAll(() => vi.unstubAllEnvs());
beforeEach(async () => {
  f = await expenseFixture();
  f.owner.role = "finance_preparer";
  f.owner.assignmentId = await f.grant(f.owner, "finance_preparer");
  f.manager.role = "finance_approver";
  f.manager.assignmentId = await f.grant(f.manager, "finance_approver");
  await setFinanceCompanyTool(
    f.owner,
    {
      tool: "planning",
      enabled: true,
      expectedVersion: 0,
      reason: "Isolated File Sources owner accounting verification",
    },
    key(),
  );
});
it("binds actually scanned File Sources before independent review, preserves exact manifest checksum and locks reviewed evidence", async () => {
  const b = await prepareBudgetEdition(
      f.owner,
      {
        chartId: f.chart,
        name: "Budget with original scanned evidence",
        year: 2026,
        lines: [
          { accountId: f.accounts.expense, month: "2026-09", amount: "100.00" },
        ],
      },
      key(),
    ),
    id = String(b.id),
    set = await createAttachmentSet(
      db,
      f.owner,
      {
        targetType: "finance_plan_edition",
        targetId: id,
        componentKind: "record",
        purposeKey: "finance_support",
        expectedTargetVersion: 1,
      },
      key(),
    ),
    reserved = await reserveAttachmentFile(
      db,
      f.owner,
      set.id,
      {
        filename: "reviewed-budget.png",
        mediaType: "image/png",
        bytes: png.byteLength,
      },
      key(),
    );
  await expect(
    decideFinancePlan(
      f.manager,
      id,
      { expectedVersion: 1, reason: "Unfinished file must prevent approval" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "FINANCE_FILE_EVIDENCE_NOT_READY" });
  const uploaded = await completeAttachmentUpload(
    db,
    env.H001_FILES,
    f.owner,
    reserved.uploadIntentId,
    png.buffer.slice(png.byteOffset, png.byteOffset + png.byteLength),
  );
  expect(
    await drainAttachmentScans(db, env.H001_FILES, uploaded.sourceId),
  ).toBe(true);
  await expect(
    decideFinancePlan(
      f.manager,
      id,
      {
        expectedVersion: 1,
        reason: "Clean but unbound file must prevent approval",
      },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "FINANCE_FILE_EVIDENCE_NOT_READY" });
  await bindRecordAttachmentSet(db, f.owner, set.id, 1, key());
  await expect(
    decideFinancePlan(
      f.manager,
      id,
      {
        expectedVersion: 1,
        reason: "File binding advanced the source version",
      },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "STALE_VERSION" });
  await expect(
    db
      .prepare("UPDATE attachments SET source_state='quarantined' WHERE id=?")
      .bind(uploaded.sourceId)
      .run(),
  ).rejects.toThrow("ATTACHMENT_SOURCE_STATE_TRANSITION");
  const command = key(),
    review = {
      expectedVersion: 2,
      reason:
        "Independent Finance reviewer checked scanned original bytes and budget",
    };
  await decideFinancePlan(f.manager, id, review, "approve", command);
  await decideFinancePlan(f.manager, id, review, "approve", command);
  const proof = await db
    .prepare("SELECT * FROM finance_expansion_review_files WHERE command_id=?")
    .bind(command)
    .first<{
      manifest_json: string;
      manifest_sha256: string;
      target_version: number;
    }>();
  expect(proof?.target_version).toBe(2);
  expect(JSON.parse(proof!.manifest_json)).toMatchObject([
    { sourceId: uploaded.sourceId, checksumSha256: await sha256(new Uint8Array(png).buffer) },
  ]);
  expect(proof?.manifest_sha256).toBe(await sha256(proof!.manifest_json));
  await expect(
    db
      .prepare(
        "UPDATE finance_expansion_review_files SET manifest_json='[]' WHERE command_id=?",
      )
      .bind(command)
      .run(),
  ).rejects.toThrow();
  await expect(
    createAttachmentSet(
      db,
      f.owner,
      {
        targetType: "finance_plan_edition",
        targetId: id,
        componentKind: "record",
        purposeKey: "finance_support",
        expectedTargetVersion: 3,
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "ATTACHMENT_TARGET_LOCKED" });
  expect(
    await listAttachmentSets(db, f.manager, {
      targetType: "finance_plan_edition",
      targetId: id,
      componentKind: "record",
      purposeKey: "finance_support",
    }),
  ).toMatchObject([
    { id: set.id, state: "bound", links: [{ sourceId: uploaded.sourceId }] },
  ]);
});
it.each(["book", "contract", "lease"] as const)(
  "binds canonical scanned %s evidence, refuses the old owner version and retains exactly the independently reviewed source",
  async (kind) => {
    const accounts: Record<string, string> = {};
    for (const [code, category, control] of [
      ["AR", "asset", "receivable"],
      ["DEF", "liability", "none"],
      ["BANK", "asset", "bank"],
      ["DEBT", "liability", "none"],
      ["ROU", "asset", "none"],
      ["ACC", "asset", "none"],
      ["INT", "expense", "none"],
      ["DEP", "expense", "none"],
    ]) {
      const id = key();
      accounts[code] = id;
      await db
        .prepare(
          "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',?,?,1,1)",
        )
        .bind(
          id,
          f.chart,
          f.company,
          code,
          code,
          category,
          control === "none" ? 0 : 1,
          control,
        )
        .run();
    }
    let id: string,
      targetType: string,
      table: string,
      decide: (version: number, command: string) => Promise<unknown>;
    if (kind === "book") {
      const chart = key();
      await db
        .prepare(
          "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,'THB',?,1,1)",
        )
        .bind(chart, f.company, "Scanned secondary book", f.owner.userId)
        .run();
      const mappings = [];
      for (const account of (
        await db
          .prepare(
            "SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY id",
          )
          .bind(f.chart)
          .all<{ id: string }>()
      ).results) {
        const to = key();
        await db
          .prepare(
            "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?",
          )
          .bind(to, chart, account.id)
          .run();
        mappings.push({ primaryAccountId: account.id, secondaryAccountId: to });
      }
      const record = await prepareBookRule(
        f.owner,
        bookRuleSchema.parse({
          primaryChartId: f.chart,
          secondaryChartId: chart,
          effectiveDate: "2026-09-01",
          openingPeriodId: f.period,
          reason: "Scanned independently reviewed secondary opening",
          mappings,
        }),
        key(),
      );
      id = String(record.id);
      targetType = "finance_book_rule";
      table = "finance_book_rules";
      decide = (expectedVersion, command) =>
        decideBookRule(
          f.manager,
          id,
          {
            expectedVersion,
            reason: "Independent scanned original opening and maps checked",
          },
          "approve",
          command,
        );
    } else if (kind === "contract") {
      await setFinanceCompanyTool(
        f.owner,
        {
          tool: "business_contracts",
          enabled: true,
          expectedVersion: 0,
          reason: "Canonical scanned contract evidence verification",
        },
        key(),
      );
      const party = key();
      await db
        .prepare(
          "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Synthetic file-evidence customer','THB',1,1)",
        )
        .bind(party, f.company, party)
        .run();
      const record = await prepareBusinessContract(
        f.owner,
        businessContractSchema.parse({
          chartId: f.chart,
          partyId: party,
          number: key(),
          name: "Scanned monthly support",
          currency: "THB",
          startDate: "2026-09-01",
          endDate: "2026-09-30",
          reason: "Original service evidence",
          cycle: "once",
          pricing: { kind: "flat", amount: "100.01" },
          receivableAccountId: accounts.AR,
          deferredAccountId: accounts.DEF,
          obligations: [
            {
              key: "support",
              name: "Support",
              standalonePrice: "100.01",
              revenueAccountId: f.accounts.revenue,
              mode: "point",
            },
          ],
        }),
        key(),
      );
      id = String(record.id);
      targetType = "finance_business_contract";
      table = "finance_business_contracts";
      decide = (expectedVersion, command) =>
        decideBusinessContract(
          f.manager,
          id,
          {
            expectedVersion,
            reason: "Independent scanned original contract checked",
          },
          "approve",
          command,
        );
    } else {
      await setFinanceCompanyTool(
        f.owner,
        {
          tool: "instruments",
          enabled: true,
          expectedVersion: 0,
          reason: "Canonical scanned lease evidence verification",
        },
        key(),
      );
      const record = await prepareInstrument(
        f.owner,
        instrumentEditionSchema.parse({
          kind: "lease",
          number: key(),
          name: "Scanned fixed lease",
          chartId: f.chart,
          currency: "THB",
          startDate: "2026-09-01",
          months: 3,
          annualRateBps: 1200,
          payment: "100.00",
          bankAccountId: accounts.BANK,
          liabilityAccountId: accounts.DEBT,
          interestAccountId: accounts.INT,
          rightOfUseAccountId: accounts.ROU,
          accumulatedAccountId: accounts.ACC,
          depreciationAccountId: accounts.DEP,
          reason: "Original fixed lease evidence",
          periodId: f.period,
        }),
        key(),
      );
      id = String(record.id);
      targetType = "finance_instrument";
      table = "finance_instrument_editions";
      decide = (expectedVersion, command) =>
        decideInstrument(
          f.manager,
          id,
          {
            expectedVersion,
            reason: "Independent scanned original lease and PV checked",
          },
          "approve",
          command,
        );
    }
    const set = await createAttachmentSet(
        db,
        f.owner,
        {
          targetType,
          targetId: id,
          componentKind: "record",
          purposeKey: "finance_support",
          expectedTargetVersion: 1,
        },
        key(),
      ),
      reserved = await reserveAttachmentFile(
        db,
        f.owner,
        set.id,
        {
          filename: "synthetic-original.png",
          mediaType: "image/png",
          bytes: png.byteLength,
        },
        key(),
      ),
      uploaded = await completeAttachmentUpload(
        db,
        env.H001_FILES,
        f.owner,
        reserved.uploadIntentId,
        png.buffer.slice(png.byteOffset, png.byteOffset + png.byteLength),
      );
    expect(
      await drainAttachmentScans(db, env.H001_FILES, uploaded.sourceId),
    ).toBe(true);
    await expect(decide(1, key())).rejects.toMatchObject({
      code: "FINANCE_FILE_EVIDENCE_NOT_READY",
    });
    expect(
      await db
        .prepare(`SELECT version FROM ${table} WHERE id=?`)
        .bind(id)
        .first("version"),
    ).toBe(1);
    await bindRecordAttachmentSet(db, f.owner, set.id, 1, key());
    await expect(decide(1, key())).rejects.toThrow();
    const command = key();
    await decide(2, command);
    await decide(2, command);
    const proof = await db
      .prepare(
        "SELECT target_type,target_version,manifest_json,manifest_sha256 FROM finance_expansion_review_files WHERE command_id=?",
      )
      .bind(command)
      .first<{
        target_type: string;
        target_version: number;
        manifest_json: string;
        manifest_sha256: string;
      }>();
    expect(proof).toMatchObject({ target_type: targetType, target_version: 2 });
    expect(JSON.parse(proof!.manifest_json)).toMatchObject([
      { sourceId: uploaded.sourceId, checksumSha256: await sha256(new Uint8Array(png).buffer) },
    ]);
    expect(proof!.manifest_sha256).toBe(await sha256(proof!.manifest_json));
    await expect(
      createAttachmentSet(
        db,
        f.owner,
        {
          targetType,
          targetId: id,
          componentKind: "record",
          purposeKey: "finance_support",
          expectedTargetVersion: 3,
        },
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_TARGET_LOCKED" });
    if (kind === "lease")
      expect(
        await db
          .prepare(
            "SELECT COALESCE(sum(l.base_debit_minor),0) amount FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND j.status='posted' AND j.source_type='instrument_activation'",
          )
          .bind(f.company)
          .first("amount"),
      ).toBe(29410);
  },
);
