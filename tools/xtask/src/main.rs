use std::env;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    match env::args().nth(1).as_deref() {
        Some("status") => print_status(),
        Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(command) => Err(Box::new(UnknownCommand(command.to_owned()))),
    }
}

fn print_help() {
    println!("NonProxy repository tasks");
    println!();
    println!("Usage: cargo run -p xtask -- <command>");
    println!();
    println!("Commands:");
    println!("  status  Print the resolved repository and tool directories");
    println!("  help    Print this help");
}

fn print_status() -> Result<(), Box<dyn Error>> {
    let repository = repository_root()?;
    let tools = env::var_os("NONPROXY_TOOLS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository.join(".tools"));

    println!("repository={}", repository.display());
    println!("tools={}", tools.display());
    println!("profile=development");
    Ok(())
}

fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository = manifest_dir
        .parent()
        .and_then(|tools| tools.parent())
        .ok_or("xtask must remain under tools/xtask")?;
    Ok(repository.to_path_buf())
}

#[derive(Debug)]
struct UnknownCommand(String);

impl Display for UnknownCommand {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown xtask command: {}", self.0)
    }
}

impl Error for UnknownCommand {}
