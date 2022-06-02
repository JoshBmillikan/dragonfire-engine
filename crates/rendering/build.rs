extern crate core;

use std::env;
use std::fs::read_dir;
use std::path::{Path, PathBuf};

fn main() {
    #[cfg(feature = "vulkan")]
    compile_glsl();
}

#[cfg(feature = "vulkan")]
fn compile_glsl() {
    println!("cargo:rerun-if-changed=src/vulkan/shader/glsl");
    let sources = get_sources("src/vulkan/shader".as_ref()).expect("Failed to read shader paths");
    for file in sources {
        compile_shader(&file);
    }
}

/// Compiles glsl code to spv by executing glslc as a sub process
/// Requires the VULKAN_SDK enviroment variable to refer to the
/// vulkan sdk install directory, or that glslc is available in PATH,
#[cfg(feature = "vulkan")]
fn compile_shader(path: &Path) {
    eprintln!("Compiling {:#?}", path.as_os_str());
    let name = if let Ok(env) = env::var("VULKAN_SDK") {
        env + "/bin/glslc"
    } else {
        String::from("glslc")
    };

    let out_path = find_cargo_target_dir().join("asset").join("shaders");
    std::fs::create_dir_all(&out_path).unwrap();
    #[cfg(target_family = "windows")]
    let name = name + ".exe";
    let mut cmd = std::process::Command::new(name);
    let cmd = cmd.arg(path.to_str().unwrap());
    #[cfg(not(debug_assertions))]
    let cmd = cmd.arg("-O");
    let cmd = cmd.arg("-o").arg(format!(
        "{}/{}.spv",
        out_path.to_string_lossy(),
        path.file_name().unwrap().to_str().unwrap()
    ));
    let child = cmd.spawn().unwrap_or_else(|_| {
        panic!(
            "Failed to start shader compiler for shader {}",
            path.to_str().unwrap()
        )
    });
    let out = child
        .wait_with_output()
        .expect("Failed to wait for child shader compiler process");
    if !out.status.success() {
        panic!(
            "Failed to compile shader {}\nError:{out:?}",
            path.to_string_lossy()
        );
    }
}

fn get_sources(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    assert!(path.is_dir());
    let mut paths = Vec::new();
    for file in read_dir(&path)? {
        let file = file?;
        if file.file_type()?.is_dir() {
            let mut p = get_sources(&path.join(file.file_name()))?;
            paths.append(&mut p);
        } else {
            paths.push(file.path());
        }
    }

    Ok(paths)
}

// borrowed from Rust-SDL2's build script
fn find_cargo_target_dir() -> PathBuf {
    // Infer the top level cargo target dir from the OUT_DIR by searching
    // upwards until we get to $CARGO_TARGET_DIR/build/ (which is always one
    // level up from the deepest directory containing our package name)
    let pkg_name = env::var("CARGO_PKG_NAME").unwrap();
    let mut out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    loop {
        {
            let final_path_segment = out_dir.file_name().unwrap();
            if final_path_segment.to_string_lossy().contains(&pkg_name) {
                break;
            }
        }
        if !out_dir.pop() {
            panic!("Malformed build path: {}", out_dir.to_string_lossy());
        }
    }
    out_dir.pop();
    out_dir.pop();
    out_dir
}
