# Conventions

stoker follows the program's shared conventions, whose text lives in quire's `CONVENTIONS.md`
(identical in quire, sill, shell-host, palmrest, detent and porter): types, traits, effects,
errors, tests, comments, change discipline, borrowing. Read it first. `ARCHITECTURE.md` here
adds the crate map, the one-home table, the traits, the recipes and the repo rules; where the
two disagree, `ARCHITECTURE.md` wins for stoker.

What stoker adds or decides differently, each with its reason (the first four are porter's
additions, kept so the two repos read alike):

1. **Closed sets are enums without `Word`.** stoker does not depend on quire's `ds-core`: the
   model layer must build for any desktop and any OS, below the design system. A closed set's
   stable slug is its serde `snake_case` form (the same slug in catalog files, cassettes and on
   the wire); nothing hand-writes `slug`, `parse` or `ALL`, and labels a person reads belong to
   the UI that draws them.
2. **Async seams use `-> impl Future<Output = ...> + Send`** (return-position `impl Trait`), so
   implementations write `async fn` and every future can cross a multi-threaded runtime. Closed
   sets of implementations are enums implementing the trait, never `dyn`.
3. **`todo!()` bodies exist only while an interface is frozen and its behaviour is not
   built.** Each one is listed in `FINDINGS.md` with the work that removes it. This departs from
   the shared rule "no `todo!()` stub on master"; the freeze decides when it lands.
4. **Nothing ambient below the daemons.** The clock, the environment, directories, processes,
   sockets and the GPU are passed in (`MonoMs` as an input, `EnginePaths` from settings, the
   `EngineHost`, `ReadyProbe` and `GpuProbe` seams); only a daemon that links these crates
   reads the real thing.
5. **No dependency on porter.** stoker is portable and sits below porter in the repo order.
   The one place a stoker type and a porter type meet is porter's `inferd::bridge`; a stoker
   type that mirrors a porter one (`CatalogKind` for `AiKind`) uses the same slugs and has a
   total mapping test on the porter side.
6. **Model and screen text is personal.** A type that holds what the person typed, what a model
   said or what a screenshot showed writes `Debug` by hand and prints a length, never the
   content (`TypedText`, `JsonText`, `ImageBytes`, `RequestJson`, `PreparedImage`).
7. **Parsers are total.** A parser of model output never panics, never evals, and refuses what
   it does not know: an unknown verb is dropped and reported, an out-of-frame point is refused
   and never clamped.
8. **Recorded fixtures come from dev scripts** run by hand against the user's own engine.
   Nothing in CI generates, records or downloads a model, a weight or a screenshot.
