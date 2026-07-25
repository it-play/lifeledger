# AGENTS.md — LifeLedger

Rules for every agent and contributor working in this repository. Harness-specific files
(`CLAUDE.md` and friends) reference this file, so change rules **here only**.

## Project

A mock asset-management life simulation web game. It runs on real financial rules and market
statistics, but only the player's assets are fictional. Non-commercial personal project.

`plan-docs/development-plan.md` is the single source of truth for design. Any decision that
changes the design goes into that document **before** it goes into code.

## Layout

```
client/      TypeScript + webpack, no framework (do not add React, Vue, Svelte, …)
  src/lib/     reactive · hooks · store · sse · http · router · view · form · dom — our own foundation
  src/api/     server contracts (zod) and the domain API
  src/app/     screens and app state
server/      Rust + axum. Simulation, settlement, and saves are authoritative here
plan-docs/   design documents
```

Dependencies flow one way: `app → api → lib`. `lib/` knows nothing about zod, the domain, or
screens. All zod glue lives in `src/api/zod-adapters.ts` and nowhere else.

## Commands

| Purpose | Command |
|---------|---------|
| Client dev server | `cd client && npm start` (proxies `/api` to 127.0.0.1:8080) |
| Client build | `cd client && npm run build` |
| Type check | `cd client && npm run typecheck` |
| Lint / format | `cd client && npm run lint` / `npm run lint:fix` |
| Unit tests | `cd client && npm test` |
| Run server | `cd server && cargo run` |
| Server checks | `cd server && cargo clippy --all-targets -- -D warnings && cargo fmt --check` |

The Rust toolchain lives in `~/.cargo` and is **not** on `PATH` by default. Run
`export PATH="$HOME/.cargo/bin:$PATH"` first when needed.

## Testing policy (important)

**Write unit tests for core and service logic only.** Nothing else gets tests.

- The test framework is **jest** (`client/jest.config.js`). Do not add another runner.
- In scope: simulation, tax and settlement math, the state store, protocol parsers,
  policy and eligibility rules — anything that is a **pure rule**.
- Out of scope: DOM rendering, screen interaction, routing transitions, network round trips,
  end-to-end flows, snapshots.
- Test files sit next to their subject as `*.test.ts`.
- Rust core logic is tested the same way with `cargo test`: a `#[cfg(test)] mod tests` holding
  `mod context_<situation>` blocks and `given_… _when_… _then_…` function names.

### Structure tests as BDD/DCI

- **Data** — the rule under test is the outermost `describe`
- **Context** — the situation is the inner `describe`, written as `'맥락: …'`
- **Interaction** — what happens is the `it`, titled in **given … when … then …** order

```ts
describe('재시도 판단', () => {                          // Data: the rule
  describe('맥락: 일시적인 문제로 끊긴 경우', () => {      // Context: the situation
    it('given 5xx 응답, when 판단하면, then 재시도한다', () => {  // Interaction
      expect(decider.shouldRetry({ kind: 'http', status: 500 })).toBe(true);
    });
  });
});
```

- Lay the body out as given (arrange) → when (act) → then (assert), separated by blank lines
- Name arrange helpers `givenX()` and act helpers `whenY()`
- One `it` asserts one interaction. If you need more, split the context.
- Test titles and context names stay in Korean — they describe domain behaviour to Korean readers.

## Code rules

- **Interfaces first.** Every module declares its contract in `types.ts` and exposes
  implementations through `create*()` factories. Do not extend behaviour by class inheritance.
- **Fixed public surface.** Import a module's `index.ts` barrel only. Never reach into another
  module's internal files.
- **Injectable seams.** Time, randomness, `fetch`, and logging are constructor options so tests
  can substitute them (`createManualClock`, `createNullLogger`).
- **Ownership of cleanup.** Every subscription and listener is registered in a `DisposableBag`
  and released together in `unmount` / `dispose`.
- Keep TypeScript at `strict` plus `noUncheckedIndexedAccess` and `exactOptionalPropertyTypes`.
  `any` and non-null assertions (`!`) are forbidden.
- Validate server responses with zod at the boundary. Unvalidated data never reaches a screen.
- Money is an integer number of KRW. Never compute money in floating point.

### Comments

**Write comments in English, and only where the code cannot speak for itself.**

A comment earns its place by explaining **why** — a constraint, a trade-off, a bug it prevents,
an external rule it obeys. Delete anything that restates the code.

```rust
// Bad — restates the code
// increment the game day
save.game_day += 1;

// Good — explains why this shape was chosen
// Add in the database rather than read-modify-write, so concurrent
// requests cannot overwrite each other's advance.
```

- Doc comments (`///`, `/** */`) describe the contract of a public item: what it is for and what
  a caller must know. Keep them to a sentence or two.
- Reference the design document by section (`§4.5`) instead of restating its reasoning.
- Do not leave commented-out code, changelog notes, or "TODO" without a tracked reason.
- **Exception:** BDD test titles and `describe('맥락: …')` names stay in Korean — they describe
  domain behaviour to Korean readers, and are prose, not comments.

### Client UI conventions (no framework)

**Usage guides live in the `client-foundation` skill** (`.claude/skills/client-foundation/`,
mirrored under `.agents/`). Read it before writing screen code — it documents every layer with
examples. Do not duplicate that material here.

The rules it assumes:

- Build DOM once in `mount`, then update only the nodes that changed. Never replace a subtree.
- Use `src/lib/hooks` instead of hand-rolling subscriptions; hooks register their resources with
  the `DisposableBag` passed to `createHooks`.
- Unlike React there is no call-order rule: hooks may be called conditionally or in loops.
- Charts come from a framework-agnostic library. Do not hand-draw charts.

## Commits

Follow `.claude/skills/git-commit`: `type(scope): Korean description`, subject line only,
never add an AI tool as co-author. Split work into logical units, one commit each.
