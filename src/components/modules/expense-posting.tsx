"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useExpenseCommand } from "./expense-controls";
export function ExpensePosting({
  base,
  id,
  version,
  periodId,
  entryDate,
  canPost,
}: {
  base: string;
  id: string;
  version: number;
  periodId: string;
  entryDate: string;
  canPost: boolean;
}) {
  const { send, pending, error } = useExpenseCommand(base);
  return (
    <div>
      {error && (
        <InlineNotice tone="danger" role="alert" className="error">
          {error}
        </InlineNotice>
      )}
      {canPost && (
        <Button appearance="standard"
          type="submit"
          variant="primary"
          disabled={pending || !periodId || !entryDate}
          onClick={() =>
            void send(`/${id}/post`, { expectedVersion: version, periodId, entryDate })
          }
        >
          Confirm Debit / Credit and post to GL
        </Button>
      )}
    </div>
  );
}
