// Keep packaged builds independent of the workspace manifest. The generator records its TypeScript pin here.
include!("src/release_generated.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/release_generated.rs");
    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap();

    println!("cargo:rustc-env=TSRS_RELEASE_VERSION={pkg_version}");
    println!("cargo:rustc-env=TSRS_TYPESCRIPT_VERSION={TYPESCRIPT_VERSION}");
    println!("cargo:rustc-env=TSRS_TYPESCRIPT_COMMIT={}", &TYPESCRIPT_COMMIT[..12]);
}
