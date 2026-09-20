// TLS origin composition guest entry point.

use guest::{run, GuestInput};
use risc0_zkvm::guest::env;

fn main() {
    let input: GuestInput = env::read();
    let output =
        run(&input).expect("guest input must validate when origin_verified_in_guest is true");
    env::commit(&output);
}
