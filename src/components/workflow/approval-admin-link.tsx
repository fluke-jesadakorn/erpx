
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { approvalAdminHref } from "@/lib/shared/administration";
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
    <UiActionLink appearance="standard" variant="secondary" href={approvalAdminHref(base, kind, id)}>
      Resolve approval issue
    </UiActionLink>
  );
}
