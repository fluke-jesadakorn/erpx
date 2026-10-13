/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { updateRecordSchema, createRecordSchema, transitionSchema, addReferenceSchema, suggestionSchema, suggestionDecisionSchema, addPrincipalMentionSchema, listRecords, getRecord, createRecord, archiveRecord, updateRecord, transitionRecord, recordContext, searchReferences, addPrincipalMention, addReference, requestSuggestion, decideSuggestion, recordsPage } from "@/server/modules/records/adapters/records";
export type { RecordField, WorkspaceRecord } from "@/server/modules/records/adapters/records";
