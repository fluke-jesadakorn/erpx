import { AccessState } from "@/components/workplace/access-state";
import { accessDiagnosis } from "@/server/access-diagnosis";
import { isAccessProblem, safeReturnPath } from "@/lib/access-problem";
export const dynamic = "force-dynamic";
export const metadata = { title: "Workspace access · ERPX", robots: { index: false, follow: false } };
export default async function AccessPage({ searchParams }: { searchParams: Promise<{ from?: string; reason?: string }> }) {
  const query = await searchParams;
  const from = safeReturnPath(query.from);
  const hint = query.reason && ["permission", "capability", "conflict", "invalid", "rate_limited"].includes(query.reason) && isAccessProblem(query.reason) ? query.reason : undefined;
  const diagnosis = await accessDiagnosis(from, hint);
  return <AccessState {...diagnosis} returnTo={from ?? undefined} />;
}
