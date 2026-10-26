use std::process::ExitCode;

fn main() -> ExitCode {
    match tsrs_generate_stringutil::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
