// Performance claims guest entry point: reads the witnessed input,
// runs the pure logic in `lib.rs` (validate A0 origin, reconcile the
// ledger, compute TWR/MDD/capital), and commits the result as the public
// journal. Any failure here means the proof is never produced.

use guest::{run, GuestInput};
use risc0_zkvm::guest::env;

fn main() {
    let input: GuestInput = env::read();
    let output = run(&input)
        .expect("guest input must validate: A0 origin, reconciled ledger, positive NAV series");
    env::commit(&output);
}
