//! `fiducia` — command-line tool for fiducia.cloud.
//!
//! This file stays thin on purpose: argv in, exit code out. The CLI surface is
//! declared in `.cli-flags.toml` and the behaviour lives in the library modules
//! documented in `src/lib.rs`.

fn main() {
    if ores_clis_core::self_update::self_update_requested() {
        ores_clis_core::self_update::run_self_update_cli(
            ores_clis_core::self_update::SelfUpdateConfig::new(
                "fiducia-cloud",
                "fiducia-cli.rs",
                "fiducia",
                env!("CARGO_PKG_VERSION"),
            ),
        );
    }

    let argv = std::env::args().collect::<Vec<_>>();
    std::process::exit(fiducia_cli::run(&argv));
}
