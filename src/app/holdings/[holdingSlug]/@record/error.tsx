"use client";
import { useRouter } from "next/navigation";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { withoutRecordDrawer } from "@/lib/record-drawer";
export default function RecordError({ reset }: { reset: () => void }) {
  const router = useRouter();
  return <Dialog appearance="standard" presentation="form-drawer" title="Record details" open onClose={() => router.replace(withoutRecordDrawer(window.location.href), { scroll: false })}><p role="alert">The record could not be loaded. Your list is still available.</p><Button appearance="standard" onClick={reset}>Retry</Button></Dialog>;
}
