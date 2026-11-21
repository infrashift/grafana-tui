# decisions

<!-- okfw:generated — maintained by okfw; edits here are overwritten -->

## Concepts

* [ADR-0001: Adopt an OKF wiki for project knowledge](adr-0001-adopt-an-okf-wiki-for-project-knowledge--01a1fd4e4873.md) - The project knowledge is in an OKF wiki in .okfw/ that okfw manages and make wiki validates.
* [ADR-0002: Adapt the NASA Power of 10 rules to Rust](adr-0002-adapt-the-nasa-power-of-10-rules-to-rust--01a1fd4e4878.md) - The production code follows the ten rules with Rust adaptations for rules 3, 5 and 9, and this ADR records each deviation.
* [ADR-0003: semscope and clippy enforce the Power of 10 rules](adr-0003-semscope-and-clippy-enforce-the-power-of-10-rules--01a1fd4e487d.md) - make p10 runs semscope with a ratchet overlay, and the clippy lints table turns on each rule when its phase closes.
* [ADR-0004: lexiscope checks the ASD-STE100 writing rules](adr-0004-lexiscope-checks-the-asd-ste100-writing-rules--01a1fd4e4882.md) - make docs-style runs lexiscope with the ste-100 ruleset and vocabulary.cue on the docs, the wiki and the code comments.
* [ADR-0005: Keep a modern-mode ZMakefile next to the Makefile](adr-0005-keep-a-modern-mode-zmakefile-next-to-the-makefile--01a1fd4e4886.md) - The ZMakefile has the same tasks as the Makefile in zmake modern mode, and GNU Make keeps the Makefile.
* [ADR-0006: One bounded layout walker replaces recursion](adr-0006-one-bounded-layout-walker-replaces-recursion--01a1fd7e8997.md) - Explicit stacks with depth, node and step limits replace the 24 recursive walks of the layout tree and other chains.
* [ADR-0007: Named limits cap each input and growable collection](adr-0007-named-limits-cap-each-input-and-growable-collection--01a1fdaaf533.md) - The table of every resource limit, where the program checks it and what the user sees at the limit.
* [ADR-0008: Enum dispatch replaces trait objects and function pointers](adr-0008-enum-dispatch-replaces-trait-objects-and-function-pointers--01a1fdc48e48.md) - An enum of annotation providers, plain data and small traits replace Box dyn, dyn Fn, function pointers and the theme macro.
* [ADR-0009: Remove the upstream history and keep the legal notices](adr-0009-remove-the-upstream-history-and-keep-the-legal-notices--01a1ff3c1071.md) - A new root commit replaces the upstream git history; NOTICE and the file headers keep the attribution that Apache-2.0 requires.

