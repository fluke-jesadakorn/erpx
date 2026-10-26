# Historical source and private evidence references

Dated documents retain their original bytes, paths and acceptance scope. Moving a repository or preparing a public clone does not make their old source links current evidence. The documentation checker distinguishes the following references instead of treating all unavailable targets as valid.

## Recorded source snapshots

[historical-source-snapshots.json](../status/historical-source-snapshots.json) records the source commit for each listed historical document. An absolute source link is interpreted relative to an old repository root only when that exact document explicitly records the root. The checker then requires the referenced Git file or directory object to exist at the recorded commit. It does not read an arbitrary absolute path or substitute today's implementation for the old source.

The payroll research document's original source links use this policy. Git history must be available to validate them; a missing historical object remains a failing check. A public-history rewrite must update the active source mappings through its reviewed old-to-new commit map while retaining the original private evidence.

## Source-bound private artifacts

[private-artifact-references.json](../status/private-artifact-references.json) records an exact referring document, ignored artifact path, expected SHA-256, source-document commit/hash and a dated private-retention location. The source document must contain the expected artifact hash and match the recorded historical bytes. The register does not contain the artifact, authentication material or configuration files.

The Legal operations checkpoint manifest remains private. Its expected checksum and original referring document were verified when this metadata was prepared. At each documentation check, an available current or recorded private copy must match the expected hash. A mismatch fails the check. A fresh clone may have neither private copy; the checker then reports a **known unpublished private reference with availability unverified**, separately from verified links and available checksummed evidence.

An unavailable private reference does not establish current retention, reproducibility or release acceptance. Before relying on that dated Legal checkpoint outside the original workspace, obtain and verify the original evidence through the authorized private retention process. Do not copy ignored evidence into Git to make a public check pass.

## Current references still fail normally

Unknown missing links, arbitrary absolute paths, repository escapes, forged private checksums and mismatched source provenance fail. The special handling applies only to the exact source-bound references in the registers, not to every ignored path or dated document. Public-release readiness remains false until the broader [publication gates](open-source.md) are satisfied.
