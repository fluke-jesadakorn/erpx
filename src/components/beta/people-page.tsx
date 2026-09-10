import Link from "next/link";
import { notFound } from "next/navigation";
import { publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { MemberProfile } from "@/components/modules/member-profile";
import { companyMembers } from "@/server/member-directory";
import type { WorkContext } from "@/server/authorization";
export async function BetaPeople({
  actor,
  member,
}: {
  actor: WorkContext;
  member?: string;
}) {
  const people = await companyMembers(actor),
    base = workspaceBase(publicScope(actor));
  const selected = member ? people.find((p) => p.id === member) : null;
  if (member && !selected) notFound();
  return (
    <main className="content">
      <p className="eyebrow">Better together</p>
      <h1>Your people</h1>
      <p>The teammates you can work with in this company.</p>
      <section className="card">
        {people.map((p) => (
          <Link
            className="row"
            key={p.id}
            href={`${base}/people?member=${encodeURIComponent(p.id)}`}
          >
            <span className="avatar">{p.name.slice(0, 1)}</span>
            <strong>{p.name}</strong>
          </Link>
        ))}
      </section>
      {selected && (
        <MemberProfile base={base} actor={actor} member={selected} />
      )}
    </main>
  );
}
