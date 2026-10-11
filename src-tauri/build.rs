fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let target = std::env::var("TARGET").unwrap();
    let script = manifest_dir.join("../scripts/prepare-libraw.mjs");
    println!("cargo:rerun-if-changed=resources/libraw-manifest.json");
    println!("cargo:rerun-if-changed={}", script.display());
    println!("cargo:rerun-if-changed=../scripts/libraw-dependency.mjs");
    println!("cargo:rerun-if-changed=src/core/photo/raw_bridge.cpp");
    println!("cargo:rerun-if-env-changed=NODE");
    let output =
        std::process::Command::new(std::env::var_os("NODE").unwrap_or_else(|| "node".into()))
            .arg(script)
            .args(["--offline", "--json", "--target", &target])
            .output()
            .expect("Node.js is needed to verify the prepared LibRaw dependency");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let link: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("LibRaw link configuration invalid");
    let root = std::path::PathBuf::from(link["directory"].as_str().unwrap());
    println!("cargo:rerun-if-changed={}", root.display());
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        .include(root.join("include"))
        .define("LIBRAW_NODLL", None)
        .warnings(false)
        .flag_if_supported("-Wno-deprecated-declarations")
        .file("src/core/photo/raw_bridge.cpp");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        assert!(
            !std::env::var("CARGO_CFG_TARGET_FEATURE")
                .unwrap_or_default()
                .split(',')
                .any(|f| f == "crt-static"),
            "The prepared MSVC library uses the dynamic release CRT"
        );
        build.static_crt(false);
        build.define("_CRT_SECURE_NO_WARNINGS", None);
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build.flag("-mmacosx-version-min=15.0");
    }
    build.compile("lutlab_raw_bridge");
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("lib").display()
    );
    println!(
        "cargo:rustc-link-lib=static={}",
        link["link"].as_str().unwrap()
    );
    for library in link["libraries"].as_array().unwrap() {
        println!("cargo:rustc-link-lib={}", library.as_str().unwrap());
    }
    tauri_build::build()
}
