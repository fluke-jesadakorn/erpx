CREATE TABLE attachment_documents (
 attachment_id TEXT PRIMARY KEY REFERENCES attachments(id), company_id TEXT NOT NULL,
 original_checksum TEXT NOT NULL, page_count INTEGER NOT NULL CHECK(page_count BETWEEN 1 AND 20),
 created_at INTEGER NOT NULL
) STRICT;
