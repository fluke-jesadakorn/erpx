# ERPX Canvas Authoring Guardrails

These rules apply to every file in `Designed/`.

## Pencil-only canvas access

- Open, inspect, search, edit, validate, render, and export `.pen` files only through the Pencil MCP tools.
- Never read, grep, parse, generate, patch, or rewrite `.pen` contents with filesystem or shell tools. Git may stage and commit a `.pen` file by its explicit path without inspecting its contents.
- Load the active Pencil schema before any canvas operation and keep the same `.pen` file explicit in each tool call.

## Authoring policy

- Update the relevant existing screen before creating a duplicate. Preserve and extend existing reusable components before introducing a new component.
- Treat canonical pages as the source of truth. Representative integrations may point to a reusable archetype instead of duplicating the same detailed UI across records.
- Name root frames with one of these prefixes: `Page ·`, `Action State ·`, `Decision ·`, or `Outcome ·`. Include the canonical URL in parentheses where the frame represents a route.
- Keep the primary columns at Page `x = 600`, Action State `x = 1820`, Decision `x = 3040`, and Outcome `x = 4260`. Canonical route frames are 1120 px wide with 100 px between columns.
- Indent each nested URL level by 160 px within its column. Keep at least 160 px between route rows and at least 320 px between categories.
- Put each action to the right of its owning page, its decision to the right of the action, and its success, blocked, and failed outcomes to the right of the decision. Align outcome branches to their decision.
- Every canonical page header must show its URL, route owner, and lifecycle. Route names, decision names, and outcomes must remain English-only.

## URL hierarchy

- Use canonical, lowercase, slash-delimited URLs. Dynamic identifiers use `:parameter_name`.
- Preserve parent-child hierarchy in both indentation and naming. A focused record page is nested below its collection; action routes append `/actions/<verb>`; decision and outcome routes append `/decision` and `/success`, `/blocked`, or `/failed`.
- Use qualified references derived from canonical ownership. A reference never grants access or exposes restricted metadata.

## Pencil workflow

- Set `placeholder: true` before modifying any root frame and clear it as soon as that frame is complete.
- After each changed section, inspect resolved bounds and layout problems through Pencil. Fix clipping, invalid dimensions, empty placeholders, overlaps, and orphan branches immediately.
- Capture a focused Pencil screenshot only after a section is complete and its placeholder flag is cleared.
- Before handoff, inspect every changed category, check for overlaps and clipped content, verify that no placeholder nodes remain, and confirm that each decision branch has success, blocked, and failed outcomes.

## Git safety

- Preserve unrelated worktree and index changes. Stage and commit only explicit paths; never use broad staging such as `git add .` or `git add -A`.
- Use non-destructive Git commands. Never force-push, reset the worktree, discard changes, clean untracked files, or rewrite shared history.
- Commit canvas and guardrail changes as small verified changesets and push `development` without force.
