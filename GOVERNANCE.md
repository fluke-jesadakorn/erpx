# Project governance

ERPX starts with a maintainer-led model. [@fluke-jesadakorn](https://github.com/fluke-jesadakorn) is the initial maintainer and repository administrator. There is no elected committee or independent technical council at this stage.

## Decisions and ownership

Anyone may propose an issue, documentation change or pull request. The maintainer owns release and merge decisions, appoints maintainers, handles moderation and records the reasons for material technical decisions. [CODEOWNERS](.github/CODEOWNERS) identifies current review responsibility; it is not a statement that one person wrote or owns the copyright to every file.

Discuss domain boundaries, public contracts, authorization, schema changes, licensing and deployment topology before implementation. Use an issue for the proposal and an ADR for decisions that are hard to reverse, surprising without context and based on a real tradeoff. The [production plan](docs/production-plan.md) remains the single active delivery plan; proposals and this governance document are not competing implementation roadmaps.

Resolve disagreements through concrete examples, current contracts and testable consequences. The maintainer records the final decision when consensus is not reached. A participant with a conflict of interest should disclose it and ask for another reviewer where one is available.

## Review and merging

Follow the repository's actual protections, required checks and merge queue. Required checks must correspond to the exact reviewed candidate. Ask for independent review of material security, authorization, accounting and persistence changes where reviewers are available; do not describe an author's own checks as independent approval.

At this early stage one maintainer may author and merge a change if repository rules allow it. Record validation and material limitations in the PR. Never fabricate a review, bypass a required check or claim a one-person decision was a committee decision.

Ordinary PRs target `development`. They do not authorize production deployment, public visibility, public registry publication or historical ref replacement. Development verification, provider verification and release acceptance remain separate.

## Becoming a maintainer

A contributor may request maintainer responsibility after demonstrating sustained work in an owner area, constructive reviews, understanding of its contracts and willingness to maintain tests and documentation. The initial maintainer discusses the requested area publicly, records the decision and updates CODEOWNERS and this document when access changes.

Maintainers may step back by recording a handover, open obligations and an appropriate successor. Access may be narrowed or removed for inactivity, security needs or a code-of-conduct breach; record the reason without disclosing private details. Broader committee governance can be proposed when there are enough active maintainers to support it.

## Conduct, security and licensing

Follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), [SECURITY.md](SECURITY.md) and [LICENSING.md](LICENSING.md). Contributions use the Developer Certificate of Origin; no contributor is required to assign their copyright to the project. A governance change cannot override another rights holder's license or clear historical provenance by declaration.
