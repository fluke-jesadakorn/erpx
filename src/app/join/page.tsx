import betaStyles from "@/components/beta/beta.module.css";
import { JoinForm } from "@/components/beta/join-form";
export default async function Join({
  searchParams,
}: {
  searchParams: Promise<{ holding?: string; token?: string }>;
}) {
  const q = await searchParams;
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <h1>You’re invited to work together</h1>
      <JoinForm holding={q.holding ?? ""} token={q.token ?? ""} />
    </main>
  );
}
