import Link from "next/link";
import { approvalAdminHref } from "@/lib/administration";
export function ApprovalAdminLink({
  base,
  kind,
  id,
}: {
  base: string;
  kind: string;
  id: string;
}) {
  return (
    <Link className="button secondary" href={approvalAdminHref(base, kind, id)}>
      Resolve approval issue
    </Link>
  );
}
