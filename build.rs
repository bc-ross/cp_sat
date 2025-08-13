extern crate prost_build;

use std::{env, path::PathBuf};

use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use tar::Archive;

use cargo_metadata::{CargoOpt, MetadataCommand};

struct Repository {
    path: PathBuf,
}

impl Repository {
    fn get() -> Result<Self> {
        if let Ok(s) = env::var("ORTOOLS_PREFIX") {
            let f = PathBuf::from(s);
            if f.is_dir() {
                return Ok(Self { path: f });
            } else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "ORTOOLS_PREFIX is not a valid directory",
                )
                .into());
            }
        } else {
            return Ok(Self::download()?);
        }
    }

    fn download() -> Result<Self> {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
        let manifest_path = format!("{}/Cargo.toml", manifest_dir);

        let metadata = MetadataCommand::new()
            .manifest_path(&manifest_path)
            .features(CargoOpt::AllFeatures)
            .exec()?;

        let ortools_version = metadata.root_package().ok_or(anyhow!("failed to get package metadata"))?.metadata.get("ortools_version").ok_or(anyhow!("failed to get ortools version"))?;
        let ortools_patch = metadata.root_package().ok_or(anyhow!("failed to get package metadata"))?.metadata.get("ortools_patch").ok_or(anyhow!("failed to get ortools patch"))?;

        // Configure
        let PREFIX = format!("or-tools-{}", ortools_version.as_str().ok_or(anyhow!("failed to get ortools version as string"))?);
        let URL = format!(
            "https://github.com/google/or-tools/releases/download/v{}/or-tools_{}_cpp_v{}.{}.{}",
            ortools_version.as_str().ok_or(anyhow!("failed to get ortools version as string"))?,
            match std::env::var("TARGET")?.as_str() {
                "x86_64-pc-windows-msvc" => "x64_VisualStudio2022",
                "aarch64-unknown-linux-gnu" => "aarch64_AlmaLinux-8.10",
                _ => return Err(anyhow!("unsupported target: {}", std::env::var("TARGET").unwrap())),
            },
            ortools_version.as_str().ok_or(anyhow!("failed to get ortools version as string"))?,
            ortools_patch.as_str().ok_or(anyhow!("failed to get ortools patch as string"))?,
            cfg!(target_os = "windows")
                .then(|| "zip")
                .unwrap_or("tar.gz"),
        );

        let DIR = format!(
            "or-tools_{}_cpp_v{}.{}",
            match std::env::var("TARGET")?.as_str() {
                "x86_64-pc-windows-msvc" => "x64_VisualStudio2022",
                "aarch64-unknown-linux-gnu" => "aarch64_AlmaLinux-8.10",
                _ => return Err(anyhow!("unsupported target: {}", std::env::var("TARGET").unwrap())),
            },
            ortools_version.as_str().ok_or(anyhow!("failed to get ortools version as string"))?,
            ortools_patch.as_str().ok_or(anyhow!("failed to get ortools patch as string"))?,
        );

        let path = PathBuf::from(
            env::var("OUT_DIR").expect("failed to get environment variable: OUT_DIR"),
        );

        // Download source code
        let mut file = {
            let response = ::ureq::get(dbg!(&URL))
                .call()
                .expect("failed to download source code");

            if response.status() != 200 {
                let code = response.status_text();
                panic!("failed to download source code {URL:?}: status code {code}");
            }

            response.into_reader()
        };

        // Extract the download file
        if cfg!(target_os = "windows") {
            // Read the zip file into a buffer so we can seek
            let mut buffer = Vec::new();
            std::io::copy(&mut file, &mut buffer).expect("failed to read zip file into buffer");
            let cursor = std::io::Cursor::new(buffer);
            let mut archive = zip::ZipArchive::new(cursor).expect("failed to read zip archive");
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).expect("failed to access file in zip archive");
                let outpath = path.join(file.sanitized_name());
                if file.is_dir() {
                    std::fs::create_dir_all(&outpath).expect("failed to create directory");
                } else {
                    if let Some(parent) = outpath.parent() {
                        std::fs::create_dir_all(parent).expect("failed to create parent directory");
                    }
                    let mut outfile = std::fs::File::create(&outpath).expect("failed to create file");
                    std::io::copy(&mut file, &mut outfile).expect("failed to copy file contents");
                }
            }
        } else {
            let mut archive = Archive::new(GzDecoder::new(file));
            archive
            .entries()
            .expect("failed to get entries from downloaded file")
            .filter_map(Result::ok)
            .for_each(|mut entry| {
                if let Some(path) = entry
                .path()
                .ok()
                .and_then(|p| p.strip_prefix(&PREFIX).ok().map(|p| path.join(p)))
                {
                entry.unpack(path).expect("failed to extract file");
                }
            });
        }

        Ok(Self { path: path.join(&DIR) })
    }
}

fn main() {
    prost_build::compile_protos(
        &["src/cp_model.proto", "src/sat_parameters.proto"],
        &["src/"],
    )
    .unwrap();

    if std::env::var("DOCS_RS").is_err() {
        let ortools_lib = Repository::get().unwrap();
        let ortools_prefix = ortools_lib.path.as_os_str().to_str().unwrap();
        let mut builder = cc::Build::new();
        builder.cpp(true);
        if cfg!(target_os = "windows") && cfg!(target_env = "msvc") {
            builder.flag("/std:c++20");
        } else {
            builder.flag("-std=c++20");
        }
        builder
            .file("src/cp_sat_wrapper.cpp")
            .include(&[dbg!(&ortools_prefix), "/include"].concat())
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
