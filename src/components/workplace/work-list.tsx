import Link from "next/link";
import { StatusBadge, EmptyState } from "@/components/ui/primitives";
import { workTypes, type WorkItem } from "@/lib/work-item";
export function WorkList({
  base,
  items,
  returnTo,
}: {
  base: string;
  items: WorkItem[];
  returnTo: string;
}) {
  return (
    <section className="card" aria-label="Work results">
      {!items.length ? (
        <EmptyState
          title="No matching work"
          description="Try another queue or clear the filters."
        />
      ) : (
        items.map((item) => (
          <Link
            className="row"
            key={`${item.type}:${item.id}`}
            href={`${base}${item.href}?${new URLSearchParams({ return: returnTo })}`}
          >
            <div className="row-main">
              <small className="muted">
                {workTypes.find(([id]) => id === item.type)?.[1]}
              </small>
              <strong>{item.title}</strong>
              <p className="muted">{item.nextAction ?? item.detail}</p>
              {item.responsibleQueue && (
                <small>With {item.responsibleQueue}</small>
              )}
              {item.dueAt && (
                <small>
                  {" "}
                  · Due{" "}
                  {new Date(item.dueAt).toLocaleDateString("en-GB", {
                    timeZone: "Asia/Bangkok",
                  })}
                </small>
              )}
              {item.blocker && <p className="muted">Blocked: {item.blocker}</p>}
            </div>
            <StatusBadge status={item.status} />
          </Link>
        ))
      )}
    </section>
  );
}
