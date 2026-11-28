# Repository recovery

Rust/Dioxus/Tonic/SQLx 137 crates (`:3004/:50051`) hard-cut to **Next.js 16** on `development` (`d15909bb`). Docs preserved, code not migrated. The former `backup/rust-2026-09-02` branch and `rust-archive-2026-09-02` tag were removed during the 2026-11-04 cleanup; the recovery baseline remains recorded as commit `cc0887d`.

```bash
git show cc0887d --stat                    # baseline
# Historical protobuf/SQLx contracts are archived; active REST contracts and PostgreSQL migrations live on this branch.
```

Removed superseded plans, the obsolete root project review and completed documentation-triage files remain recoverable from commit `9b9349b59673beaa4f24c09f5ac11f98be70291f` with `git show <commit>:<path>`. The2026-10-27 cleanup also retained a private snapshot of the complete pre-cleanup tracked/untracked working tree and patches under ignored `.scratch/dev/repository-cleanup/`; uncommitted revisions must be recovered from that snapshot rather than inferred from a commit.

