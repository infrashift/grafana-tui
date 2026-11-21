// The semscope overlay of grafana-tui (`make p10`). ADR-0002 in the wiki adapts
// the NASA Power of 10 rules to Rust. ADR-0003 records why semscope checks them.
package overlay

// R3 deviation (ADR-0002): Rust, tokio, reqwest, serde and ratatui allocate on
// the heap after start-up. Named limits cap each growable structure (ADR-0007),
// so the gate reports allocations as warnings, not as errors.
rules: "NASA-P10-03": severity: "warning"

// R5 (ADR-0002): a check on external data returns an error to the caller with
// `ensure!` or `bail!`. That is the assertion form of the original rule.
rules: "NASA-P10-05": check: assert_calls: rust: [
	"assert!", "assert_eq!", "assert_ne!",
	"debug_assert!", "debug_assert_eq!", "debug_assert_ne!",
	"ensure!", "bail!",
]

// The ratchet: a rule stays off until the refactor phase that clears it turns
// it on. Remove each line when its phase closes (ISSUE-002 to ISSUE-009).
rules: "NASA-P10-02": enabled: false // ISSUE-004 loop bounds
rules: "NASA-P10-05": enabled: false // ISSUE-009 assertion density
rules: "NASA-P10-07": enabled: false // ISSUE-005 checked results
rules: "NASA-P10-08": enabled: false // ISSUE-008 macros
rules: "NASA-P10-09": enabled: false // ISSUE-008 indirection
