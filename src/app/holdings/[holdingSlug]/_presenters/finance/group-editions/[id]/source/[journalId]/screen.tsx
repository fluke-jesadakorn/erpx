import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { Table } from "@/components/ui/foundation";
import { RecordLink } from "@/components/records/record-link";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { unitsToDecimal } from "@/lib/finance/finance-money";
import { groupEditionSourceJournal } from "@/server/modules/finance/public/finance-group-editions";
import { pageError } from "@/server/modules/work_projection/public/page-error";
export const dynamic = "force-dynamic";
export default async function GroupSourceJournalPage({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string; journalId: string }>;
}) {
  const { holdingSlug, id, journalId } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    { edition, entity, lines } = await groupEditionSourceJournal(
      actor,
      id,
      journalId,
    ).catch((e) => pageError(e, holdingSlug)),
    j = lines[0];
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        base={base}
        active="group"
        title="Retained source journal"
        description={`${entity.reporting_name} · ${entity.chart_name} · ${j.entry_date}`}
      >
        <RecordLink href={canonicalUiHref(`${base}/finance/group-editions`)}>
          Return to group editions
        </RecordLink>
        <p>
          Report edition {String(edition.edition)} · {String(edition.status)} ·
          Journal version {String(j.version)} · {String(j.status)}
        </p>
        <p>{String(j.memo ?? "")}</p>
        <p>
          Original document: {String(j.source_type ?? "manual")} ·{" "}
          {String(j.source_id ?? journalId)}
        </p>
        {entity.company_id === actor.companyId && (
          <RecordLink href={canonicalUiHref(`${base}/finance/journals/${journalId}`)}>
            Open Company journal and document
          </RecordLink>
        )}
        <Table>
          <thead>
            <tr>
              <th>Account</th>
              <th>Debit {String(j.currency ?? entity.base_currency)}</th>
              <th>Credit</th>
              <th>Base debit {String(entity.base_currency)}</th>
              <th>Base credit</th>
              <th>Dimensions</th>
            </tr>
          </thead>
          <tbody>
            {lines.map((l) => (
              <tr key={String(l.id)}>
                <td>
                  {String(l.code)} · {String(l.name)}
                </td>
                <td>
                  {unitsToDecimal(BigInt(l.debit_minor ?? l.base_debit_minor!))}
                </td>
                <td>
                  {unitsToDecimal(
                    BigInt(l.credit_minor ?? l.base_credit_minor!),
                  )}
                </td>
                <td>{unitsToDecimal(BigInt(l.base_debit_minor!))}</td>
                <td>{unitsToDecimal(BigInt(l.base_credit_minor!))}</td>
                <td>
                  {Object.entries(JSON.parse(String(l.dimension_json)))
                    .map(([k, v]) => `${k}: ${v}`)
                    .join(", ")}
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </FinanceWorkspace>
    </FeaturePage>
  );
}
