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

// R5 deviation (ADR-0002): semscope averages over every function, also the
// one-line helpers. scripts/p10-assert-density.py checks the functions of
// more than 5 lines, and `make p10` runs it after this scan.
rules: "NASA-P10-05": enabled: false
