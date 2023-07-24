fn main() {
    let profile = std::env::var("PROFILE").unwrap();
    match profile.as_str() {
        "debug" => {
            println!("cargo:rustc-link-search=../tonos-cli/target/debug");
            println!("cargo:rustc-link-search=../tonos-cli/target/debug/deps");
        },
        "release" => {
            println!("cargo:rustc-link-search=../tonos-cli/target/release");
            println!("cargo:rustc-link-search=../tonos-cli/target/release/deps");
        },
        _ => (),
    }
    println!("cargo:rustc-link-lib=dylib=tonos_cli");
}

