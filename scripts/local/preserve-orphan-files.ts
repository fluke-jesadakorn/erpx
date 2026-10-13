/** Explicit byte-preserving development storage adjudication; preview by default. */
import { userInfo } from "node:os";
import { dirname, join } from "node:path";
import { LocalFileStore } from "../../src/server/modules/runtime/adapters/runtime/filesystem";
import { getRuntime, closeRuntime } from "../../src/server/modules/runtime/adapters/runtime/env";
import { resourceSlots, registeredResources } from "../../src/lib/runtime/holding-registry";
import { reconcileAttachmentStorage } from "../../src/server/modules/files/public/attachments/migration";
import { orphanObjectReferences, preserveUnreferencedFiles, sealUnreferencedStoredObjects } from "../../src/server/modules/files/public/attachments/orphan-preservation";

const apply=process.argv.includes("--apply"),env=getRuntime();
if(env.ERPX_ENVIRONMENT!=="local")throw new Error("DEVELOPMENT_FILE_PRESERVATION_ONLY");
try {
  const holdings=[];
  for(const slot of resourceSlots(env)) {
    const {write,files}=registeredResources(env,slot.id),database=await write.prepare("SELECT current_database() name").first<{name:string}>();
    if(!database||!/^erpx_dev_/.test(database.name))throw new Error("LOCAL_DEVELOPMENT_STORE_REQUIRED");
    const report=await reconcileAttachmentStorage(write,files,{verifyChecksums:true});
    const references=await orphanObjectReferences(write,report.orphanObjects),eligible=report.orphanObjects.filter(key=>!key.startsWith("legal/")&&!references.get(key)!.length);
    const blocked=report.orphanObjects.filter(key=>!eligible.includes(key)).map(key=>({objectKey:key,references:references.get(key),reason:key.startsWith("legal/")?"Legal owner decision required":"Persisted text/JSON reference requires owner investigation"}));
    const common={db:write,files,objectKeys:eligible,operatorId:`os:${userInfo().username}`,reason:"Preserve every byte and version outside canonical Company quota pending a record-owner decision; grant no file access."};
    const moved=apply&&eligible.length?files instanceof LocalFileStore
      ?await preserveUnreferencedFiles({...common,files,preservationRoot:join(dirname(files.root),`preserved-orphans-${slot.id}`)})
      :await sealUnreferencedStoredObjects({...common,storeId:slot.id}):[];
    holdings.push({holdingId:slot.id,orphanCount:report.orphanObjects.length,eligible,blocked,preserved:moved,
      after:apply?await reconcileAttachmentStorage(write,files,{verifyChecksums:true}):null});
  }
  console.log(JSON.stringify({mode:apply?"preserve-pending-owner":"read-only-preview",ownershipAssigned:false,bytesDeleted:false,holdings},null,2));
} finally { await closeRuntime(); }
