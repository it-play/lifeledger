---
name: git-commit
description: Create Git commits by splitting changes into logical units following project conventions.
allowed-tools: Bash
---

## Commit Message Rules

Format: `type(scope): 설명`

- **Types**: `add` / `update` / `fix` / `refactor` / `ci/cd` / `docs` / `test` / `merge` (English)
- **Scopes** (English):
  - **Primary**: Domain names — infer from changed file paths and directory structure
  - **Cross-cutting concerns only**: Module names or `global`
  - Use domain names by default. Only use module names when changes affect multiple modules or are cross-cutting.
- **Description**: Korean, no period, avoid endings: `~한다/~된다`, `~하기/~하기 위해`, `~합니다/~됩니다`, `~했습니다`
  - Good examples: `엔티티 필드 추가`, `트랜잭션 롤백 방지`, `로직 개선`
- Subject line only (no body)
- Do NOT add AI tool as co-author

## Scope Selection

For the full scope selection table and examples, read `.agents/skills/git-commit/references/scope-guide.md`.
For commit type and scope naming conventions, read `.agents/skills/git-commit/references/commit-conventions.md`.

Quick rule: infer domain from changed file paths and directory structure. Use `global` / `ci/cd` / module names only for cross-cutting changes.

## Commit Flow

1. Inspect changes: `git status`, `git diff`
2. Categorize into logical units (feature / bug fix / refactoring / etc.)
3. Group files per unit
4. For each group:
   - Stage only relevant files with `git add`
   - Write a commit message following the rules above
   - `git commit -m "message"`
5. Verify with `git log --oneline -n <count>`
