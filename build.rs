use std::io::Result;

fn main() -> Result<()> {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
    println!("cargo:rustc-link-arg=-fapple-link-rtlib");

    Ok(())
}