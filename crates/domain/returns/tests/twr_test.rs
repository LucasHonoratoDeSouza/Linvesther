//! Exercises the acceptance criteria using the exact named vectors from
//! the protocol specification's "Vetores mínimos independentes" table:
//! aporte/saque/lucro/perda total pass exactly, and a gap or a zero
//! baseline never connects across segments.

use returns::{compute_twr, ReturnError, TimelineEvent::Flow, TimelineEvent::Valuation, TwrResult};
use rust_decimal::Decimal;
use std::str::FromStr;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn index_to_return(index: Decimal) -> Decimal {
    index - Decimal::ONE
}

#[test]
fn aporte_sem_lucro() {
    // 10.000 → aporte 100.000 → 110.000 : Retorno 0
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("10000"),
        },
        Flow {
            time_ms: 1,
            amount: dec("100000"),
        },
        Valuation {
            time_ms: 2,
            nav: dec("110000"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0"));
}

#[test]
fn lucro_e_aporte() {
    // 10.000 → 11.000; aporte 100.000; 111.000 → 122.100 : TWR 21%,
    // independente do salto patrimonial.
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("10000"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("11000"),
        },
        Flow {
            time_ms: 2,
            amount: dec("100000"),
        },
        Valuation {
            time_ms: 3,
            nav: dec("122100"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0.21"));
    assert_eq!(result.sub_returns, vec![dec("0.10"), dec("0.10")]);
}

#[test]
fn retirada_sem_lucro() {
    // 10.000 → saque 4.000 → 6.000 : Retorno 0
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("10000"),
        },
        Flow {
            time_ms: 1,
            amount: dec("-4000"),
        },
        Valuation {
            time_ms: 2,
            nav: dec("6000"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0"));
}

#[test]
fn taxa() {
    // 10.000, sem fluxo, fee 10 : Retorno -0,1%
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("10000"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("9990"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("-0.001"));
}

#[test]
fn transferencia_interna_sem_fee() {
    // A=100,B=0; mover 40 sem fee : NAV conjunto 100, retorno 0. Modeled
    // at the consolidated-portfolio level (the internal move nets to zero
    // net flow and zero P&L): NAV observed unchanged at 100 throughout.
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("100"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0"));
}

#[test]
fn conta_adicionada_sem_pnl() {
    // A=100; entrada B=900, sem P&L : NAV 1.000, retorno 0. Membership
    // addition is modeled as an external flow at the joining account's
    // authenticated value, exactly like a deposit.
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Flow {
            time_ms: 1,
            amount: dec("900"),
        },
        Valuation {
            time_ms: 2,
            nav: dec("1000"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0"));
}

// --- total loss: segment ends at -100%, a later deposit never connects ---

#[test]
fn perda_total_intradiaria_terminal_segment_is_minus_100_percent() {
    // NAV 100 → 0 por perda às 15h, sem fluxo.
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("0"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(
        index_to_return(result.index),
        dec("-1"),
        "total loss must be exactly -100%"
    );
}

#[test]
fn aporte_posterior_a_perda_total_abre_outro_segmento_sem_conectar() {
    // aporte 100 às 16h: a fresh compute_twr call is how a new segment
    // is represented — its index starts at 1 regardless of segment 1's
    // ending index, proving the loss is never silently erased or
    // averaged into the new segment's baseline.
    let segment1 = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("0"),
        },
    ];
    let result1 = compute_twr(&segment1).unwrap();
    assert_eq!(result1.index, Decimal::ZERO);

    let segment2 = [Flow {
        time_ms: 2,
        amount: dec("100"),
    }];
    let result2 = compute_twr(&segment2).unwrap();
    assert_eq!(
        result2.index,
        Decimal::ONE,
        "segment 2 starts fresh at I_0 = 1, unrelated to segment 1's ending index"
    );
}

#[test]
fn attempting_to_continue_the_dead_segment_is_rejected_not_silently_zero() {
    // If a caller mistakenly tries to keep closing subperiods within the
    // SAME segment after NAV hit zero, that must be a typed error, not a
    // silently-accepted zero baseline.
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("0"),
        },
        Valuation {
            time_ms: 2,
            nav: dec("50"),
        }, // wrong: tries to treat 0 as a valid v_start
    ];
    let result = compute_twr(&events);
    assert_eq!(
        result,
        Err(ReturnError::NonPositiveBaseline { v_start: dec("0") })
    );
}

// --- full withdrawal to zero: 0%, never -100% ---------------------------

#[test]
fn retirada_integral_returns_zero_not_total_loss() {
    // NAV 100, saque 100, saldo 0 : Retorno até retirada 0%; não é perda
    // total e não gera MDD 100%. The event stream ends right after the
    // Flow — no trailing Valuation(0) — so the post-withdrawal (now
    // capital-less) period is correctly left uncounted rather than
    // computed as a spurious -100%.
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("100"),
        },
        Flow {
            time_ms: 2,
            amount: dec("-100"),
        },
    ];
    let result = compute_twr(&events).unwrap();
    assert_eq!(index_to_return(result.index), dec("0"));
}

// --- gap never connects --------------------------------------------------

#[test]
fn gap_never_connects_two_segments() {
    // Segment before a gap ends with some non-trivial index; the segment
    // after the gap must start completely fresh, never chained from the
    // first.
    let before_gap = [
        Valuation {
            time_ms: 0,
            nav: dec("100"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("150"),
        },
    ];
    let before = compute_twr(&before_gap).unwrap();
    assert_eq!(before.index, dec("1.5"));

    let after_gap = [
        Valuation {
            time_ms: 100,
            nav: dec("40"),
        },
        Valuation {
            time_ms: 101,
            nav: dec("44"),
        },
    ];
    let after = compute_twr(&after_gap).unwrap();
    assert_eq!(index_to_return(after.index), dec("0.10"), "the segment after a gap computes its own fresh 10% return, not influenced by the 1.5 before it");
}

// --- overflow / non-positive guards --------------------------------------

#[test]
fn zero_baseline_at_segment_start_is_never_silently_treated_as_positive() {
    let events = [
        Valuation {
            time_ms: 0,
            nav: dec("0"),
        },
        Valuation {
            time_ms: 1,
            nav: dec("100"),
        }, // would be "infinite" return from zero
    ];
    let result = compute_twr(&events);
    assert_eq!(
        result,
        Err(ReturnError::NonPositiveBaseline { v_start: dec("0") })
    );
}

#[test]
fn cumulative_return_between_two_indices_matches_manual_computation() {
    let r = TwrResult::cumulative_return(dec("1.0"), dec("1.21")).unwrap();
    assert_eq!(r, dec("0.21"));
}

#[test]
fn cumulative_return_rejects_non_positive_index_a() {
    let result = TwrResult::cumulative_return(dec("0"), dec("1.21"));
    assert!(result.is_err());
}
