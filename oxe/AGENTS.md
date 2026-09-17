# oxe/ backend rules

Two ladders. The data ladder says no `dict` crosses a module boundary undeclared; the error
ladder says exactly one place catches broadly and everywhere else is typed. Full rationale in
`.agents/plans/2026-09-17-v0.5.0-archive-rebuild.md` ("Python: the same treatment").

## The data ladder

1. Wire models: pydantic `BaseModel` with
   `model_config = ConfigDict(extra="forbid", frozen=True, strict=True)`.
2. Internal values: frozen dataclasses or `NamedTuple`, not mutable plain objects.
3. `dict[K, V]` only for genuine homogeneous maps, always parameterised.
4. `Any` is banned.

## The error ladder

1. Domain modules raise typed exceptions declared in `oxe/errors.py` (does not exist yet —
   create it when the first domain module needs to raise something typed; this is a naming
   convention for where those exceptions live, not a claim the module exists today).
2. Every `except` names concrete types; a handler either re-raises or logs with context.
3. Exactly one broad boundary handler, in `oxe/api/errors.py` (does not exist yet, same
   caveat), registered as a FastAPI exception handler mapping domain exceptions onto an error
   envelope.
4. The ruff `ignore` list starts empty. Every future addition needs an inline comment saying
   why.

## Enforced

- `Any` is banned in function signatures and variable annotations: ruff's `ANN` rule group
  (including `ANN401`, "dynamically typed expressions disallowed") is selected, and
  basedpyright strict mode turns on `reportAny` / `reportExplicitAny`. [gate: pyproject.toml]
- Every `dict[K, V]` must be parameterised — bare `dict` fails basedpyright strict's
  `reportMissingTypeArgument`. This catches the parameterisation half of rule 3 above; it does
  not catch "a dict was used where a model should have been" (see Conventions).
  [gate: pyproject.toml]
- Broad/blind exception handling is a lint error, not a review comment: ruff's `BLE`
  (`BLE001`, blind except) and `S` (`S110`, try-except-pass) groups are selected, `TRY` is
  selected for exception-handling anti-patterns, and `ignore = []`. [gate: pyproject.toml]
- The ruff ignore list is empty today and the `select` list is the expanded one from the
  archive-rebuild plan (`E, W, F, I, B, SIM, RUF, C4, UP, BLE, TRY, ANN, ASYNC, S, PT, DTZ,
  PTH, LOG, G, TID, ERA, FBT, N, C90, PL, RET, ARG, EM, ISC, PIE, SLF, INP`).
  [gate: pyproject.toml]
- `ruff format --check` runs in the merge gate, so formatting drift cannot land as a separate
  commit. [gate: fmt:py]
- basedpyright runs in strict mode over `oxe/` and `tests/`. [gate: typecheck:py]
- Blocking calls in the search path must go through `asyncio.to_thread`: ruff's `ASYNC` group
  is selected and fails the build on `ASYNC2xx` violations. [gate: pyproject.toml]
- Function complexity and argument count are capped (`C901`/mccabe max-complexity 10,
  `PLR0913`/pylint max-args 6), so a single module cannot regrow into a 642-line file hiding
  parse defects unnoticed. [gate: pyproject.toml]
- `filterwarnings = ["error::ResourceWarning"]` in pytest config promotes leaked
  connections/files to test failures instead of silent leaks. [gate: test:py]

## Conventions

- Wire models use `ConfigDict(extra="forbid", frozen=True, strict=True)`. Nothing in
  `pyproject.toml` today mechanically checks that a given pydantic model sets this — there is
  no custom lint rule for it yet — so this is reviewer-enforced until one exists.
- Internal values are frozen dataclasses or `NamedTuple` rather than mutable plain classes or
  loose locals restructured as dicts. Not mechanically checked.
- "No `dict` crosses a module boundary" as a semantic rule (a `dict` return type that should
  have been a model) is a review judgement call; only the parameterisation half is gated (see
  Enforced).
- `oxe/errors.py` (typed domain exceptions) and `oxe/api/errors.py` (the one broad boundary
  handler) don't exist yet. When they land, every `except` clause added elsewhere should be
  narrowing toward them, not adding a second broad handler.
- Every future addition to the ruff `ignore` list needs an inline comment explaining why —
  the empty-ignore-list *state* is gated, but the review discipline around adding to it is not.
