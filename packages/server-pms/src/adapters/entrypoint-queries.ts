// Retired: the PMS document entrypoint reads are executed by the mounted owner
// routes GET /api/v1/holdings/{slug}/pms/native-documents/document and
// /postings. The TypeScript plan/execute authority (the document query
// PmsDocumentRow, PMS_RD_DOCUMENT_BY_ID and PMS_RD_DOCUMENT_RELATED_POSTINGS)
// was deleted; callers bind the strict feature-pms native client instead.
export {};
