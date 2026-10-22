/** Explicit public API. See ownership.json for capability migration status. */
export { updateRecordSchema, createRecordSchema, transitionSchema, addReferenceSchema, suggestionSchema, suggestionDecisionSchema, addPrincipalMentionSchema, listRecords, getRecord, createRecord, archiveRecord, updateRecord, transitionRecord, recordContext, searchReferences, addPrincipalMention, addReference, requestSuggestion, decideSuggestion, recordsPage } from "@erpx/server-records/public/records";
export type { RecordField, WorkspaceRecord } from "@erpx/server-records/public/records";
