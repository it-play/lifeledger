# CLAUDE.md — LifeLedger

All working rules for this repository live in `AGENTS.md`. The import below applies them verbatim.

@AGENTS.md

## Non-negotiables (from the file above)

- **Tests cover core and service logic only.** Framework is **jest**; structure is **BDD/DCI**
  (`describe` = Data → `describe('맥락: …')` = Context → `it('given … when … then …')` = Interaction).
  No DOM, routing, network, or end-to-end tests.
- **Never add a UI framework to the client.** Use the foundation in `client/src/lib/`
  (`reactive` signals and the `hooks` layer) instead.
- **Interfaces first, barrels only.** Never import another module's internal files.
- Commits are `type(scope): Korean description`, subject line only, no co-author.

## Verification

After a change, run what is relevant — not everything.

```
cd client && npm test && npm run typecheck && npm run lint
cd server && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

`cargo` is at `~/.cargo/bin` and is not on `PATH` by default:
`export PATH="$HOME/.cargo/bin:$PATH"`.

## Orientation

- Design decisions: `plan-docs/development-plan.md` (§4.1 client structure, §4.2 day-advance pipeline)
- The SSE client is a hand-written implementation of the WHATWG event-stream algorithm —
  read `client/src/lib/sse/parser.ts` before touching stream parsing.
- The server owns game time. The client only asks *how far* to advance.
