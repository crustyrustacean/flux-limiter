
# The Record

A curated list of factual, scoped statements asserting the application's **current** state. Authoritative for the present, never the future.

This file is consulted before proposing work on a task. If a task **contradicts** an entry here, the contradiction is surfaced before work proceeds. If a task **establishes** a new high-level fact, a verbatim entry is proposed for human approval as part of that proposal. You may edit the RECORD, but only after presenting the changes to a human and getting human approval.

## Why This File Exists

**IMPORTANT**:

- Its common for behaviors in code to span multiple lines or modules. For example, emitting a message in module `Foo` may be listened to by module `Bar` which then performs side-effect `Baz`. This behavior requires 3 hops to discover, so is _expensive to derive_. The RECORD.md file encodes this entire behavior in a **single line**, providing the context needed for the agent to easily learn how the application works.

If a reader could recover the fact from one grep or one file read, it does not belong here. Most things do not belong here. The list is expected to be short, and adding an entry is a claim that the fact is not already obvious from the code.

Deleting an entry is always safe; a reader that needs it will find it. A record that grows to mirror the codebase costs every future reader and goes stale within weeks.

## Format Rules

- **Factual.** Assert how things are _now_. Never future intent ("we will...", "should..."). Each entry is the _current state_ of the application.
- **Durable.** Every entry must survive a routine change. Apply the **rename test**: if the codebase renamed this thing tomorrow — a config key, a crate, a type, an actor, an event, a schema version — would the entry be false? If yes, it is not a fact about the application; it is a fact about today's source tree. Delete it.
- **Scoped.** Name what each entry applies to — repo, app, frontend, or a named subsystem. An unscoped fact (e.g. "uses Fossil") is ambiguous: is that the repo, or the app's supported VCS list? Always disambiguate.
- **High-level.** One-liners (a few sentences at most). Capture decisions and facts a reader needs, not implementation minutiae.
- **Single tag.** Each entry carries exactly one subsystem tag as a `(tag)` prefix: `- (tools) The bash tool runs...`. One entry, one tag — this keeps tag usage a meaningful coverage metric (a tag growing large signals over-specification or a tag that should split). If you cannot decide between two tags for an entry, that is a signal to **re-evaluate the entry itself**, not to assign both. Use `(tag)` rather than `[tag]` to avoid colliding with markdown task-list (checkbox) syntax.
- **Tag subsystem scope.** It's not always obvious what tag to use for a given record entry. Pick based on existing tags or somewhat related subsystem. As particular tags start to become numerous, evaluate whether a new tag (subsystem) should be created based on the content of the tags. It's normal for subsystems to form after-the-fact so feel free to propose re-tagging of existing records.
- **Singular concept.** Each entry should be a single sentence and only concerned with a single concept. Prefer multiple entries versus combining many things into one.
- **Because.** Every entry ends with `BECAUSE: <one sentence>` — why the fact holds, from the dialogue that established the fact. Not history. The clause is exempt from the singular-concept budget; the fact before it is not.

## Templates

| Pattern     | Form                                                                             | Example                                                                                                                                                                              |
| ----------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| State       | `[Scope] currently [does X / is Y]. BECAUSE: <why>.`                             | "(TUI) The TUI's first screen at startup is the chat screen. BECAUSE: startup always resumes or creates a session, so no landing screen is needed."                                  |
| Persistence | `[Scope] persists [what] to [where]. BECAUSE: <why>.`                            | "(sessions) Sessions persist to SQLite. BECAUSE: one embedded file, no server, transactional."                                                                                       |
| Flow        | `[Input/event] is handled by [actor/subsystem], which [action]. BECAUSE: <why>.` | "(tools) File edits route through the `edit` tool, which requires a unique match or `replace_all`. BECAUSE: a non-unique match must fail loudly, not edit an arbitrary occurrence."  |
| Boundary    | `[Scope] is bounded by [constraint]. BECAUSE: <why>.`                            | "(projects) Project discovery walks ancestors until a VCS root or `$HOME`, whichever comes first. BECAUSE: a repo under `$HOME` must not swallow the home directory as one project." |

## Absence

A missing record, or an un-recorded area, simply means the list has no entry there yet. Absence is not a constraint — it is an open question, and a task that fills a gap may establish the first entry for that area (proposed for human approval as part of the proposal).

## Editing

Entries are added or amended **only with human approval**.

---


- (config) `FluxLimiterConfig` rate is bounded by `1_000_000_000` requests per second, inclusive, and must be finite and positive. BECAUSE: the rate interval is stored as whole nanoseconds, so a rate above 1e9 truncates to a zero interval that never limits.
- (config) `FluxLimiterConfig` burst must be finite and non-negative, and small enough that `burst × (1e9 / rate)` fits in 64 bits. BECAUSE: the tolerance is derived from that product, and a value that does not fit would saturate to an effectively unlimited burst.
- (errors) `FluxLimiterError` has exactly three variants: `InvalidRate`, `InvalidBurst`, and `ClockError`. BECAUSE: out-of-range values reuse the existing invalid-config variants rather than adding new ones, so callers never need to distinguish "negative" from "too large".
- (gcra) All TAT arithmetic in `FluxLimiter` uses saturating addition. BECAUSE: a saturated TAT means "deny until the end of time", which is the safe outcome for an overflow that validation should already have prevented.
- (gcra) The rate interval and burst tolerance are truncated to whole nanoseconds. BECAUSE: integer arithmetic keeps timing exact and deterministic, at the cost of precision for fractional bursts and rates above roughly 1e8 per second.
- (clock) `FluxLimiter`'s default clock is `MonotonicClock`. BECAUSE: GCRA only needs elapsed time, and a monotonic source is immune to the backwards clock steps that deny all clients until the wall clock catches up.
- (clock) `MonotonicClock` anchors to the epoch once, on first `now()`, and reports `epoch_at_anchor + elapsed`. BECAUSE: it keeps `reset_time_nanos` a real Unix-epoch timestamp for headers like `X-RateLimit-Reset`, without ever re-reading the OS clock.
- (clock) `SystemClock` is the opt-in wall-clock source. BECAUSE: some callers need readings that match the OS clock exactly; the crate does not decide that for them.
- (clock) `MonotonicClock` never returns an error; `ClockError` exists for `SystemClock` and user-implemented clocks. BECAUSE: `Instant::elapsed` cannot fail, so the default clock can no longer produce `FluxLimiterError::ClockError`.
- (clock) A forward clock step has no special handling. BECAUSE: with the monotonic default a step is invisible, and with `SystemClock` it refills capacity, which GCRA handles via `max(now, tat)`.
