/** Compatibility entry point; implementation belongs to finance. */
export {createGroupEntity,createIntercompany,decideIntercompany,groupEntitySchema,intercompanyDecisionSchema,intercompanyDetail,intercompanyTransactionSchema,intercompanyWorkspace} from "@/server/modules/finance/adapters/finance-intercompany";
export type {GroupEntity,IntercompanyRow} from "@/server/modules/finance/adapters/finance-intercompany";
