extern crate bindgen;
extern crate cc;
extern crate num_cpus;
extern crate pkg_config;

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::str;

use cmake::Config;

fn feature_name(name: &str) -> String {
    "CARGO_FEATURE_".to_string() + &name.to_uppercase()
}

fn feature(name: &str) -> bool {
    env::var(feature_name(name)).is_ok()
}

fn build_folder() -> String {
    if feature("taglib112") {
        "taglib-1.12".to_string()
    } else {
        "taglib-master".to_string()
    }
}

fn output() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
}

fn source() -> PathBuf {
    output().join(build_folder())
}

fn fetch() -> io::Result<()> {
    let output_base_path = output();
    let clone_dest_dir = build_folder();
    let _ = std::fs::remove_dir_all(output_base_path.join(&clone_dest_dir));
    let status = if feature("taglib112") {
        Command::new("git")
            .current_dir(&output_base_path)
            .arg("clone")
            .arg("--depth=1")
            .arg("-b")
            .arg("v1.12")
            .arg("--single-branch")
            .arg("https://github.com/taglib/taglib")
            .arg(&clone_dest_dir)
            .status()?
    } else {
        Command::new("git")
            .current_dir(&output_base_path)
            .arg("clone")
            .arg("--depth=1")
            .arg("https://github.com/taglib/taglib")
            .arg(&clone_dest_dir)
            .status()?
    };

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "fetch failed"))
    }
}

#[cfg(not(target_env = "msvc"))]
fn try_vcpkg(_statik: bool) -> Option<Vec<PathBuf>> {
    None
}

#[cfg(target_env = "msvc")]
fn try_vcpkg(statik: bool) -> Option<Vec<PathBuf>> {
    if !statik {
        env::set_var("VCPKGRS_DYNAMIC", "1");
    }

    vcpkg::find_package("taglib")
        .map_err(|e| {
            println!("Could not find taglib with vcpkg: {}", e);
        })
        .map(|library| library.include_paths)
        .ok()
}

fn search_include(include_paths: &[PathBuf], header: &str) -> Option<String> {
    for dir in include_paths {
        let include = dir.join(header);
        println!("{}", include.display());
        if fs::metadata(&include).is_ok() {
            return Some(include.as_path().to_str().unwrap().to_string());
        }
    }
    None
}

fn link_to_libraries(statik: bool) {
    let ffmpeg_ty = if statik { "static" } else { "dylib" };
    println!("cargo:rustc-link-lib={}={}", ffmpeg_ty, "libtag");
    if env::var("CARGO_FEATURE_BUILD_ZLIB").is_ok() && cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=z");
    }
}

fn main() {
    let statik = env::var("CARGO_FEATURE_STATIC").is_ok();

    // user requested to build
    let include_paths: Vec<PathBuf> = if env::var("CARGO_FEATURE_BUILD").is_ok() {
        link_to_libraries(statik);
        // check if we have built it
        if fs::metadata(&source()).is_err() {
            fs::create_dir_all(&output()).expect("failed to create build directory");
            fetch().unwrap();
        }
        let dst = Config::new(source())
            .define("BUILD_SHARED_LIBS", "OFF")
            .define("ENABLE_STATIC_RUNTIME", "ON")
            //.define("CMAKE_BUILD_TYPE", "Release")
            .build();
        println!("cargo:rustc-link-search=native={}", dst.display());
        vec![dst.join("include").join("taglib")]
    } else if let Some(paths) = try_vcpkg(statik) {
        // vcpkg doesn't detect the "system" dependencies
        if statik {
            /*if cfg!(feature = "avcodec") || cfg!(feature = "avdevice") {
                println!("cargo:rustc-link-lib=ole32");
            }

            if cfg!(feature = "avformat") {
                println!("cargo:rustc-link-lib=secur32");
                println!("cargo:rustc-link-lib=ws2_32");
            }

            // avutil depdendencies
            println!("cargo:rustc-link-lib=bcrypt");
            println!("cargo:rustc-link-lib=user32");*/
        }

        paths
    }
    // Fallback to pkg-config
    else {
        pkg_config::Config::new()
            .statik(statik)
            .probe("taglib")
            .unwrap()
            .include_paths
    };

    let clang_includes = include_paths
        .iter()
        .map(|include| format!("-I{}", include.to_string_lossy()));

    let bindings = bindgen::Builder::default()
        .header("src/wrapper.hpp")
        .clang_args(clang_includes)
        .enable_cxx_namespaces()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .clang_arg("-std=c++14")
        .ctypes_prefix("libc")
        .rustified_enum("*")
        //.prepend_enum_name(false)
        //.derive_eq(true)
        .size_t_is_usize(true)
        .opaque_type("std::.*")
        //.clang_arg("-x c++")
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
