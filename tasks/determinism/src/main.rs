mod runner;

fn main() -> std::process::ExitCode {
    match runner::main(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("determinism: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
