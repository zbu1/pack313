use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use walkdir::WalkDir;
use zip::write::FileOptions;

use chrono;

/// Simple packer for 313-style C++ projects.
///
/// It walks the given folder and zips only:
/// - *.sln
/// - *.cpp
/// - *.h
/// - *.vc*proj* (e.g. .vcxproj, .vcxproj.filters)
///
/// By default, it writes the zip into the current working directory.
#[derive(Parser, Debug)]
#[command(name = "pack313")]
#[command(version)]
#[command(
    about = "Create a filtered zip archive of a C++/Visual Studio solution tree",
    long_about = "Pack313 creates a filtered zip archive from a C++ project directory, including only \
                  source files and Visual Studio project files. It recursively scans the input directory \
                  and includes solution files (.sln), C++ source files (.cpp), header files (.h), and \
                  Visual Studio project files (.vcxproj, .vcxproj.filters, etc.). The tool creates both \
                  a folder copy and a zip archive with preserved file timestamps. All matching files are \
                  included."
)]
struct Cli {
    /// Root directory to scan for C++ project files
    input: PathBuf,

    /// Output path for the zip archive or folder
    ///
    /// If the path ends with .zip, it's treated as the zip file path and the folder name is derived from it.
    /// Otherwise, it's treated as the folder path and a .zip file is created alongside it.
    /// If not specified, defaults to <cwd>/<foldername>_submission/ and <cwd>/<foldername>_submission.zip
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Base name for output folder and zip file
    ///
    /// Overrides the default naming scheme. The output will be <name>/ and <name>.zip
    /// in the current working directory (unless --output is specified).
    #[arg(short = 'n', long = "name")]
    output_name: Option<String>,

    /// Suppress verbose output about files being added to the archive
    ///
    /// When enabled, only the final summary and output paths are printed.
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

    let folder_name = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pack313");
    let base_name = cli.output_name.unwrap_or_else(|| format!("{}_submission", folder_name));

    let (output_folder, output_zip) = if let Some(output) = cli.output {
        let output = if output.is_absolute() {
            output
        } else {
            std::env::current_dir()?.join(output)
        };
        
        // If output has .zip extension, use it for zip and derive folder name
        // Otherwise, treat it as folder name
        if output.extension().and_then(|e| e.to_str()) == Some("zip") {
            let folder_name = output.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&base_name);
            let folder_path = output.parent()
                .unwrap_or_else(|| Path::new("."))
                .join(folder_name);
            (folder_path, output)
        } else {
            let zip_path = output.with_extension("zip");
            (output, zip_path)
        }
    } else {
        let cwd = std::env::current_dir()?;
        (cwd.join(&base_name), cwd.join(format!("{}.zip", base_name)))
    };

    // Create output folder
    if output_folder.exists() {
        fs::remove_dir_all(&output_folder)?;
    }
    fs::create_dir_all(&output_folder)?;

    // Create parent dirs for zip if needed
    if let Some(parent) = output_zip.parent() {
        fs::create_dir_all(parent)?;
    }

    let zip_file = File::create(&output_zip)?;
    let mut zip = zip::ZipWriter::new(zip_file);

    let base_options: FileOptions<'_, ()> = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut buffer = Vec::new();
    let mut file_count = 0;

    for entry in WalkDir::new(&input).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        if !should_include(path) {
            continue;
        }

        let rel_path = path.strip_prefix(&input).unwrap_or(path);
        let rel_str = rel_path.to_str().ok_or_else(|| {
            format!("non-UTF-8 path encountered: {:?}", rel_path)
        })?;

        if !cli.quiet {
            writeln!(io::stderr(), "Adding: {}", rel_str.replace('\\', "/"))?;
        }

        // Copy to output folder
        let dest_path = output_folder.join(rel_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, &dest_path)?;

        // Get file metadata for timestamp preservation
        let metadata = fs::metadata(path)?;
        let modified_time = metadata.modified()?;
        let zip_datetime: zip::DateTime = chrono::DateTime::<chrono::Local>::from(modified_time)
            .naive_local()
            .try_into()
            .map_err(|_| "Time out of range for zip format")?;

        // Add to zip with preserved timestamp
        let options = base_options.last_modified_time(zip_datetime);
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
            "Added {} file{}",
            file_count,
            if file_count == 1 { "" } else { "s" }
        )?;
    }

    writeln!(
        io::stderr(),
        "Created folder at {}",
        output_folder.to_string_lossy()
    )?;
    writeln!(
        io::stderr(),
        "Created zip at {}",
        output_zip.to_string_lossy()
    )?;

    Ok(())
}

fn should_include(path: &Path) -> bool {
    let file_name = match path.file_name().and_then(|s| s.to_str()) {
        Some(name) => name,
        None => return false,
    };

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
