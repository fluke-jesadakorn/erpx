import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { accessPage } from "@/server/page-error";
import { ApiError } from "@/server/http";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { businessReadiness } from "@/server/business-readiness";
import { CompanyBusinessAcceptance } from "@/components/settings/company-business-acceptance";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
import { PageContent, Card, FilterBar } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
const labels = [
  "A · Acceptance and routing",
  "B · Finance and accounting",
  "C · Procurement and stock",
  "D · Customer service and Portal",
  "E · Projects, Field Service and Legal",
  "F · POS, Hotel and E-commerce",
  "G · Planning and Manufacturing",
  "H · People and Payroll",
  "I · Platform and remaining capabilities",
];
export default async function BusinessReadinessPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ package?: string; scenario?: string; q?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  let data: Awaited<ReturnType<typeof businessReadiness>>;
  try {
    data = await businessReadiness(actor);
  } catch (error) {
    if (error instanceof ApiError && error.status === 403)
      return accessPage("permission", actor.holdingSlug);
    throw error;
  }
  const raw = await searchParams,
    query = {
      package: typeof raw.package === "string" ? raw.package : "",
      scenario: typeof raw.scenario === "string" ? raw.scenario : "",
      q: typeof raw.q === "string" ? raw.q : "",
    };
  const rows = data.capabilities.filter(
    (row) =>
      (!query.package || row.package === query.package) &&
      (!query.scenario || row.scenarios.includes(query.scenario)) &&
      (!query.q ||
        `${row.id} ${row.label} ${row.owner}`.toLowerCase().includes(query.q.toLowerCase())),
  );
  const pending = data.capabilities.reduce(
    (sum, row) => sum + Object.values(row.gates).filter((gate) => gate.state === "pending").length,
    0,
  );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/settings`}>← Settings</Link>
      <header>
        <p className="eyebrow">Thai hotel pilot</p>
        <h1>Business readiness</h1>
        <p>
          One hotel and restaurant, purchasing, HR, Thai payroll and assisted daily work. Release
          acceptance requires all criteria below.
        </p>
      </header>
      <CompanyBusinessAcceptance base={base} initial={data.companyAcceptance} />
      <Card as="section" appearance="standard" className="ui-panel" id="release-acceptance" aria-label="Release acceptance">
        <h2>Release acceptance · {data.releaseSignOff ? "Passed" : "Pending"}</h2>
        <p>
          Company setup, test observations and release acceptance are separate decisions. The
          ten-working-day checkpoint is a readiness review, not a launch date.
        </p>
        <ul>
          {data.releaseAcceptance.blockers.map((blocker) => (
            <li key={blocker}>{blocker}</li>
          ))}
        </ul>
        <TableFrame appearance="standard" label="Records" className="business-table-wrap">
          <Table className="business-table">
            <thead>
              <tr>
                <th>Release criterion</th>
                <th>Responsible reviewer</th>
                <th>Acceptance</th>
              </tr>
            </thead>
            <tbody>
              {data.releaseAcceptance.gates.map((gate) => (
                <tr key={gate.id}>
                  <td>{gate.label}</td>
                  <td>{gate.owner}</td>
                  <td>
                    {gate.state}
                    {gate.reason && <p>{gate.reason}</p>}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
        <details>
          <summary>Release evidence and versions</summary>
          <p>Scope: {data.releaseAcceptance.inputs.scopeVersion}</p>
          <p>Build: {data.releaseAcceptance.inputs.softwareVersion}</p>
          <p>Configuration revision: {data.releaseAcceptance.inputs.configurationRevision}</p>
          {data.releaseAcceptance.release && (
            <>
              <p>Release: {data.releaseAcceptance.release.id}</p>
              <p>Evidence SHA-256: {data.releaseAcceptance.release.evidenceSha256}</p>
              <RecordDocumentPanel appearance="standard"
                base={base}
                targetType="company_release_acceptance"
                targetId={data.releaseAcceptance.release.id}
                expectedTargetVersion={data.releaseAcceptance.release.version}
                title="Sanitized release evidence"
                purposes={[
                  {
                    key: "sanitized_release_evidence",
                    label: "Sanitized release evidence",
                    description:
                      "Attach only a reviewed artifact with passwords, keys, secrets, configuration exports, and database contents removed.",
                    visibility:
                      "Restricted to Company release reviewers. A server-recorded sanitization attestation is required before binding.",
                  },
                ]}
                editable={hasPermission(actor, "organization.manage")}
              />
            </>
          )}
        </details>
      </Card>
      <details open={Boolean(query.package || query.scenario || query.q)}>
        <summary>
          Reference inventory · {data.capabilities.length} capabilities · {data.scenarios.length}{" "}
          scenarios
        </summary>
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Workspace capability inventory</h2>
          <p>
            {pending} inventory gates pending. This inventory includes future modules; it does not
            expand the hotel release scope. Implementation labels and passing subsets are not
            business sign-off.
          </p>
        </Card>
        <FilterBar className="ui-filter-bar">
          <Field appearance="standard">
            Work package
            <Select appearance="standard" name="package" defaultValue={query.package ?? ""}>
              <option value="">All packages</option>
              {labels.map((label, index) => (
                <option key={label} value={String.fromCharCode(65 + index)}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Scenario
            <Select appearance="standard" name="scenario" defaultValue={query.scenario ?? ""}>
              <option value="">All scenarios</option>
              {data.scenarios.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.id} · {s.capabilityIds.length} capabilities · {s.observationIds.length} scoped
                  observations
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Capability or owner
            <TextInput appearance="standard" name="q" defaultValue={query.q ?? ""} />
          </Field>
          <Button appearance="standard" type="submit" variant="primary">
            Filter
          </Button>
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/settings/business-readiness`}>
            Clear
          </UiActionLink>
        </FilterBar>
        <p>{rows.length} matching capabilities</p>
        <Card as="section" appearance="standard" className="ui-panel">
          <TableFrame appearance="standard" label="Records" className="business-table-wrap">
            <Table className="business-table">
              <thead>
                <tr>
                  <th>Package / capability</th>
                  <th>Owner / coverage</th>
                  <th>Implementation / remaining work</th>
                  <th>Independent gates</th>
                  <th>Scoped observations</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <tr key={row.id}>
                    <td>
                      <strong>
                        {row.package} · {row.label}
                      </strong>
                      <p>{row.id}</p>
                    </td>
                    <td>
                      {row.owner}
                      <p>
                        {row.scenarios.join(", ")} · {row.gaps.join(", ")}
                      </p>
                    </td>
                    <td>
                      {row.implementation.status}
                      <p>{row.implementation.gap}</p>
                    </td>
                    <td>
                      {Object.entries(row.gates).map(([gate, value]) => (
                        <p key={gate}>
                          {gate}: {value.state}
                        </p>
                      ))}
                    </td>
                    <td>
                      {row.observations.length
                        ? row.observations.map((record) => (
                            <details key={record.id}>
                              <summary>
                                {record.gate} · {record.environment} · {record.outcome}
                              </summary>
                              <p>{record.scope}</p>
                              <p>
                                {record.observedAt} · {record.recordedBy}
                              </p>
                              <ul>
                                {record.limitations.map((text) => (
                                  <li key={text}>{text}</li>
                                ))}
                              </ul>
                              <p>Evidence ID: {record.id}</p>
                              {record.evidence.map((file) => (
                                <p key={file.path}>
                                  {file.path}
                                  <br />
                                  <small>SHA-256: {file.sha256}</small>
                                </p>
                              ))}
                            </details>
                          ))
                        : "No scoped observation recorded"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
          {!rows.length && <p>No capabilities match these filters.</p>}
        </Card>
        <p className="muted">
          Evidence registry SHA-256: {data.evidenceLedgerSha256}. Snapshot is validated during
          capability generation; browser and provider acceptance remain separate.
        </p>
      </details>
    </PageContent>
  );
}
