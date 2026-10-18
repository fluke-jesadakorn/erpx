"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { FormDisclosure } from "@/components/ui/form-disclosure";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import { FeatureWidget, WidgetGrid } from "@/components/shared/features/feature-page";
import { SetupChecklist, type SetupItem } from "@/components/ui/setup-checklist";

import { PositionPreparation } from "@/components/people_payroll/lifecycle-workspace";
import { LevelLeaveDefaults } from "@/components/people_payroll/level-leave-defaults";
import {
  staffLevelTitles,
  type OrganizationNode,
  type OrganizationNodeChange,
  type OrganizationOverview,
  type OrganizationPosition,
  type OrganizationTemplatePosition,
} from "@/lib/organization/organization";
import type { PeopleLifecycleData } from "@/lib/people_payroll/people-lifecycle";
import { Network, UserPlus, Users } from "lucide-react";
import { RecordLink as Link } from "@/components/records/record-link";
import { useSearchParams } from "@/lib/shared/destination-search";
import { useState } from "react";
import { OrganizationChart } from "@/components/organization/organization-chart";
import styles from "@/components/organization/organization.module.css";
import { SpendingReadiness } from "@/components/people_payroll/spending-readiness";
import { SpendingTemplateEditor } from "@/components/people_payroll/spending-template-editor";
import { useStaffCommand } from "@/components/work_projection/use-staff-command";
import { type UiAppearance } from "@/components/ui/foundation";

export function OrganizationWorkspace({ appearance = "legacy",
  base,
  data,
  lifecycle,
  initialPositionId,
  initialView,
}: ({
  base: string;
  data: OrganizationOverview;
  lifecycle?: PeopleLifecycleData;
  initialPositionId?: string;
  initialView?: "people" | "positions" | "templates" | "spending";
}) & { appearance?: UiAppearance }) {
  const [positionSearch, setPositionSearch] = useState("");
  const issues = data.people.filter((p) => p.issues.length),
    activePositions = data.positions.filter((p) => p.active === 1);
  const ceoPosition = activePositions.find((position) => position.is_ceo === 1),
    ceoReady =
      !!ceoPosition &&
      data.people.some(
        (person) =>
          person.position_id === ceoPosition.id &&
          person.level_number === 1 &&
          !person.issues.length,
      );
  const approvalIssues = data.canManage
    ? data.people.filter((person) =>
        person.issues.some((issue) => issue.includes("People Manager access is missing")),
      ).length
    : 0;
  const setupItems: SetupItem[] = [
    {
      id: "staff-levels",
      label: "Six staff levels",
      ready: data.levels.length >= 6,
      owner: "Company HR administrator",
      href: canonicalUiHref(`${base}/people/organization?view=positions`),
      detail:
        data.levels.length >= 6
          ? "The level catalog is installed."
          : "Install the level catalog before creating positions.",
    },
    {
      id: "business-units",
      label: "Branch and department structure",
      ready: data.nodes.some((node) => node.kind === "branch" && node.status === "active"),
      owner: "Company HR administrator",
      href: canonicalUiHref(`${base}/people/organization`),
      detail: data.nodes.some((node) => node.kind === "branch" && node.status === "active")
        ? "At least one active branch is available."
        : "Create and independently approve the first active branch.",
    },
    {
      id: "leadership",
      label: "Active CEO and reporting anchor",
      ready: ceoReady,
      owner: "People administrator",
      href: canonicalUiHref(`${base}/people/organization?view=positions`),
      detail: ceoReady
        ? "An active Level 1 position has an assigned person."
        : "Create a Level 1 CEO position and assign an active person.",
    },
    {
      id: "staff-profiles",
      label: "Staff profiles ready for work",
      ready: issues.length === 0,
      owner: "People manager",
      href: canonicalUiHref(`${base}/people`),
      detail:
        issues.length === 0
          ? "All visible staff have valid placement and reporting lines."
          : `${issues.length} profile${issues.length === 1 ? "" : "s"} need organization setup.`,
    },
    {
      id: "approval-coverage",
      label: "Approval coverage for reporting teams",
      ready: approvalIssues === 0,
      owner: "Organization administrator",
      href: canonicalUiHref(`${base}/settings?section=approvals`),
      detail:
        approvalIssues === 0
          ? "No missing People Manager coverage is reported."
          : `${approvalIssues} reporting team${approvalIssues === 1 ? "" : "s"} need a scoped approver grant.`,
    },
  ];
  return (
    <WidgetGrid pageKey="people-organization">
      <div data-ui-appearance={appearance} className={styles.stats}>
        <div data-ui-appearance={appearance}>
          <Users size={20} />
          <strong data-ui-appearance={appearance}>{data.people.length}</strong>
          <span data-ui-appearance={appearance}>Staff profiles</span>
        </div>
        <div data-ui-appearance={appearance}>
          <Network size={20} />
          <strong data-ui-appearance={appearance}>{activePositions.length}</strong>
          <span data-ui-appearance={appearance}>Active positions</span>
        </div>
        <div data-ui-appearance={appearance}>
          <UserPlus size={20} />
          <strong data-ui-appearance={appearance}>
            {activePositions.filter((p) => !data.people.some((s) => s.position_id === p.id)).length}
          </strong>
          <span data-ui-appearance={appearance}>Vacant positions</span>
        </div>
        <div data-ui-appearance={appearance}>
          <strong data-ui-appearance={appearance}>{issues.length}</strong>
          <span data-ui-appearance={appearance}>Profiles needing setup</span>
        </div>
      </div>
      <div data-ui-appearance={appearance} className={styles.toolbar}>
        <UiActionLink appearance={appearance} variant="secondary" href={canonicalUiHref(`${base}/people`)}>
          People directory
        </UiActionLink>
        <UiActionLink appearance={appearance} variant="secondary" href={canonicalUiHref(`${base}/inbox?tab=teams`)}>
          Organization channels
        </UiActionLink>
      </div>
      <SetupChecklist title="Organization readiness" items={setupItems} />
      {data.canManageUnits && (
        <FeatureWidget id="organization-units" title="Branches & departments">
          <OrganizationUnits appearance={appearance} base={base} data={data} />
        </FeatureWidget>
      )}
      {data.levels.length < 6 && data.canManage && <InstallLevels appearance={appearance} base={base} />}
      {(!initialView || initialView === "people") && (
        <FeatureWidget id="organization-organizationchart" title="Teams">
          <OrganizationChart key={base} base={base} data={data} />
        </FeatureWidget>
      )}
      {(!initialView || initialView === "positions") && lifecycle && (
        <FeatureWidget id="organization-positionpreparation" title="Position preparation">
          <PositionPreparation
            base={base}
            data={{ ...lifecycle, canPrepare: false }}
            surface="people"
            positionId={initialPositionId}
            hrDetails={(id) => {
              const position = data.positions.find((p) => p.id === id);
              return (
                position && (
                  <>
                    <p data-ui-appearance={appearance}>
                      {data.people.filter((p) => p.position_id === id).length} staff · Level{" "}
                      {position.level_number} · Reports to{" "}
                      {data.positions.find((p) => p.id === position.reports_to_position_id)
                        ?.title ?? "Company leadership"}
                    </p>
                    {lifecycle.canPrepare && (
                      <Link
                        href={canonicalUiHref(`${base}/technology?view=organization&position=${encodeURIComponent(id)}`)}
                      >
                        Open position GPO in Technology
                      </Link>
                    )}
                    {data.canManage && (
                      <details data-ui-appearance={appearance}>
                        <summary data-ui-appearance={appearance}>Edit HR position details</summary>
                        <PositionEditor appearance={appearance} base={base} data={data} position={position} />
                      </details>
                    )}
                  </>
                )
              );
            }}
          />
        </FeatureWidget>
      )}
      {(!initialView || initialView === "positions") && (
        <FeatureWidget id="organization-positions" title="Positions and levels">
          <p data-ui-appearance={appearance} className="muted">
            Positions stay when staff move or leave. HR defines the work and reporting line. IT
            prepares resources separately in Technology.
          </p>
          <Field appearance={appearance}>
            Find position
            <TextInput appearance={appearance}
              type="search"
              value={positionSearch}
              onChange={(event) => setPositionSearch(event.target.value)}
              placeholder="Search position title or level"
            />
          </Field>
          {data.positions
            .filter((position) =>
              `${position.title} ${position.level_number}`
                .toLowerCase()
                .includes(positionSearch.toLowerCase()),
            )
            .map((position) => (
              <details data-ui-appearance={appearance}
                key={position.id}
                id={`position-${position.id}`}
                open={initialPositionId === position.id ? true : undefined}
                className={styles.position}
              >
                <summary data-ui-appearance={appearance}>
                  <strong data-ui-appearance={appearance}>{position.title}</strong> · Level {position.level_number} ·{" "}
                  {position.active
                    ? data.people.filter((p) => p.position_id === position.id).length + " staff"
                    : "Archived"}
                </summary>
                <p data-ui-appearance={appearance}>
                  {data.nodes.find((n) => n.id === position.branch_id)?.display_name ?? "Company"} /{" "}
                  {data.nodes.find((n) => n.id === position.department_id)?.display_name ??
                    "All departments"}{" "}
                  · Reports to{" "}
                  {data.positions.find((p) => p.id === position.reports_to_position_id)?.title ??
                    "Company leadership"}
                </p>
                <PositionDefaultPay appearance={appearance} base={base} data={data} position={position} />
                {data.canManage && (
                  <>
                    <UiActionLink appearance={appearance}
                      variant="secondary"
                      href={canonicalUiHref(`${base}/people?new=person&position=${encodeURIComponent(position.id)}`)}
                    >
                      Add staff to this position
                    </UiActionLink>
                    <details data-ui-appearance={appearance}>
                      <summary data-ui-appearance={appearance}>Edit position details</summary>
                      <PositionEditor appearance={appearance} base={base} data={data} position={position} />
                    </details>
                  </>
                )}
              </details>
            ))}
          {data.canManage && (
            <details data-ui-appearance={appearance} className={styles.position}>
              <summary data-ui-appearance={appearance}>Add a position</summary>
              <PositionEditor appearance={appearance} base={base} data={data} />
            </details>
          )}
        </FeatureWidget>
      )}
      <SpendingReadiness appearance={appearance} base={base} />
      {(!initialView || initialView === "spending") &&
        data.canManageSpending &&
        data.spendingTemplate && (
          <FeatureWidget id="organization-spendingtemplateeditor" title="Spending approvals">
            <SpendingTemplateEditor appearance={appearance} base={base} data={data} />
          </FeatureWidget>
        )}
      {(!initialView || initialView === "templates") && data.canManage && (
        <FeatureWidget id="organization-templateeditor" title="Organization templates">
          <TemplateEditor appearance={appearance} base={base} data={data} />
        </FeatureWidget>
      )}
    </WidgetGrid>
  );
}
export function PositionDefaultPay({ appearance = "legacy",
  base,
  data,
  position,
}: ({
  base: string;
  data: OrganizationOverview;
  position: OrganizationPosition;
}) & { appearance?: UiAppearance }) {
  const pay = data.defaults?.pay.find((item) => item.position_id === position.id),
    { send, busy, error } = useStaffCommand(base);
  if (!data.monthlyPayGroups?.length) return null;
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h4 data-ui-appearance={appearance}>Default pay</h4>
      <p data-ui-appearance={appearance}>
        {pay
          ? `${pay.currency} ${(pay.monthly_amount_minor / 100).toLocaleString()} / month · ${pay.pay_group_name} · v${pay.version}`
          : "No default pay yet"}
      </p>
      <p data-ui-appearance={appearance} className="muted">Recommendation only. Payroll uses approved compensation.</p>
      {data.canManage && (
        <FormDisclosure appearance={appearance}>
          <summary data-ui-appearance={appearance}>Edit default pay</summary>
          <form data-ui-appearance={appearance}
            className={appearance === "standard" ? "ui-form-grid" : "business-form"}
            onSubmit={(event) => {
              event.preventDefault();
              const fields = new FormData(event.currentTarget);
              void send(`people/organization/positions/${position.id}/default-pay`, {
                payGroupId: fields.get("payGroupId"),
                currency: fields.get("currency"),
                monthlyAmount: Math.round(Number(fields.get("amount")) * 100),
                expectedVersion: pay?.version ?? 0,
              });
            }}
          >
            <Field appearance={appearance}>
              Monthly pay group
              <Select appearance={appearance} name="payGroupId" required defaultValue={pay?.pay_group_id ?? ""}>
                <option data-ui-appearance={appearance} value="">Choose pay group</option>
                {data.monthlyPayGroups.map((group) => (
                  <option data-ui-appearance={appearance} key={group.id} value={group.id}>
                    {group.name} · {group.currency}
                  </option>
                ))}
              </Select>
            </Field>
            <Field appearance={appearance}>
              Currency
              <Select appearance={appearance} name="currency" defaultValue={pay?.currency ?? "USD"}>
                {[...new Set(data.monthlyPayGroups.map((group) => group.currency))].map(
                  (currency) => (
                    <option data-ui-appearance={appearance} key={currency}>{currency}</option>
                  ),
                )}
              </Select>
            </Field>
            <Field appearance={appearance}>
              Monthly amount
              <TextInput appearance={appearance}
                name="amount"
                type="number"
                min="0"
                step="0.01"
                required
                defaultValue={pay ? pay.monthly_amount_minor / 100 : undefined}
              />
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
              Save new version
            </Button>
            {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
          </form>
        </FormDisclosure>
      )}
    </section>
  );
}
export function OrganizationUnits({ appearance = "legacy", base, data }: ({ base: string; data: OrganizationOverview }) & { appearance?: UiAppearance }) {
  const active = data.nodes.filter((n) => n.kind !== "company" && n.status === "active"),
    suspended = data.nodes.filter((n) => n.kind !== "company" && n.status === "suspended"),
    archived = data.nodes.filter((n) => n.kind !== "company" && n.status === "archived"),
    [documentNodeId, setDocumentNodeId] = useState<string | null>(null),
    documentNode = data.nodes.find((node) => node.id === documentNodeId);
  return (
    <div data-ui-appearance={appearance} className="content-stack">
      <p data-ui-appearance={appearance} className="muted">
        Create or change a business unit through an independent review. Active staff, roles,
        positions, child units, or warehouses must be moved before a unit can be suspended or
        archived. Relocating a department moves its current staff, positions and scoped assignments
        with it.
      </p>
      <details data-ui-appearance={appearance} className={styles.position}>
        <summary data-ui-appearance={appearance}>
          <strong data-ui-appearance={appearance}>Propose a new branch or department</strong>
        </summary>
        <NodeChangeForm appearance={appearance} base={base} data={data} />
      </details>
      <div data-ui-appearance={appearance} className={styles.unitGrid}>
        {[...active, ...suspended, ...archived].map((node) => (
          <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"} key={node.id}>
            <h3 data-ui-appearance={appearance}>{node.display_name}</h3>
            <p data-ui-appearance={appearance}>
              {node.kind} · {node.status} · revision {node.hierarchy_revision}
            </p>
            {node.kind === "department" && (
              <p data-ui-appearance={appearance} className="muted">
                {data.nodes.find((n) => n.id === node.branch_id)?.display_name ??
                  "Unavailable branch"}
              </p>
            )}
            <AttachmentFeatureSurface>
              <Button appearance={appearance}
                variant="secondary"
                type="button"
                onClick={() =>
                  setDocumentNodeId((current) => (current === node.id ? null : node.id))
                }
              >
                {documentNodeId === node.id ? "Close official documents" : "Official documents"}
              </Button>
            </AttachmentFeatureSurface>
            <details data-ui-appearance={appearance}>
              <summary data-ui-appearance={appearance}>Propose a change</summary>
              <NodeChangeForm appearance={appearance} key={node.hierarchy_revision} base={base} data={data} node={node} />
            </details>
          </section>
        ))}
      </div>
      <AttachmentFeatureSurface>
        {documentNode && (
          <RecordDocumentPanel appearance={appearance}
            key={`${documentNode.id}:${documentNode.hierarchy_revision}`}
            base={base}
            targetType="organization_node"
            targetId={documentNode.id}
            expectedTargetVersion={documentNode.hierarchy_revision}
            title={`${documentNode.display_name} official documents`}
            purposes={[
              {
                key: "registration",
                label: "Registration",
                description:
                  "Attach the official registration document for this organization unit.",
              },
              {
                key: "license",
                label: "License",
                description:
                  "Attach the current license or regulated operating document for this organization unit.",
              },
              ...(documentNode.kind === "branch"
                ? [
                    {
                      key: "branch_permit",
                      label: "Branch permit",
                      description: "Attach the permit authorizing this branch or establishment.",
                    },
                  ]
                : []),
            ]}
          />
        )}
      </AttachmentFeatureSurface>
      <OrganizationChangeQueue appearance={appearance} base={base} data={data} />
    </div>
  );
}
function NodeChangeForm({ appearance = "legacy",
  base,
  data,
  node,
  change,
}: ({
  base: string;
  data: OrganizationOverview;
  node?: OrganizationNode;
  change?: OrganizationNodeChange;
}) & { appearance?: UiAppearance }) {
  const target = node ?? data.nodes.find((n) => n.id === change?.target_node_id),
    initialKind = change?.node_kind ?? (target?.kind === "department" ? "department" : "branch"),
    initialAction =
      change?.action ??
      (target ? (target.status === "active" ? "rename" : "reactivate") : "create");
  const { send, busy, error } = useStaffCommand(base),
    [kind, setKind] = useState<"branch" | "department">(initialKind as "branch" | "department"),
    [action, setAction] = useState<
      "create" | "rename" | "suspend" | "reactivate" | "archive" | "relocate"
    >(initialAction);
  const branchId =
    action === "relocate"
      ? (change?.branch_id ?? "")
      : (change?.branch_id ?? target?.branch_id ?? "");
  return (
    <form data-ui-appearance={appearance}
      className={appearance === "standard" ? "ui-form-grid" : "business-form"}
      onSubmit={async (e) => {
        e.preventDefault();
        const form = e.currentTarget,
          f = new FormData(form, (e.nativeEvent as SubmitEvent).submitter),
          mode = f.get("mode") === "submit" ? "submit" : "draft";
        const saved = await send(
          `people/organization/changes/${change ? `${change.id}/` : ""}${mode}`,
          {
            action,
            targetNodeId: change?.target_node_id ?? target?.id ?? null,
            kind: target ? (target.kind as "branch" | "department") : kind,
            slug: action === "create" ? f.get("slug") : null,
            displayName: f.get("displayName"),
            branchId: target
              ? action === "relocate"
                ? f.get("branchId")
                : branchId || null
              : kind === "department"
                ? f.get("branchId") || null
                : null,
            expectedNodeVersion:
              change?.expected_node_version ?? target?.hierarchy_revision ?? null,
            expectedVersion: change?.version ?? 0,
            reason: f.get("reason"),
          },
        );
        if (saved && !change) {
          form.reset();
          setKind(initialKind as "branch" | "department");
        }
      }}
    >
      {!target && (
        <Field appearance={appearance}>
          Unit type
          <Select appearance={appearance} value={kind} onChange={(e) => setKind(e.target.value as "branch" | "department")}>
            <option data-ui-appearance={appearance} value="branch">Branch</option>
            <option data-ui-appearance={appearance} value="department">Department</option>
          </Select>
        </Field>
      )}
      {target && (
        <Field appearance={appearance}>
          Change
          <Select appearance={appearance}
            value={action}
            disabled={Boolean(change)}
            onChange={(e) => setAction(e.target.value as typeof action)}
          >
            <option data-ui-appearance={appearance} value="rename">Rename</option>
            {target.status === "active" ? (
              <>
                <option data-ui-appearance={appearance} value="suspend">Suspend</option>
                <option data-ui-appearance={appearance} value="archive">Archive</option>
                {target.kind === "department" && (
                  <option data-ui-appearance={appearance} value="relocate">Relocate to another branch</option>
                )}
              </>
            ) : (
              <option data-ui-appearance={appearance} value="reactivate">Reactivate</option>
            )}
          </Select>
        </Field>
      )}
      {!target && (
        <Field appearance={appearance}>
          Stable URL code
          <TextInput appearance={appearance}
            name="slug"
            required
            pattern="[a-z0-9]+(?:-[a-z0-9]+)*"
            maxLength={80}
            placeholder="chiang-mai"
            defaultValue={change?.slug ?? ""}
          />
        </Field>
      )}
      <Field appearance={appearance}>
        Display name
        <TextInput appearance={appearance}
          name="displayName"
          required
          minLength={2}
          maxLength={120}
          defaultValue={change?.display_name ?? target?.display_name}
        />
      </Field>
      {((!target && kind === "department") || (target && action === "relocate")) && (
        <Field appearance={appearance}>
          {action === "relocate" ? "Destination branch" : "Branch"}
          <Select appearance={appearance} name="branchId" required defaultValue={branchId}>
            <option data-ui-appearance={appearance} value="">Choose an active branch</option>
            {data.nodes
              .filter(
                (n) =>
                  n.kind === "branch" &&
                  n.status === "active" &&
                  n.id !== (action === "relocate" ? target?.branch_id : null),
              )
              .map((n) => (
                <option data-ui-appearance={appearance} key={n.id} value={n.id}>
                  {n.display_name}
                </option>
              ))}
          </Select>
        </Field>
      )}
      <Field appearance={appearance}>
        Business reason
        <TextInput appearance={appearance}
          name="reason"
          required
          minLength={3}
          maxLength={1000}
          defaultValue={change?.reason}
        />
      </Field>
      <div data-ui-appearance={appearance} className={styles.formActions}>
        <Button appearance={appearance} clientManaged type="submit" variant="secondary" name="mode" value="draft" disabled={busy}>
          Save draft
        </Button>
        <Button appearance={appearance} clientManaged type="submit" variant="primary" name="mode" value="submit" disabled={busy}>
          Submit for review
        </Button>
      </div>
      {error && (
        <p data-ui-appearance={appearance} role="alert" className="error">
          {error}
        </p>
      )}
    </form>
  );
}
export function OrganizationChangeQueue({ appearance = "legacy",
  base,
  data,
}: ({
  base: string;
  data: OrganizationOverview;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    [reason, setReason] = useState<Record<string, string>>({}),
    [proof, setProof] = useState<Record<string, AttachmentPanelChange>>({});
  const queue = data.changeQueue;
  const pageLink = (page: number) =>
    canonicalUiHref(`${base}/people/organization?${new URLSearchParams({ changeState: queue.state, changeQuery: queue.query, changePage: String(page) })}#organization-change-queue`);
  const timestamp = (value: number) => <TimeText appearance={appearance} value={value} />;
  return (
    <section data-ui-appearance={appearance}
      id="organization-change-queue"
      className={styles.changeRequests}
      aria-label="Organization change requests"
    >
      <h3 data-ui-appearance={appearance}>Organization change requests</h3>
      {queue.recordId && (
        <p data-ui-appearance={appearance}>
          Showing the linked request.{" "}
          <Link href={canonicalUiHref(`${base}/people/organization#organization-change-queue`)}>
            View all open requests
          </Link>
        </p>
      )}
      <form data-ui-appearance={appearance}
        className={appearance === "standard" ? "ui-form-grid" : "business-form"}
        action={canonicalUiHref(`${base}/people/organization#organization-change-queue`)}
      >
        <Field appearance={appearance}>
          Change status
          <Select appearance={appearance} name="changeState" defaultValue={queue.state} key={queue.state}>
            <option data-ui-appearance={appearance} value="open">Open requests</option>
            <option data-ui-appearance={appearance} value="all">All requests</option>
            <option data-ui-appearance={appearance} value="draft">Draft</option>
            <option data-ui-appearance={appearance} value="submitted">Awaiting review</option>
            <option data-ui-appearance={appearance} value="rejected">Returned for correction</option>
            <option data-ui-appearance={appearance} value="applied">Applied</option>
            <option data-ui-appearance={appearance} value="cancelled">Cancelled</option>
          </Select>
        </Field>
        <Field appearance={appearance}>
          Find organization changes
          <TextInput appearance={appearance}
            type="search"
            name="changeQuery"
            maxLength={120}
            defaultValue={queue.query}
            key={queue.query}
          />
        </Field>
        <Button appearance={appearance} type="submit" variant="secondary">
          Filter changes
        </Button>
      </form>
      {!data.nodeChanges.length && (
        <p data-ui-appearance={appearance} className="muted">No organization changes match these filters.</p>
      )}
      <div data-ui-appearance={appearance} className={styles.changeQueue}>
        {data.nodeChanges.map((change) => (
          <article data-ui-appearance={appearance}
            className={`card ${styles.changeRequests}`}
            id={`organization-change-${change.id}`}
            key={change.id}
            aria-label={`${change.action} ${change.node_kind}: ${change.display_name}`}
          >
            <strong data-ui-appearance={appearance}>
              {change.action} {change.node_kind}: {change.display_name}
            </strong>
            <p data-ui-appearance={appearance}>
              {change.state} · revision {change.version}
            </p>
            <p data-ui-appearance={appearance}>{change.reason}</p>
            {change.review_reason && <p data-ui-appearance={appearance} className="muted">Review: {change.review_reason}</p>}
            {change.state === "submitted" && !change.canReview && (
              <p data-ui-appearance={appearance} className="notice">
                Awaiting an independent Company HR administrator. You cannot review your own
                request.
              </p>
            )}
            {change.canReview && (
              <>
                <Field appearance={appearance}>
                  Review reason
                  <TextInput appearance={appearance}
                    value={reason[change.id] ?? ""}
                    onChange={(e) => setReason((r) => ({ ...r, [change.id]: e.target.value }))}
                    minLength={3}
                    maxLength={1000}
                  />
                </Field>
                <AttachmentPanel appearance={appearance}
                  base={base}
                  targetType="organization_change"
                  targetId={change.id}
                  expectedTargetVersion={change.version}
                  componentKind="action"
                  componentRef="organization-change-review"
                  purposeKey="org_change_proof"
                  title="Organization-change proof"
                  description="Attach registration, permit, resolution, or other proof reviewed for this exact organization change."
                  visibility="Restricted to Company Organization administrators eligible to review this change."
                  onChange={(value) => setProof((current) => ({ ...current, [change.id]: value }))}
                />
                <div data-ui-appearance={appearance} className={styles.formActions}>
                  <Button appearance={appearance}
                    type="submit"
                    variant="primary"
                    disabled={busy || (reason[change.id]?.trim().length ?? 0) < 3}
                    onClick={() =>
                      void send(`people/organization/changes/${change.id}/review`, {
                        expectedVersion: change.version,
                        decision: "approve",
                        reason: reason[change.id],
                        attachmentSetIds:
                          proof[change.id]?.setId &&
                          proof[change.id].readyCount > 0 &&
                          proof[change.id].bindable
                            ? [proof[change.id].setId]
                            : [],
                      })
                    }
                  >
                    Approve and apply
                  </Button>
                  <Button appearance={appearance}
                    type="submit"
                    variant="secondary"
                    disabled={busy || (reason[change.id]?.trim().length ?? 0) < 3}
                    onClick={() =>
                      void send(`people/organization/changes/${change.id}/review`, {
                        expectedVersion: change.version,
                        decision: "reject",
                        reason: reason[change.id],
                        attachmentSetIds:
                          proof[change.id]?.setId &&
                          proof[change.id].readyCount > 0 &&
                          proof[change.id].bindable
                            ? [proof[change.id].setId]
                            : [],
                      })
                    }
                  >
                    Return for correction
                  </Button>
                </div>
              </>
            )}
            {change.requested_by === data.actorId &&
              ["draft", "rejected"].includes(change.state) && (
                <details data-ui-appearance={appearance}>
                  <summary data-ui-appearance={appearance}>Correct and resubmit</summary>
                  <NodeChangeForm appearance={appearance} key={change.version} base={base} data={data} change={change} />
                </details>
              )}
            {change.requested_by === data.actorId &&
              ["draft", "submitted", "rejected"].includes(change.state) && (
                <Button appearance={appearance}
                  type="submit"
                  variant="secondary"
                  disabled={busy}
                  onClick={() =>
                    void send(`people/organization/changes/${change.id}/cancel`, {
                      expectedVersion: change.version,
                    })
                  }
                >
                  Cancel request
                </Button>
              )}
            <details data-ui-appearance={appearance}>
              <summary data-ui-appearance={appearance}>Decision history</summary>
              <ol data-ui-appearance={appearance}>
                {change.events?.map((event) => (
                  <li data-ui-appearance={appearance} key={event.version}>
                    <p data-ui-appearance={appearance}>
                      Revision {event.version} · {event.action} · {event.actor_name} ·{" "}
                      {timestamp(event.created_at)}
                    </p>
                    {event.reason && <p data-ui-appearance={appearance}>{event.reason}</p>}
                  </li>
                ))}
              </ol>
            </details>
          </article>
        ))}
      </div>
      <nav data-ui-appearance={appearance} aria-label="Organization change pagination" className={styles.formActions}>
        <span data-ui-appearance={appearance}>
          {queue.total} matching requests · Page {queue.page} of {queue.pages}
        </span>
        {queue.page > 1 && <Link href={pageLink(queue.page - 1)}>Previous changes</Link>}
        {queue.page < queue.pages && <Link href={pageLink(queue.page + 1)}>Next changes</Link>}
      </nav>
      {error && (
        <p data-ui-appearance={appearance} role="alert" className="error">
          {error}
        </p>
      )}
    </section>
  );
}
export function InstallLevels({ appearance = "legacy", base }: ({ base: string }) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base);
  return (
    <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
      <h2 data-ui-appearance={appearance}>Set up six staff levels</h2>
      <p data-ui-appearance={appearance}>
        Install the numbered catalog, then explicitly assign each staff member. Existing levels and
        history are retained.
      </p>
      <Button appearance={appearance}
        type="submit"
        variant="primary"
        disabled={busy}
        onClick={() => void send("people/organization/levels", {})}
      >
        Install staff levels
      </Button>
      {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
    </section>
  );
}
export function PositionEditor({ appearance = "legacy",
  base,
  data,
  position,
}: ({
  base: string;
  data: OrganizationOverview;
  position?: OrganizationPosition;
}) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base),
    [branch, setBranch] = useState(position?.branch_id ?? ""),
    [ceo, setCeo] = useState(Boolean(position?.is_ceo));
  return (
    <form data-ui-appearance={appearance}
      className={appearance === "standard" ? "ui-form-grid" : "business-form"}
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        await send(`people/organization/positions${position ? `/${position.id}` : ""}`, {
          title: f.get("title"),
          branchId: branch || null,
          departmentId: f.get("department") || null,
          levelId: f.get("level"),
          reportsToPositionId: ceo ? null : f.get("manager") || null,
          isCeo: ceo,
          rosterRequired: f.get("rosterRequired") === "on",
          active: f.get("active") === "on",
          expectedVersion: position?.version ?? 0,
          requirements: f.get("requirements"),
          reason: f.get("reason"),
        });
      }}
    >
      <Field appearance={appearance}>
        Work requirements
        <TextArea appearance={appearance}
          name="requirements"
          defaultValue={position?.requirements ?? ""}
          maxLength={3000}
          placeholder="Accounting system, branch printer, laptop…"
        />
      </Field>
      <Field appearance={appearance}>
        Position title
        <TextInput appearance={appearance}
          name="title"
          defaultValue={position?.title}
          required
          minLength={2}
          maxLength={160}
        />
      </Field>
      <Field appearance={appearance}>
        Branch
        <Select appearance={appearance} value={branch} onChange={(e) => setBranch(e.target.value)}>
          <option data-ui-appearance={appearance} value="">Company-wide</option>
          {data.nodes
            .filter((n) => n.kind === "branch")
            .map((n) => (
              <option data-ui-appearance={appearance} key={n.id} value={n.id}>
                {n.display_name}
              </option>
            ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Department
        <Select appearance={appearance} name="department" key={branch} defaultValue={position?.department_id ?? ""}>
          <option data-ui-appearance={appearance} value="">All departments</option>
          {data.nodes
            .filter((n) => n.kind === "department" && (n.branch_id ?? "") === branch)
            .map((n) => (
              <option data-ui-appearance={appearance} key={n.id} value={n.id}>
                {n.display_name}
              </option>
            ))}
        </Select>
      </Field>
      <Field appearance={appearance}>
        Default level
        <Select appearance={appearance} name="level" required defaultValue={position?.level_id ?? ""}>
          <option data-ui-appearance={appearance} value="">Choose a level</option>
          {data.levels.map((l) => (
            <option data-ui-appearance={appearance} key={l.id} value={l.id}>
              Level {l.level_number} · {l.name}
            </option>
          ))}
        </Select>
      </Field>
      <Field appearance={appearance} orientation="inline">
        <TextInput appearance={appearance} type="checkbox" checked={ceo} onChange={(e) => setCeo(e.target.checked)} />{" "}
        Designated CEO position (Level 1)
      </Field>
      {!ceo && (
        <Field appearance={appearance}>
          Expected reporting position
          <Select appearance={appearance} name="manager" required defaultValue={position?.reports_to_position_id ?? ""}>
            <option data-ui-appearance={appearance} value="">Choose a reporting position</option>
            {data.positions
              .filter((p) => p.active === 1 && p.id !== position?.id)
              .map((p) => (
                <option data-ui-appearance={appearance} key={p.id} value={p.id}>
                  {p.title} · Level {p.level_number}
                </option>
              ))}
          </Select>
        </Field>
      )}
      <Field appearance={appearance} orientation="inline">
        <TextInput appearance={appearance} type="checkbox" name="active" defaultChecked={position?.active !== 0} /> Active
        position
      </Field>
      <Field appearance={appearance} orientation="inline">
        <TextInput appearance={appearance}
          type="checkbox"
          name="rosterRequired"
          defaultChecked={position?.roster_required === 1}
        />{" "}
        Staff in this position must have a weekly roster shift
      </Field>
      <Field appearance={appearance}>
        Reason
        <TextInput appearance={appearance}
          name="reason"
          required
          minLength={3}
          maxLength={1000}
          placeholder="Describe this organization change"
        />
      </Field>
      <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy || !data.levels.length}>
        {position ? "Save position" : "Create position"}
      </Button>
      {error && <p data-ui-appearance={appearance} role="alert">{error}</p>}
    </form>
  );
}
export function TemplateEditor({ appearance = "legacy", base, data }: ({ base: string; data: OrganizationOverview }) & { appearance?: UiAppearance }) {
  const { send, busy, error } = useStaffCommand(base);
  const starter: OrganizationTemplatePosition[] = staffLevelTitles.map((title, i) => ({
    key: `level-${i + 1}`,
    title,
    levelNumber: i + 1,
    reportsToKey: i ? `level-${i}` : null,
    isCeo: i === 0,
    rosterRequired: false,
  }));
  const [draft, setDraft] = useState(starter),
    [name, setName] = useState("Standard organization"),
    [selected, setSelected] = useState(""),
    [branch, setBranch] = useState("");
  const template = data.templates.find((t) => t.id === selected);
  return (
    <div data-ui-appearance={appearance} className="content-stack">
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h2 data-ui-appearance={appearance}>Create a reusable organization template</h2>
        <p data-ui-appearance={appearance} className="muted">
          Review the positions and reporting structure. Templates create positions; they do not
          assign people or grant roles.
        </p>
        <Field appearance={appearance}>
          Template name
          <TextInput appearance={appearance} value={name} onChange={(e) => setName(e.target.value)} maxLength={120} />
        </Field>
        <div data-ui-appearance={appearance} className={styles.templateGrid}>
          {draft.map((p, i) => (
            <div data-ui-appearance={appearance} key={p.key}>
              <Field appearance={appearance}>
                Default level
                <Select appearance={appearance}
                  value={p.levelNumber}
                  disabled={p.isCeo}
                  onChange={(e) =>
                    setDraft((d) =>
                      d.map((v, j) =>
                        j === i
                          ? { ...v, levelNumber: Number(e.target.value), reportsToKey: "level-1" }
                          : v,
                      ),
                    )
                  }
                >
                  {staffLevelTitles.map((t, j) => (
                    <option data-ui-appearance={appearance} key={j} value={j + 1}>
                      Level {j + 1} · {t}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field appearance={appearance}>
                Position title
                <TextInput appearance={appearance}
                  value={p.title}
                  onChange={(e) =>
                    setDraft((d) =>
                      d.map((v, j) => (j === i ? { ...v, title: e.target.value } : v)),
                    )
                  }
                />
              </Field>
              <Field appearance={appearance}>
                Reports to
                <Select appearance={appearance}
                  disabled={p.isCeo}
                  value={p.reportsToKey ?? ""}
                  onChange={(e) =>
                    setDraft((d) =>
                      d.map((v, j) =>
                        j === i ? { ...v, reportsToKey: e.target.value || null } : v,
                      ),
                    )
                  }
                >
                  <option data-ui-appearance={appearance} value="">No superior (CEO)</option>
                  {draft
                    .filter((v) => v.levelNumber < p.levelNumber)
                    .map((v) => (
                      <option data-ui-appearance={appearance} key={v.key} value={v.key}>
                        {v.title}
                      </option>
                    ))}
                </Select>
              </Field>
              <Field appearance={appearance} orientation="inline">
                <TextInput appearance={appearance}
                  type="checkbox"
                  checked={!!p.rosterRequired}
                  onChange={(e) =>
                    setDraft((d) =>
                      d.map((v, j) => (j === i ? { ...v, rosterRequired: e.target.checked } : v)),
                    )
                  }
                />{" "}
                Weekly roster required
              </Field>
              {!p.isCeo && (
                <Button appearance={appearance}
                  type="submit"
                  variant="secondary"
                  onClick={() => setDraft((d) => d.filter((_, j) => i !== j))}
                >
                  Remove position
                </Button>
              )}
            </div>
          ))}
        </div>
        <Button appearance={appearance}
          type="submit"
          variant="secondary"
          onClick={() =>
            setDraft((d) => [
              ...d,
              {
                key: crypto.randomUUID(),
                title: "New position",
                levelNumber: 6,
                reportsToKey: "level-1",
                isCeo: false,
                rosterRequired: false,
              },
            ])
          }
        >
          Add template position
        </Button>
        <Button appearance={appearance}
          type="submit"
          variant="primary"
          disabled={busy || name.trim().length < 2}
          onClick={() =>
            void send("people/organization/templates", {
              name,
              positions: draft,
              expectedVersion: 0,
            })
          }
        >
          Save template
        </Button>
      </section>
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h2 data-ui-appearance={appearance}>Apply a reviewed template</h2>
        <Field appearance={appearance}>
          Saved template
          <Select appearance={appearance} value={selected} onChange={(e) => setSelected(e.target.value)}>
            <option data-ui-appearance={appearance} value="">Choose a template</option>
            {data.templates.map((t) => (
              <option data-ui-appearance={appearance} key={t.id} value={t.id}>
                {t.name} · revision {t.version}
              </option>
            ))}
          </Select>
        </Field>
        {template && (
          <form data-ui-appearance={appearance}
            className={appearance === "standard" ? "ui-form-grid" : "business-form"}
            onSubmit={async (e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget);
              await send(`people/organization/templates/${template.id}/apply`, {
                expectedVersion: template.version,
                branchId: branch || null,
                departmentId: f.get("department") || null,
                requirements: f.get("requirements"),
                reason: f.get("reason"),
              });
            }}
          >
            <Field appearance={appearance}>
              Target branch
              <Select appearance={appearance} value={branch} onChange={(e) => setBranch(e.target.value)}>
                <option data-ui-appearance={appearance} value="">Company-wide</option>
                {data.nodes
                  .filter((n) => n.kind === "branch")
                  .map((n) => (
                    <option data-ui-appearance={appearance} key={n.id} value={n.id}>
                      {n.display_name}
                    </option>
                  ))}
              </Select>
            </Field>
            <Field appearance={appearance}>
              Target department
              <Select appearance={appearance} key={branch} name="department">
                <option data-ui-appearance={appearance} value="">All departments</option>
                {data.nodes
                  .filter((n) => n.kind === "department" && (n.branch_id ?? "") === branch)
                  .map((n) => (
                    <option data-ui-appearance={appearance} key={n.id} value={n.id}>
                      {n.display_name}
                    </option>
                  ))}
              </Select>
            </Field>
            <ul data-ui-appearance={appearance}>
              {template.positions.map((p) => (
                <li data-ui-appearance={appearance} key={p.key}>
                  {p.title} · Level {p.levelNumber} →{" "}
                  {template.positions.find((v) => v.key === p.reportsToKey)?.title ??
                    "Company leadership"}
                </li>
              ))}
            </ul>
            <p data-ui-appearance={appearance}>
              The existing CEO position is reused when present. Other positions are added as
              vacancies.
            </p>
            <Field appearance={appearance}>
              Reason
              <TextInput appearance={appearance} name="reason" required minLength={3} maxLength={1000} />
            </Field>
            <Field appearance={appearance} orientation="inline">
              <TextInput appearance={appearance} type="checkbox" required /> I have reviewed the positions and target
              placement.
            </Field>
            <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={busy}>
              Apply reviewed template
            </Button>
          </form>
        )}
      </section>
      {error && (
        <p data-ui-appearance={appearance} role="alert" className="error">
          {error}
        </p>
      )}
    </div>
  );
}

/** Advanced HR organization controls shared by the consolidated Company Settings panel. */
export function OrganizationAdvancedTools({ appearance = "legacy",
  base,
  data,
  lifecycle,
}: ({
  base: string;
  data: OrganizationOverview;
  lifecycle?: PeopleLifecycleData | null;
}) & { appearance?: UiAppearance }) {
  const section = useSearchParams().get("section");
  const currentPositions = data.positions.filter((position) => position.active === 1);
  const vacancies = currentPositions.filter(
    (position) => !data.people.some((person) => person.position_id === position.id),
  );
  const issues = data.people.filter((person) => person.issues.length);
  return (
    <div data-ui-appearance={appearance} className="content-stack" aria-label="Advanced organization tools">
      <section data-ui-appearance={appearance} className={appearance === "standard" ? "ui-panel" : "card"}>
        <h3 data-ui-appearance={appearance}>Organization readiness</h3>
        <p data-ui-appearance={appearance}>
          {data.people.length} people · {currentPositions.length} active positions ·{" "}
          {vacancies.length} vacant positions · {issues.length} profiles need review
        </p>
        {issues.length > 0 && (
          <Link href={canonicalUiHref(`${base}/people?tab=people`)}>Review people and reporting warnings</Link>
        )}
      </section>
      <details data-ui-appearance={appearance}
        id="people-company-levels"
        className={styles.position}
        open={section === "levels" || data.levels.length < 6}
      >
        <summary data-ui-appearance={appearance}>
          <strong data-ui-appearance={appearance}>Company levels and leave defaults</strong> · {data.levels.length} levels
        </summary>
        <ol data-ui-appearance={appearance}>
          {data.levels.map((level) => (
            <li data-ui-appearance={appearance} key={level.id}>
              Level {level.level_number} · {level.name}
            </li>
          ))}
        </ol>
        {data.levels.length < 6 && data.canManage && <InstallLevels appearance={appearance} base={base} />}
        {data.canManage && data.levels.length > 0 && (
          <LevelLeaveDefaults appearance={appearance}
            base={base}
            levels={data.levels}
            defaults={data.defaults?.leave ?? []}
          />
        )}
      </details>
      {data.canManage && (
        <details data-ui-appearance={appearance}
          id="people-company-templates"
          className={styles.position}
          open={section === "templates"}
        >
          <summary data-ui-appearance={appearance}>
            <strong data-ui-appearance={appearance}>Organization templates</strong> · {data.templates.length} saved
          </summary>
          <TemplateEditor appearance={appearance} base={base} data={data} />
        </details>
      )}
      <details data-ui-appearance={appearance}
        id="people-company-spending"
        className={styles.position}
        open={section === "spending"}
      >
        <summary data-ui-appearance={appearance}>
          <strong data-ui-appearance={appearance}>Spending approval readiness and exceptions</strong>
        </summary>
        <SpendingReadiness appearance={appearance} base={base} />
        {data.canManageSpending && data.spendingTemplate && (
          <SpendingTemplateEditor appearance={appearance} base={base} data={data} />
        )}
      </details>
      {lifecycle && (
        <details data-ui-appearance={appearance}
          id="people-company-it-preparation"
          className={styles.position}
          open={section === "preparation"}
        >
          <summary data-ui-appearance={appearance}>
            <strong data-ui-appearance={appearance}>Position preparation and IT handoff</strong>
          </summary>
          <PositionPreparation
            base={base}
            data={{ ...lifecycle, canPrepare: false }}
            surface="people"
            hrDetails={(id) => {
              const position = data.positions.find((item) => item.id === id);
              return (
                position && (
                  <>
                    <p data-ui-appearance={appearance}>
                      {data.people.filter((person) => person.position_id === id).length} people ·
                      Level {position.level_number}
                    </p>
                    <Link href={canonicalUiHref(`${base}/people?position=${encodeURIComponent(id)}`)}>
                      Open HR position
                    </Link>
                    <Link
                      href={canonicalUiHref(`${base}/technology?view=organization&position=${encodeURIComponent(id)}`)}
                    >
                      Open IT policies and delivery
                    </Link>
                  </>
                )
              );
            }}
          />
        </details>
      )}
    </div>
  );
}
