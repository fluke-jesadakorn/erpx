import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
import {
  AttachmentFeatureSurface,
  AttachmentFeatureProvider,
  canMutateAttachments,
  useAttachmentFeatureMode,
} from "@erpx/feature-files/components/attachment-feature-context";
import { RecordDocumentDrawer } from "@erpx/feature-files/components/record-document-drawer";

function ModeProbe() {
  return <span>{useAttachmentFeatureMode()}</span>;
}

it("provides one server-derived attachment mode at the application root", () => {
  const source = readFileSync("packages/workplace/src/routes/layout.tsx", "utf8");
  expect(source).toContain("attachmentsV2Enabled()");
  expect(source).toContain("attachmentsV2ReadOnly()");
  expect(source.match(/<AttachmentFeatureProvider\b[^>]*mode=\{attachmentFeatureMode\}/g)).toHaveLength(1);
});

it("serializes the server-selected attachment feature mode into the client boundary", () => {
  expect(
    renderToStaticMarkup(
      <AttachmentFeatureProvider mode="read-only">
        <ModeProbe />
      </AttachmentFeatureProvider>,
    ),
  ).toContain("read-only");
});

it("fails closed when a file surface is rendered outside the application provider", () => {
  expect(renderToStaticMarkup(<ModeProbe />)).toContain("disabled");
  expect(
    renderToStaticMarkup(
      <AttachmentFeatureSurface>
        <button type="button">Files</button>
      </AttachmentFeatureSurface>,
    ),
  ).toBe("");
});

it("shows custom attachment triggers only when the global mode permits reads", () => {
  expect(
    renderToStaticMarkup(
      <AttachmentFeatureProvider mode="enabled">
        <AttachmentFeatureSurface>
          <button type="button">Files</button>
        </AttachmentFeatureSurface>
      </AttachmentFeatureProvider>,
    ),
  ).toContain("Files");
  expect(
    renderToStaticMarkup(
      <AttachmentFeatureProvider mode="read-only">
        <AttachmentFeatureSurface>
          <button type="button">Files</button>
        </AttachmentFeatureSurface>
      </AttachmentFeatureProvider>,
    ),
  ).toContain("Files");
});

it("allows attachment mutations only in enabled mode with domain authorization", () => {
  expect(canMutateAttachments("enabled", true)).toBe(true);
  expect(canMutateAttachments("enabled", false)).toBe(false);
  expect(canMutateAttachments("read-only", true)).toBe(false);
  expect(canMutateAttachments("disabled", true)).toBe(false);
});

it("wraps attachment-only cards and consoles in the global feature surface", () => {
  for (const path of [
    "packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/planning/screen.tsx",
    "packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/quality/screen.tsx",
    "packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/ecommerce/screen.tsx",
    "packages/feature-support/src/components/support-incident-console.tsx",
  ]) {
    expect(readFileSync(path, "utf8"), path).toContain(
      "AttachmentFeatureSurface",
    );
  }
});

it("keeps external legacy file mutations behind enabled-mode guards", () => {
  const branding = readFileSync(
    "packages/feature-brand/src/components/brand-administration.tsx",
    "utf8",
  );
  const support = readFileSync(
    "packages/feature-support/src/components/support-evidence-console.tsx",
    "utf8",
  );
  expect(branding).toContain('if (attachmentMode !== "enabled") return;');
  expect(branding).toContain("canMutateFiles");
  expect(branding).toContain("canReview={canEdit && canReviewPublications}");
  expect(support).toContain("canMutateEvidence");
  expect(support).toContain("{canMutateEvidence &&");
  expect(support).toContain('{attachmentMode !== "disabled" && (');
  expect(support).toContain("Customer-shareable files");
  expect(support).toContain(
    'if (attachmentMode === "disabled" && data.items.length === 0) return null;',
  );
});

it("suppresses staged attachment IDs as soon as the global mode stops allowing mutations", async () => {
  const panel = readFileSync(
    "packages/feature-files/src/components/attachment-panel.tsx",
    "utf8",
  );
  const incident = readFileSync(
    "packages/feature-support/src/components/support-incident-console.tsx",
    "utf8",
  );
  expect(panel).toContain("onChange={mutationsEnabled ? onChange : undefined}");
  expect(panel).toContain(
    "previousMutationsEnabled.current = mutationsEnabled;",
  );
  expect(panel).toContain("if (!wasEnabled || mutationsEnabled) return;");
  expect(panel).toContain("{draftMutable && fieldName && set?.id && (");
  const ts = await import("typescript"), file = ts.createSourceFile("panel.tsx", panel, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  let expression: import("typescript").Expression | undefined;
  const visit = (node: import("typescript").Node) => { if (ts.isVariableDeclaration(node) && ts.isIdentifier(node.name) && node.name.text === "submissionBlocked") expression = node.initializer; ts.forEachChild(node, visit); };
  visit(file);
  expect(expression).toBeDefined();
  const evaluate = new Function("pendingWithdrawal", "pendingBind", "draftMutable", "requiredCount", "readyCount", "activeLinks", "bindable", `return (${expression!.getText(file)});`) as (withdrawal: boolean, bind: boolean, mutable: boolean, required: number, ready: number, links: unknown[], bindable: boolean) => boolean;
  expect(evaluate(false, false, false, 1, 0, [{}], false)).toBe(false);
  expect(evaluate(false, false, true, 1, 0, [{}], false)).toBe(true);
  expect(evaluate(true, false, false, 0, 0, [], true)).toBe(true);
  expect(evaluate(false, true, false, 0, 0, [], true)).toBe(true);
  expect(incident).toContain('attachmentMode === "enabled"');
});

it("does not render a record-document trigger while the global gate is disabled", () => {
  const html = renderToStaticMarkup(
    <AttachmentFeatureProvider mode="disabled">
      <RecordDocumentDrawer
        base="/holdings/acme"
        targetType="finance_bank_statement"
        targetId="statement-1"
        expectedTargetVersion={1}
        title="Statement files"
        purposes={[
          {
            key: "bank_statement",
            label: "Bank statement",
            description: "Private source",
          },
        ]}
      />
    </AttachmentFeatureProvider>,
  );
  expect(html).toBe("");
});
