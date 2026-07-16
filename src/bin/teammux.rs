//! `teammux` — the fake `tmux` executable (issue #85). Real argv-in,
//! herdr-CLI-argv-out translation happens in the `herdmates::teammux`
//! library module; this binary is only the process boundary.

use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    herdmates::teammux::run(&argv)
}
