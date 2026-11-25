// Pinned tsgo panics on this input, so EXPECTED is tsrs output rather than tsgo output.

use std::process::Command;

const SOURCE: &str = "async function* g() {
    yield { then() {} };
    yield 1;
}

const inferred: string = g();
";

const TSCONFIG: &str = r#"{"compilerOptions":{"target":"es2018","strict":true,"noEmit":true,"types":[]},"files":["a.ts"]}"#;

const EXPECTED: &str = "a.ts(2,11): error TS1321: Type of 'yield' operand in an async generator must either be a valid promise or must not contain a callable 'then' member.
a.ts(6,7): error TS2322: Type 'AsyncGenerator<number, void, unknown>' is not assignable to type 'string'.
";

#[test]
fn missing_yield_type_beside_other_yields_is_skipped() {
    let dir = std::env::temp_dir()
        .join(format!("tsrs-async-generator-missing-yield-type-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.ts"), SOURCE).unwrap();
    std::fs::write(dir.join("tsconfig.json"), TSCONFIG).unwrap();

    for mode in [&[][..], &["--fast"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_tsrs"))
            .current_dir(&dir)
            .args(["-p", ".", "--pretty", "false", "--singleThreaded"])
            .args(mode)
            .env_remove("TSRS_FAST")
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{mode:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "{mode:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
