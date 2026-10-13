import { hasPermission } from "@/lib/identity_access/permissions";
import { Suspense } from "react";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { isRecordKind, recordRoutes, recordCanonicalHref, type RecordTarget } from "@/lib/records/record-drawer";
import { recordOwnerPages } from "@/server/modules/records/public/record-owner-pages";
import { RecordDrawer } from "@/components/records/record-drawer";
import { RecordSupplement } from "@/components/records/record-supplement";
import { posWorkspace } from "@/server/modules/pos/public/pos";
import { PosRecordConsole } from "@/components/records/loaders/pos-record";
import { PosRecordSummary } from "@/components/records/pos-record-summary";

export const dynamic = "force-dynamic";
type Query = Record<string, string | string[] | undefined>;
export default async function RecordSlot({ params, searchParams }: { params: Promise<{ holdingSlug: string; path: string[] }>; searchParams: Promise<Query> }) {
  const q = await searchParams;
  if (typeof q.detailKind !== "string" || !isRecordKind(q.detailKind) || typeof q.detailId !== "string" || !q.detailId || q.detailId.length > 180) return null;
  const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
  if(q.detailKind==='pos_sale' && (await params).path[0]==='pos')return null;
  let args: Record<string, string> = {};
  try { const parsed = JSON.parse(typeof q.detailArgs === "string" && q.detailArgs.length < 4096 ? q.detailArgs : "{}"); if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) args = Object.fromEntries(Object.entries(parsed).filter((entry): entry is [string,string] => typeof entry[1] === "string")); } catch {}
  const target: RecordTarget = { kind: q.detailKind, id: q.detailId, args };
  const content = <RecordContent holdingSlug={holdingSlug} target={target} args={{ ...args, detailPage: typeof q.detailPage === "string" ? q.detailPage : "1" }} />;
  return <RecordDrawer key={`${base}:${actor.assignmentId}:${target.kind}:${target.id}`} title={recordRoutes[target.kind].label} fullHref={recordCanonicalHref(base, target)}><Suspense fallback={<p role="status">Loading record details…</p>}>{content}</Suspense></RecordDrawer>;
}
async function RecordContent({ holdingSlug, target, args }: { holdingSlug: string; target: RecordTarget; args: Record<string,string> }) {
  try {
    if(target.kind==='work_flow') {
      const {default:OwnerPage}=args.view==='department-approvals'?await import('../../work/department-approvals/[kind]/[id]/page'):await import('../../work/routing/[kind]/[id]/page');
      return await OwnerPage({params:Promise.resolve({holdingSlug,id:target.id,kind:args.kind}),searchParams:Promise.resolve(args)});
    }
    if(target.kind==='pos_sale')return await PosContent(holdingSlug,target.id);
    const load = recordOwnerPages[target.kind];
    if (!load) return await RecordSupplement({ holdingSlug, target, args });
    const { default: OwnerPage } = await load();
    return await OwnerPage({ params: Promise.resolve({ holdingSlug, id: target.id }), searchParams: Promise.resolve(target.kind === "legal" ? { ...args, record: target.id } : target.kind === "finance_period" ? { ...args, periodId: target.id } : args) });
  } catch (error) {
    const digest = error && typeof error === "object" && "digest" in error ? String(error.digest) : "";
    if (digest.startsWith("NEXT_HTTP_ERROR_FALLBACK;")) return <p role="alert">This record is unavailable in your current work scope.</p>;
    if (digest.startsWith("NEXT_REDIRECT;")) throw error;
    throw error;
  }
}

async function PosContent(holdingSlug:string,id:string){const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),financial=!hasPermission(actor,'pos.read'),tab=financial?'reports':'sale',initial=await posWorkspace(actor,id,'',tab,undefined,false,financial);return <PosRecordConsole base={base} initial={initial} initialTab={tab} financeMode={financial} initialRead={{sale:id,tab,finance:financial}} initialSummary={<PosRecordSummary data={initial}/>}/>;}
