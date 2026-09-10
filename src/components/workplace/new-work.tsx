"use client";
import { useHydrated } from "@/components/ui/use-hydrated";
import Link from "next/link";
import { useState } from "react";
import { Dialog } from "@/components/ui/dialog";
import { newWorkHref, type WorkCreation } from "@/lib/work-cards";
export function NewWork({
  base,
  creation,
  current,
}: {
  base: string;
  creation: WorkCreation[];
  current: string;
}) {
  const ready = useHydrated();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        className="button"
        disabled={!ready}
        aria-expanded={open}
        onClick={() => setOpen(true)}
      >
        New
      </button>
      <Dialog title="New work" open={open} onClose={() => setOpen(false)}>
        <div className="action-menu">
          {creation.map((item) => (
            <Link
              key={item.kind}
              href={newWorkHref(base, item, current)}
              onClick={() => setOpen(false)}
            >
              {item.label}
            </Link>
          ))}
        </div>
      </Dialog>
    </>
  );
}
