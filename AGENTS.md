# enzan — pointers for coding agents

- What the project is and promises, and the crate layout: [README.md](README.md)
- Disclosure policy — read before anything ships:
  [PUBLIC_DEVELOPMENT.md](PUBLIC_DEVELOPMENT.md)
- Issue, branch, commit and PR conventions:
  [CONTRIBUTING.md](CONTRIBUTING.md)
- Security reports: [SECURITY.md](SECURITY.md)
- Design — the model, its rules, and decisions with what settled them — lives
  in the crate docs (`cargo doc`): start at `enzan-core`'s crate root. There is
  no design directory; write design where the code it constrains is defined.
  Code documentation outranks stale issue text.
- `enzan-core` stays pure: no clock reads, no I/O, no rendering, nothing that
  breaks determinism between native and WebAssembly. The rule is stated at
  `enzan-core`'s crate root; a change that needs one of those in the core is a
  design question for the issue, not an import to add.
- Never work on `main`; one branch per issue, cut from `origin/main` after a
  fetch. `just worktree-new <type> <slug>` from the main checkout does that and
  runs `just branch-check`.
- `just check` is green. [CONTRIBUTING.md](CONTRIBUTING.md#verification) says
  what it covers.
- Review a change locally before it goes up — by a person or an AI agent — and
  say in the PR body who or what reviewed it.
  [CONTRIBUTING.md](CONTRIBUTING.md#working-with-coding-agents--the-recommended-pattern)
  lists the questions a review answers.
