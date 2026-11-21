# Open-source preparation

This guide records public-development requirements, not another delivery plan. The [production plan](../production-plan.md) owns implementation order and status. Adding community files does not publish the repository, accept the Rust migration or clear retained history.

The source grant is [Apache-2.0](../../LICENSE), subject to the [licensing scope](../../LICENSING.md). [provenance.json](provenance.json) is an incomplete inventory of known review categories. The initial maintainer is [@fluke-jesadakorn](https://github.com/fluke-jesadakorn); [governance](../../GOVERNANCE.md) records the current one-maintainer model.

## Development contribution foundation

- [CONTRIBUTING.md](../../CONTRIBUTING.md): local workflow, owner boundaries, tests and DCO sign-off.
- [SECURITY.md](../../SECURITY.md): private reporting and safe verification.
- [CODE_OF_CONDUCT.md](../../CODE_OF_CONDUCT.md): expected behavior, moderation and reporting.
- [CODEOWNERS](../../.github/CODEOWNERS) and the issue/PR templates: review responsibility and useful contribution context.

Maintain current engineering and community prose in English. Retain original historical evidence and add an English explanation when needed; do not rewrite a dated receipt as if it came from a new implementation. Application translations, exact business data and quoted source material have their own purpose and are not an instruction to change the UI language.

Public-fork CI uses read-only permissions, disposable databases and fictional accounts. It must not depend on operator credentials, private hosts or provider secrets. Provider rehearsals and intentional production acceptance stay separate from ordinary PR checks. Do not execute contribution code through `pull_request_target`, grant PR jobs deployment permissions or publish an image as a side effect of merging to `development`.

## Public-release gates

The following gates remain pending until their evidence is recorded:

1. A fresh clone can install pinned toolchains, prepare synthetic data, sign in normally and run relevant checks without private infrastructure. Container and Dev Container onboarding must be checked when implemented.
2. Review the current tree and every intended public branch/tag, including archived Rust source, for rights, secrets and personal data. Inspect release archives, LFS, hosted artifacts, screenshots and eventual image layers separately.
3. Finish dependency and asset provenance, retain required licenses/notices and resolve the historical proprietary-source scope. A dependency list alone is not a rights clearance.
4. Prepare an independently restorable private backup before any historical redaction. Keep exact secrets and replacement rules out of source, PR bodies and public audit reports.
5. Redact only necessary sensitive material in an isolated mirror. Preserve authorship, dates and parent relationships; keep commit/ref maps. With `git-filter-repo`, explicitly disable pruning of newly empty commits and degenerate merges when preserving this history.
6. Verify mapped recovery references and documentation links after filtering. Keep original signed/checksummed evidence privately; a redacted public derivative needs its own identity and checksum. Original signatures do not validate rewritten objects.
7. Coordinate historical ref replacement and other clones after implementation PRs merge. Revoke or rotate discovered live credentials first. Rewriting history does not erase copies already held by others.
8. Verify the maintainer's private vulnerability-reporting path, repository protections and final public settings. Repository visibility, history replacement and registry publication remain explicit release operations.

Prepare and test the public mirror without changing the original repository or remote refs. A private development checkpoint may be pushed after its source security review; that checkpoint is not public-release clearance.

## Checks and evidence

`bun run docs:check` checks community links, required foundation files and the provenance register's structure and truthful incomplete status. It does not scan all Git history, validate copyright ownership or certify operational readiness.

[Historical-reference policy](historical-references.md) explains how recorded Git source snapshots and exact, source-bound unpublished private evidence are checked without rewriting dated documents or pretending unavailable private artifacts are verified.

Keep detailed scans and sensitive review material in ignored private directories with restricted permissions. Public summaries contain paths, classifications, scope and outcomes rather than secret values or personal records. [GitHub's sensitive-data guidance](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository) describes cleanup limitations; [git-filter-repo](https://github.com/newren/git-filter-repo) documents the rewrite and mapping behavior.
