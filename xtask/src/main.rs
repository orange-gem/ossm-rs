use serde::Serialize;
use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

type DynError = Box<dyn std::error::Error>;

struct Board {
    name: String,
    mcu: Mcu,
}

enum Mcu {
    Esp32S3,
    Esp32C6,
}

#[derive(Serialize)]
struct Toolchain {
    channel: String,
    components: Option<Vec<String>>,
    targets: Option<Vec<String>>,
}

#[derive(Serialize)]
struct ToolchainFile {
    toolchain: Toolchain,
}

impl Mcu {
    fn target_triple(&self) -> &str {
        match self {
            Mcu::Esp32S3 => "xtensa-esp32s3-none-elf",
            Mcu::Esp32C6 => "riscv32imac-unknown-none-elf",
        }
    }

    fn toolchain(&self) -> Toolchain {
        match self {
            Mcu::Esp32S3 => Toolchain {
                channel: "esp".to_string(),
                components: None,
                targets: None,
            },
            Mcu::Esp32C6 => Toolchain {
                channel: "stable".to_string(),
                components: Some(vec!["rust-src".to_string()]),
                targets: Some(vec!["riscv32imac-unknown-none-elf".to_string()]),
            },
        }
    }
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("run") => build_and_run()?,
        Some("clean") => clean()?,
        _ => print_help(),
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "
Available Tasks:
run: builds and runs the firmware
clean: remove all the built files
"
    )
}

fn build_and_run() -> Result<(), DynError> {
    let board = board()?;
    let feature = format!("board_{}", board.name);

    println!("Starting the build for {}", board.name);
    println!("Building in {}", project_root().to_str().unwrap());

    let toolchain = ToolchainFile {
        toolchain: board.mcu.toolchain(),
    };
    let toolchain_string = toml::to_string(&toolchain)?;
    let toolchain_config_path = project_root().join("rust-toolchain.toml");
    let mut toolchain_file = File::create(toolchain_config_path)?;
    toolchain_file.write_all(toolchain_string.as_bytes())?;

    let mut command = Command::new("cargo");
    let command = command
        .current_dir(project_root())
        .arg("run")
        .arg("--release")
        .args(&["--target", &board.mcu.target_triple()])
        .args(&["--features", &feature]);

    // Prevent the native toolchain from running
    let env_vars = env::vars();
    for (var, _value) in env_vars {
        if var.starts_with("CARGO") || var.starts_with("RUSTUP") {
            command.env_remove(var);
        }
    }

    let status = command.status()?;

    if !status.success() {
        Err("Failed to build ossm-rs")?;
    }

    Ok(())
}

fn clean() -> Result<(), DynError> {
    let status = Command::new("cargo")
        .current_dir(project_root())
        .arg("clean")
        .status()?;

    if !status.success() {
        Err("Failed to clean")?;
    }

    Ok(())
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .join("ossm-rs")
        .to_path_buf()
}

fn board() -> Result<Board, DynError> {
    let board = env::args().nth(2);

    if let Some(board) = board {
        let (name, mcu) = match board.as_str() {
            x @ "waveshare"
            | x @ "seeed_xiao_s3"
            | x @ "atom_s3"
            | x @ "ossm_v3"
            | x @ "custom" => (x, Mcu::Esp32S3),
            x @ "custom_c6" | x @ "ossm_alt_v2" => (x, Mcu::Esp32C6),
            x => Err(format!("Invalid board: {}", x))?,
        };

        Ok(Board {
            name: name.to_string(),
            mcu,
        })
    } else {
        Err("Board not gived")?
    }
}
