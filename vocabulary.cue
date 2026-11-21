// The vocabulary of grafana-tui: technical terms of this project that the
// rules STE-100-06 (nominalizations) and STE-100-07 (-ing forms) do not
// report. `make docs-style` loads this file over the built-in ste-100 ruleset:
//
//	lexiscope scan --ruleset ste-100 --ruleset vocabulary.cue ...
//
// Add a term only when it names a concept of the project, a tool or a
// standard. Rewrite other prose.
vocabulary: {
	// The Power of 10 rules (ADR-0002).
	allocation:  true // Power of 10 rule R3
	indirection: true // Power of 10 rule R9
	deviation:   true // a Power of 10 rule that grafana-tui adapts
	suppression: true // a semscope allow comment
	assertion:   true // Power of 10 rule R5
	production:  true // production code: the code that the rules cover, not the tests

	// Grafana and Prometheus concepts.
	annotation: true // a Grafana annotation: an event on the time axis
	resolution: true // the query resolution (step) of a Prometheus range query
	recording:  true // the grafana-tui feature that saves changed frames (Ctrl+E)

	// Licensing.
	attribution: true // the credit to the original work that Apache-2.0 requires
}
