//! Load a board and save it unchanged with Muninn's board save, to see the
//! bytes s1's load/save contract produces for a file.
//!
//! ```sh
//! cargo run -p muninn-core --example board_resave -- IN.excalidraw OUT.excalidraw
//! ```
//!
//! `OUT` is written through the atomic-write path. `scripts/ui-smoke.sh` uses
//! it as the reference a board drawn on in the app is compared with: the
//! elements that were already there must come out byte for byte like this.

use std::process::ExitCode;

use muninn_core::Board;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: board_resave IN.excalidraw OUT.excalidraw");
        return ExitCode::from(2);
    };
    match Board::load(input).and_then(|board| board.save(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("FAIL {input}: {e}");
            ExitCode::from(1)
        }
    }
}
