/** Portable compatibility exports; the business owner maintains these domain rules. */
export {kinds,labels,fields,dateSchema,itemSchema,parseItem,transitions,allowedTransitions,executedCopySchema} from "@/server/modules/legal/domain/model";
export type {LegalKind,Field,ItemInput,LegalItem,DocumentVersion,LegalEvent,LegalDetail,LegalData,ExecutedCopyInput,ExecutedCopy} from "@/server/modules/legal/domain/model";
