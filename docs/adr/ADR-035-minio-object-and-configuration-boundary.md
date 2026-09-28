# ADR-035: MinIO for file content; PostgreSQL for configuration state

> **Accepted — 2026-09-27.** Supersedes ADR-024's private-filesystem storage choice for environments configured with MinIO. Deployment of each environment is a separate cutover.

ERPX stores durable uploaded bytes and generated downloadable files through `FileStore` in the existing MinIO service. This includes attachments, documents, message files, brand images, derivatives, payroll exports and billing evidence. Each environment and Holding has a separate object-key prefix. PostgreSQL remains authoritative for object keys, ownership, permissions, status, checksums, revisions and user-editable business configuration, including branding settings, policies, templates and provider settings. Blob content moves to MinIO; the rows that govern it do not.

Database connection manifests, provisioning credentials, deployment environment variables and IAM credential profiles stay in private local configuration because they are needed to start or authenticate the service and can contain secrets. Temporary PDF/OCR files and release assets stay local. Backups must capture PostgreSQL and the matching MinIO environment prefix together. The filesystem-to-MinIO migration verifies bytes and metadata and retains local originals until a restore is proven.
