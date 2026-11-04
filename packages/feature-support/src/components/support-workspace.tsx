import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { SupportConsole } from "@erpx/feature-support/components/support-console";
import {
  ActionLink,
  Button,
  Field,
  Select,
  Table,
  TableFrame,
  TextInput,
} from "@erpx/ui/components/foundation";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { StatusBadge } from "@erpx/ui/components/primitives";
import type { NativeSupportWorkspace } from "@erpx/feature-support/lib/native-support-read";

type Query = Record<string, string | string[] | undefined>;

const pageSize = 30;
const controlled = new Set([
  "q",
  "status",
  "new",
  "orderId",
  "sourceKind",
  "sourceId",
  "sourceVersion",
  "page",
]);

function value(query: Query, key: string): string {
  const raw = query[key];
  return typeof raw === "string" ? raw : "";
}

function viewHref(base: string, query: Query, changes: Record<string, string | undefined>): string {
  const params = new URLSearchParams();
  for (const [key, raw] of Object.entries(query)) {
    if (typeof raw === "string" && raw && !controlled.has(key)) params.set(key, raw);
  }
  for (const [key, raw] of Object.entries(changes)) {
    if (raw === undefined || raw === "") params.delete(key);
    else params.set(key, raw);
  }
  const search = params.toString();
  return canonicalUiHref(`${base}/support${search ? `?${search}` : ""}`);
}

export function SupportWorkspaceView({
  base,
  data,
  query,
  recipients = [],
  source,
  typedSource,
}: {
  base: string;
  data: NativeSupportWorkspace;
  query: Query;
  recipients?: { id: string; name: string }[];
  source?: { id: string; version: number };
  typedSource?: { kind: "pos_sale" | "pms_record" | "asset"; id: string; version: number; label: string };
}) {
  const status = value(query, "status"),
    search = value(query, "q").trim().toLowerCase(),
    items = data.items.filter(
      (row) =>
        (!status || row.status === status) &&
        (!search || `${row.title} ${row.status}`.toLowerCase().includes(search)),
    ),
    rawPage = Number(value(query, "page") || 1),
    page = Number.isSafeInteger(rawPage) && rawPage > 0 ? rawPage : 1,
    rows = items.slice((page - 1) * pageSize, page * pageSize),
    creating = value(query, "new") === "1",
    canCreate = data.canCreate || data.canCreateOrder;
  return (
    <>
      <div className="ui-form-actions">
        {canCreate &&
          (creating ? (
            <ActionLink
              appearance="standard"
              href={viewHref(base, query, { new: undefined, orderId: undefined, sourceKind: undefined, sourceId: undefined, sourceVersion: undefined })}
              variant="secondary"
            >
              Close new case
            </ActionLink>
          ) : (
            <ActionLink appearance="standard" href={viewHref(base, query, { new: "1" })} variant="primary">
              New service case
            </ActionLink>
          ))}
        <ActionLink appearance="standard" href={viewHref(base, query, {})} variant="secondary">
          Clear filters
        </ActionLink>
      </div>
      <form className="ui-form-grid ui-panel" method="get" role="search">
        <Field appearance="standard">
          Search service cases
          <TextInput
            appearance="standard"
            type="search"
            name="q"
            aria-label="Search service cases"
            placeholder="Search service cases"
            defaultValue={value(query, "q")}
          />
        </Field>
        <Field appearance="standard">
          Service status
          <Select appearance="standard" name="status" aria-label="Service status" defaultValue={status}>
            <option value="">All statuses</option>
            <option value="open">Open</option>
            <option value="triage">Triage</option>
            <option value="in_progress">In progress</option>
            <option value="resolved">Resolved</option>
            <option value="closed">Closed</option>
            <option value="cancelled">Cancelled</option>
          </Select>
        </Field>
        <Button appearance="standard" type="submit" variant="secondary">
          Apply
        </Button>
      </form>
      {creating && canCreate && (
        <FormDisclosure appearance="standard" drawerTitle="New service case" open={creating}>
          <summary>Report a new issue</summary>
          {data.canCreateOrder && (
            <form className="ui-form-grid" data-work-dirty="false">
              <input type="hidden" name="new" value="1" />
              <Field appearance="standard">
                Link a customer document (optional)
                <Select appearance="standard" name="orderId" defaultValue={value(query, "orderId")}>
                  <option value="">Direct report</option>
                  {data.sources.map((row) => (
                    <option key={String(row.id)} value={String(row.id)}>
                      {String(row.customer_name)} · {String(row.number)}
                    </option>
                  ))}
                </Select>
              </Field>
              <Button appearance="standard" type="submit" variant="secondary">
                Choose source
              </Button>
            </form>
          )}
          <SupportConsole
            base={base}
            data={{ ...data, canCreate: data.canCreate && !source }}
            source={source ? { id: source.id, version: source.version } : undefined}
            recipients={recipients}
            typedSource={typedSource}
          />
        </FormDisclosure>
      )}
      {rows.length ? (
        <TableFrame appearance="standard" label={data.ownReportsOnly ? "My service reports" : "Service cases"}>
          <Table>
            <thead>
              <tr>
                <th scope="col">Case</th>
                <th scope="col">Status</th>
                <th scope="col">Due</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={String(row.id)}>
                  <td>
                    <Link className="ui-record-item" href={canonicalUiHref(`${base}/support/${row.id}`)}>
                      {String(row.title)}
                    </Link>
                  </td>
                  <td data-label="Status">
                    <StatusBadge appearance="standard" status={String(row.status)} />
                  </td>
                  <td data-label="Due">{String(row.due_date ?? "Needs triage")}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
      ) : (
        <section className="ui-panel">
          <h2>No service cases match</h2>
          <p>Adjust the filters or report a new issue.</p>
        </section>
      )}
      <nav aria-label="Pagination" className="ui-form-actions">
        <span>
          {items.length} {items.length === 1 ? "case" : "cases"}
        </span>
        {page > 1 && (
          <Link href={viewHref(base, query, { page: String(page - 1) })}>Previous</Link>
        )}
        {(page - 1) * pageSize + rows.length < items.length && (
          <Link href={viewHref(base, query, { page: String(page + 1) })}>Next</Link>
        )}
      </nav>
      {!data.ownReportsOnly && (
        <div className="ui-form-actions">
          <ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/support/sla`)}>
            Resolution-time watch
          </ActionLink>
          <ActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/support/incidents`)}>
            Shared incidents
          </ActionLink>
        </div>
      )}
    </>
  );
}
