"use client";
import { useFinanceDraftScope } from "./finance-draft-boundary";
import type {ComponentProps} from "react";
import {FinanceEvidenceDocuments} from "./finance-evidence-documents";
import {
  Button,
  Field,
  FormLayout,
  Select,
  TextArea,
  TextInput,
} from "@/components/ui/foundation";
import { FormDisclosure } from "@/components/ui/form-disclosure";
type Row = Record<string, string | number | null>;
function FinanceReviewDecision({
  row,
  root,
  approveAction,
  canApprove,
  userId,
  busy,
  run,
  periods = [],
}: {
  row: Row;
  root: string;
  approveAction: "approve" | "post";
  canApprove: boolean;
  userId: string;
  busy: boolean;
  run: (path: string, input: unknown) => Promise<void>;
  periods?: Row[];
}) {
  const draftScope=useFinanceDraftScope();
  if (!canApprove) return null;
  if (
    row.prepared_by === userId &&
    ["draft", "posted"].includes(String(row.status))
  )
    return (
      <p className="muted">Waiting for an independent Finance approver.</p>
    );
  if (row.status === "draft")
    return (
      <FormLayout data-finance-draft={draftScope}
        className="ui-form-grid"
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(
            e.currentTarget,
            (e.nativeEvent as SubmitEvent).submitter,
          );
          await run(`${root}/${row.id}/${f.get("action")}`, {
            expectedVersion: row.version,
            reason: f.get("reason"),
          });
        }}
      >
        <Field appearance="standard">
          Review reason
          <TextArea
            appearance="standard"
            name="reason"
            required
            minLength={3}
          />
        </Field>
        <Button
          appearance="standard"
          clientManaged
          type="submit"
          name="action"
          value={approveAction}
          disabled={busy}
        >
          {approveAction === "post" ? "Post reviewed draft" : "Approve policy"}
        </Button>
        <Button
          appearance="standard"
          clientManaged
          type="submit"
          name="action"
          value="reject"
          disabled={busy}
        >
          Reject draft
        </Button>
      </FormLayout>
    );
  if (row.status === "posted" && periods.length)
    return (
      <FormDisclosure
        appearance="standard"
        drawerTitle="Reverse retained posting"
      >
        <summary>Reverse posting</summary>
        <FormLayout data-finance-draft={draftScope}
          className="ui-form-grid"
          onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            await run(`${root}/${row.id}/reverse`, {
              expectedVersion: row.version,
              periodId: f.get("periodId"),
              entryDate: f.get("entryDate"),
              reason: f.get("reason"),
            });
          }}
        >
          <Field appearance="standard">
            Open period
            <Select appearance="standard" name="periodId" required>
              {periods.map((p) => (
                <option key={String(p.id)} value={String(p.id)}>
                  {String(p.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Accounting date
            <TextInput
              appearance="standard"
              type="date"
              name="entryDate"
              required
            />
          </Field>
          <Field appearance="standard">
            Correction reason
            <TextArea
              appearance="standard"
              name="reason"
              required
              minLength={3}
            />
          </Field>
          <Button
            appearance="standard"
            clientManaged
            type="submit"
            disabled={busy}
          >
            Post exact reversal
          </Button>
        </FormLayout>
      </FormDisclosure>
    );
  return null;
}
export function FinanceExpansionDecision(props:ComponentProps<typeof FinanceReviewDecision>&{base?:string;canWrite?:boolean}){
 const target:Record<string,string>={"instruments":"finance_instrument","instrument-runs":"finance_instrument_run","asset-policies":"finance_asset_policy","asset-policy-runs":"finance_asset_policy_run","group-editions":"finance_group_report","ownership-editions":"finance_ownership_interest","acquisition-editions":"finance_acquisition"};
 return <>{props.base&&target[props.root]&&<FinanceEvidenceDocuments base={props.base} row={props.row} targetType={target[props.root]} canWrite={props.canWrite??false}/>}<FinanceReviewDecision {...props}/></>;
}
