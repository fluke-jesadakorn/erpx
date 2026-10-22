"use client";
import { AccessState } from "@erpx/workplace/components/access-state";
export default function WorkspaceError({ error, retry }: { error: Error & { digest?: string }; retry: () => void }) {
  return <AccessState problem="unavailable" reference={error.digest} retry={retry} embedded />;
}
