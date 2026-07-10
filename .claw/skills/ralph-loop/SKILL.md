---
name: ralph-loop
description: RALPH iteration — prd.json, progress.txt, verify before mark-done
---

# RALPH loop

**R**equirements → **A**ct → **L**og → **P**rove → **H**alt-if-not-ready

## Artifacts

| File | Purpose |
|------|---------|
| `prd.json` | Verifiable criteria decomposed from user ask |
| `progress.txt` | Append-only iteration log |

## Each iteration

1. Read `prd.json` criteria
2. TodoWrite mirrors plan todos
3. Execute via SDK subagents
4. Append `progress.txt` with tests run and blockers
5. Run `claw doctor` + task verification before `completed`

## Hard stop

Do not emit `status: completed` until:

- All TodoWrite items are `completed` or `cancelled`
- `claw doctor` passes (no failures)
- Phase 7 completion gate ready OR explicit user-approved blockers
