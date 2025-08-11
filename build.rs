extern crate prost_build;

fn main() {
    prost_build::compile_protos(
        &["src/cp_model.proto", "src/sat_parameters.proto"],
        &["src/"],
    )
    .unwrap();

    if std::env::var("DOCS_RS").is_err() {
        let ortools_prefix = std::env::var("ORTOOLS_PREFIX")
            .ok()
            .unwrap_or_else(|| "/opt/ortools".into());
        let mut builder = cc::Build::new();
        builder.cpp(true);
        if cfg!(target_os = "windows") && cfg!(target_env = "msvc") {
            builder.flag("/std:c++20");
        } else {
            builder.flag("-std=c++20");
        }
        builder
            .file("src/cp_sat_wrapper.cpp")
            .include(&[&ortools_prefix, "/include"].concat())
            .compile("cp_sat_wrapper.a");

        println!("cargo:rustc-link-lib=dylib=ortools");
        println!("cargo:rustc-link-search=native={}/lib", ortools_prefix);
    }
}
