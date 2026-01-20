use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use walkdir::WalkDir;
use zip::write::FileOptions;

/// Simple packer for 313-style C++ projects.
///
/// It walks the given folder and zips only:
/// - *.sln
/// - *.cpp
/// - *.h
/// - *.vc*proj* (e.g. .vcxproj, .vcxproj.filters)
///
/// By default, it writes the zip into the current working directory
/// and *excludes* pch.cpp / pch.h unless --include-pch is passed.
#[derive(Parser, Debug)]
#[command(name = "pack313")]
#[command(version)]
#[command(about = "Create a filtered zip of a C++/VS solution tree", long_about = None)]
struct Cli {
    /// Root folder to scan
    input: PathBuf,

    /// Output zip path (defaults to <cwd>/<foldername>_alexanderbui_submission.zip)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Include pch.cpp and pch.h (default: false)
    #[arg(long, default_value_t = false)]
    include_pch: bool,

    /// Suppress output about included files
    #[arg(short, long, default_value_t = false)]
    quiet: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let input = cli.input.canonicalize().map_err(|e| {
        format!("failed to resolve input path {:?}: {e}", cli.input)
    })?;

    if !input.is_dir() {
        return Err(format!("input path is not a directory: {:?}", input).into());
    }

    let output = cli.output.unwrap_or_else(|| {
        let folder_name = input
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("pack313");
        let zip_name = format!("{}_alexanderbui_submission.zip", folder_name);
        std::env::current_dir().unwrap().join(zip_name)
    });

    let output = if output.is_absolute() {
        output
    } else {
        std::env::current_dir()?.join(output)
    };

    // Create parent dirs if needed.
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(&output)?;
    let mut zip = zip::ZipWriter::new(file);

    let options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut buffer = Vec::new();
    let mut file_count = 0;

    for entry in WalkDir::new(&input).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        if !should_include(path, cli.include_pch) {
            continue;
        }

        let rel_path = path.strip_prefix(&input).unwrap_or(path);
        let rel_str = rel_path.to_str().ok_or_else(|| {
            format!("non-UTF-8 path encountered: {:?}", rel_path)
        })?;

        if !cli.quiet {
            writeln!(io::stderr(), "Adding: {}", rel_str.replace('\\', "/"))?;
        }

        zip.start_file(rel_str.replace('\\', "/"), options)?;

        buffer.clear();
        std::io::copy(&mut File::open(path)?, &mut buffer_writer(&mut buffer))?;
        zip.write_all(&buffer)?;
        file_count += 1;
    }

    zip.finish()?;

    if !cli.quiet {
        writeln!(
            io::stderr(),
            "Added {} file{} to zip",
            file_count,
            if file_count == 1 { "" } else { "s" }
        )?;
    }

    writeln!(
        io::stderr(),
        "Created zip at {}",
        output.to_string_lossy()
    )?;

    Ok(())
}

fn should_include(path: &Path, include_pch: bool) -> bool {
    let file_name = match path.file_name().and_then(|s| s.to_str()) {
        Some(name) => name,
        None => return false,
    };

    if !include_pch && (file_name.eq_ignore_ascii_case("pch.cpp") || file_name.eq_ignore_ascii_case("pch.h")) {
        return false;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // Simple glob-ish filters:
    // - *.sln
    // - *.cpp
    // - *.h
    // - *.vc*proj*
    if matches!(ext.as_str(), "sln" | "cpp" | "h") {
        return true;
    }

    let lower_name = file_name.to_ascii_lowercase();
    if lower_name.contains(".vc") && lower_name.contains("proj") {
        return true;
    }

    false
}

/// Wrap a `Vec<u8>` as a `Write` so we can use `io::copy`.
fn buffer_writer(buf: &mut Vec<u8>) -> impl Write + '_ {
    struct BufWrite<'a>(&'a mut Vec<u8>);
    impl<'a> Write for BufWrite<'a> {
        fn write(&mut self, src: &[u8]) -> io::Result<usize> {
            self.0.extend_from_slice(src);
            Ok(src.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    BufWrite(buf)
}
