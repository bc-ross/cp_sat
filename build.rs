extern crate prost_build;

use std::{env, path::PathBuf};

use flate2::read::GzDecoder;
use tar::Archive;

struct Repository {
    path: PathBuf,
}

impl Repository {
    fn try_new() -> Option<Self> {
        if !cfg!(feature = "build-force")
            && env::var("DEP_ORTOOLS_LIB").is_ok()
            && env::var("DEP_ORTOOLS_INCLUDE").is_ok()
        {
            None
        } else {
            Some(Self::download())
        }
    }

    fn download() -> Self {
        // Configure
        const PREFIX: &str = concat!(
            "or-tools-",
            env!("CARGO_PKG_VERSION_MAJOR"),
            ".",
            env!("CARGO_PKG_VERSION_MINOR"),
        );
        const URL: &str = concat!(
            "https://github.com/google/or-tools/archive/refs/tags/",
            "v",
            env!("CARGO_PKG_VERSION_MAJOR"),
            ".",
            env!("CARGO_PKG_VERSION_MINOR"),
            ".tar.gz",
        );

        let path = PathBuf::from(
            env::var("OUT_DIR").expect("failed to get environment variable: OUT_DIR"),
        );

        // Download source code
        let file = {
            let response = ::ureq::get(URL)
                .call()
                .expect("failed to download source code");

            if response.status() != 200 {
                let code = response.status_text();
                panic!("failed to download source code {URL:?}: status code {code}");
            }

            response.into_reader()
        };

        // Extract the download file
        let mut archive = Archive::new(GzDecoder::new(file));
        archive
            .entries()
            .expect("failed to get entries from downloaded file")
            .filter_map(Result::ok)
            .for_each(|mut entry| {
                if let Some(path) = entry
                    .path()
                    .ok()
                    .and_then(|p| p.strip_prefix(PREFIX).ok().map(|p| path.join(p)))
                {
                    entry.unpack(path).expect("failed to extract file");
                }
            });

        Self { path }
    }
}

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

        // println!("cargo:rustc-link-lib=dylib=ortools");
        println!("cargo:rustc-link-search=native={}/lib", ortools_prefix);

        // Link with OR-Tools libraries
        let lib_pattern = format!("{}/lib/*.lib", ortools_prefix);
        for entry in glob::glob(&lib_pattern).expect("Invalid glob pattern") {
            let entry = entry.expect("Invalid entry");
            let file_stem = entry.file_stem().expect("Invalid file stem");
            let stem = file_stem.to_str().expect("Invalid file stem");
            println!("cargo:rustc-link-lib={}", stem);
        }
    }
}
