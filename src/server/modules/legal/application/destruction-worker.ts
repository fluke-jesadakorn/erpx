import type {Clock,IdentifierSource} from "../../shared/application/ports";
import type {LegalDestructionManifest} from "../domain/destruction-sources";
export type LegalDestructionWork={
beginJournal():Promise<void>;
approvedDestruction(facts:{id:string;}):Promise<{ id: string; item_id: string; company_id: string; policy_id: string; policy_version: number; } | null>;
stillEligible(facts:{item_id:string;policy_id:string;policy_version:number;now:number;}):Promise<Record<string, unknown> | null>;
blockDestruction(facts:{id:string;}):Promise<void>;
startExecution(facts:{now:number;id:string;}):Promise<void>;
recordStarted(facts:{identifier:string;fact1:string;now:number;item_id:string;}):Promise<void>;
beginExecution():Promise<void>;
executingDestruction(facts:{id:string;}):Promise<{ id: string; item_id: string; company_id: string; item_version: number; policy_id: string; policy_version: number; manifest: string; } | null>;
held(facts:{item_id:string;}):Promise<Record<string, unknown> | null>;
countAttempt(facts:{id:string;}):Promise<void>;
retainExecuting(facts:{id:string;}):Promise<void>;
retainDisposal(facts:{id:string;id2:string;checksum:string;now:number;}):Promise<void>;
eraseDocument(facts:{id:string;}):Promise<void>;
retireProcessing(facts:{item_id:string;}):Promise<void>;
eraseInspections(facts:{item_id:string;}):Promise<void>;
retireAi(facts:{item_id:string;}):Promise<void>;
revokeShares(facts:{now:number;item_id:string;}):Promise<void>;
completeDestruction(facts:{now:number;id:string;}):Promise<void>;
recordCompleted(facts:{identifier:string;item_id:string;fact2:string;item_version:number;now:number;}):Promise<void>;
lockCompany(companyId:string):Promise<void>;
canDestroy():boolean;
validate(itemId:string,companyId:string,manifest:LegalDestructionManifest):Promise<void>;
seal(companyId:string,destructionId:string,manifest:LegalDestructionManifest):Promise<void>;
destroy(keys:string[],destructionId:string):Promise<void>;
};
export function createLegalDestructionWorker(ports:{repository:{candidate():Promise<{id:string}|null>;requireReview(facts:{id:string}):Promise<void>;transaction(work:(tx:LegalDestructionWork)=>Promise<void>):Promise<void>};clock:Clock;identifiers:IdentifierSource}){
async function drainLegalDestruction(
) {
  const candidate = await ports.repository.candidate();
  if (!candidate) return;
  try {
    // Commit the execution journal before touching filesystem bytes. Recovery
    // resumes this same manifest after a crash; the item remains frozen.
    await ports.repository.transaction(async (tx) => {
      await tx.beginJournal();
      const r = await tx.approvedDestruction({id:candidate.id});
      if (!r) return;
      await tx.lockCompany(r.company_id);
      const eligible = await tx.stillEligible({item_id:r.item_id,policy_id:r.policy_id,policy_version:r.policy_version,now:ports.clock.now()});
      if (!eligible) {
        await tx.blockDestruction({id:r.id});
        return;
      }
      await tx.startExecution({now:ports.clock.now(),id:r.id});
      await tx.recordStarted({identifier:ports.identifiers.next(),fact1:`Execution journal ${r.id}`,now:ports.clock.now(),item_id:r.item_id});
    });
    await ports.repository.transaction(async (tx) => {
      await tx.beginExecution();
      const r = await tx.executingDestruction({id:candidate.id});
      if (!r) return;
      await tx.lockCompany(r.company_id);
      const held = await tx.held({item_id:r.item_id});
      if (held) throw new Error("LEGAL_HOLD");
      await tx.countAttempt({id:r.id});
      if (!tx.canDestroy())
        throw new Error("LEGAL_DESTRUCTION_STORE_UNAVAILABLE");
      const manifest = JSON.parse(r.manifest) as LegalDestructionManifest;
      await tx.validate(r.item_id,r.company_id,manifest);
      await tx.retainExecuting({id:r.id});
      await tx.destroy(manifest.keys,r.id);
      await tx.seal(r.company_id,r.id,manifest);
      for (const d of manifest.documents) {
        await tx.retainDisposal({id:d.id,id2:r.id,checksum:d.checksum,now:ports.clock.now()});
        await tx.eraseDocument({id:d.id});
      }
      await tx.retireProcessing({item_id:r.item_id});
      await tx.eraseInspections({item_id:r.item_id});
      await tx.retireAi({item_id:r.item_id});
      await tx.revokeShares({now:ports.clock.now(),item_id:r.item_id});
      await tx.completeDestruction({now:ports.clock.now(),id:r.id});
      await tx.recordCompleted({identifier:ports.identifiers.next(),item_id:r.item_id,fact2:`Destruction certificate ${r.id}; checksums retained`,item_version:r.item_version,now:ports.clock.now()});
    });
  } catch {
    await ports.repository.requireReview({id:candidate.id});
  }
}

return drainLegalDestruction;}
