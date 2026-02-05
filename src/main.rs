use std::fs::{self, File};
use std::io::{self, Write, Cursor};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Text,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use walkdir::WalkDir;
use zip::write::FileOptions;
use tui_banner::{Banner, Style as BannerStyle, Align};

use chrono;

// Embed audio files at compile time
static AUDIO_DATA: &[u8] = include_bytes!("../audio1.mp3");
static AUDIO_SLOW_DATA: &[u8] = include_bytes!("../audio_slow.mp3");

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

    /// Add delays during file processing for demonstration/testing
    ///
    /// When enabled, adds a small delay when processing each file.
    #[arg(long, default_value_t = false)]
    slow: bool,

    /// Add extra long delays during file processing for demonstration/testing
    ///
    /// When enabled, adds a 10-second delay when processing each file.
    #[arg(long, default_value_t = false)]
    extra_slow: bool,
}

enum UiMessage {
    FileAdded(String),
    Complete { file_count: usize, folder: String, zip: String },
    Error(String),
}

struct AppState {
    logs: Vec<String>,
    banner_frames: Arc<Mutex<Vec<String>>>,
    done: bool,
    list_state: ListState,
    auto_scroll: bool,
    spinner_frame: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Start audio playback in background (with appropriate audio for mode)
    play_audio_background(cli.extra_slow);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel for UI updates
    let (tx, rx) = mpsc::channel();

    // Shared state for banner frames
    let banner_frames = Arc::new(Mutex::new(vec![generate_static_banner()]));
    let banner_frames_clone = Arc::clone(&banner_frames);

    // Spawn banner animation thread
    thread::spawn(move || {
        animate_banner(banner_frames_clone);
    });

    // Spawn processing thread
    let tx_clone = tx.clone();
    thread::spawn(move || {
        if let Err(e) = process_files(cli, tx_clone) {
            let _ = tx.send(UiMessage::Error(format!("Error: {}", e)));
        }
    });

    // Run UI loop
    let result = run_ui(&mut terminal, rx, banner_frames);

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn play_audio_background(use_slow_audio: bool) {
    thread::spawn(move || {
        if let Err(_e) = play_audio(use_slow_audio) {
            // Silently fail if audio can't be played
        }
    });
}

fn play_audio(use_slow_audio: bool) -> Result<(), Box<dyn std::error::Error>> {
    use rodio::{Decoder, OutputStream, Sink};

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    // Use appropriate embedded audio data
    let audio_data = if use_slow_audio {
        AUDIO_SLOW_DATA
    } else {
        AUDIO_DATA
    };
    let cursor = Cursor::new(audio_data);
    let source = Decoder::new(cursor)?;

    sink.append(source);
    sink.sleep_until_end();

    Ok(())
}

fn generate_static_banner() -> String {
    Banner::new(">Pack 313")
        .ok()
        .map(|b| b.style(BannerStyle::FireWarning).align(Align::Center).render())
        .unwrap_or_else(|| ">Pack 313".to_string())
}

fn animate_banner(frames: Arc<Mutex<Vec<String>>>) {
    // Generate multiple banner frames for wave animation
    let banner_frames = generate_wave_frames();
    let mut frame_index = 0;

    loop {
        thread::sleep(Duration::from_millis(150));

        if let Ok(mut frames_guard) = frames.lock() {
            *frames_guard = vec![banner_frames[frame_index].clone()];
            frame_index = (frame_index + 1) % banner_frames.len();
        }
    }
}

fn generate_wave_frames() -> Vec<String> {
    // Generate multiple frames with wave effect
    // We'll create frames by adding vertical offsets to simulate wave
    let base_banner = Banner::new(">Pack 313")
        .ok()
        .map(|b| b.style(BannerStyle::FireWarning).align(Align::Center).render())
        .unwrap_or_else(|| ">pack313".to_string());

    let mut wave_frames = Vec::new();

    // Create 8 frames with wave pattern
    for phase in 0..8 {
        let mut frame = String::new();

        // Add wave offset (varying blank lines at top)
        let offset = match phase {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 1,
            4 => 0,
            5 => 1,
            6 => 2,
            7 => 1,
            _ => 0,
        };

        for _ in 0..offset {
            frame.push('\n');
        }

        frame.push_str(&base_banner);
        wave_frames.push(frame);
    }

    wave_frames
}

fn run_ui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    rx: Receiver<UiMessage>,
    banner_frames: Arc<Mutex<Vec<String>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app_state = AppState {
        logs: vec!["Starting pack313...".to_string()],
        banner_frames,
        done: false,
        list_state: ListState::default(),
        auto_scroll: true,
        spinner_frame: 0,
    };

    loop {
        // Process messages
        while let Ok(msg) = rx.try_recv() {
            match msg {
                UiMessage::FileAdded(file) => {
                    app_state.logs.push(format!("Adding: {}", file));
                }
                UiMessage::Complete { file_count, folder, zip } => {
                    app_state.logs.push(format!("\nAdded {} file{}",
                        file_count, if file_count == 1 { "" } else { "s" }));
                    app_state.logs.push(format!("Created folder at {}", folder));
                    app_state.logs.push(format!("Created zip at {}", zip));
                    app_state.logs.push("\nPress Enter to exit...".to_string());
                    app_state.done = true;
                }
                UiMessage::Error(err) => {
                    app_state.logs.push(format!("ERROR: {}", err));
                    app_state.logs.push("\nPress Enter to exit...".to_string());
                    app_state.done = true;
                }
            }
            // Auto-scroll to bottom when new message arrives
            if app_state.auto_scroll && !app_state.logs.is_empty() {
                app_state.list_state.select(Some(app_state.logs.len() - 1));
            }
        }

        // Update spinner animation
        app_state.spinner_frame = (app_state.spinner_frame + 1) % 4;

        // Render UI
        terminal.draw(|f| ui(f, &mut app_state))?;

        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // If done, exit on Enter key
                if app_state.done && key.code == KeyCode::Enter {
                    break;
                }

                // Handle scrolling
                match key.code {
                    KeyCode::Up => {
                        app_state.auto_scroll = false;
                        let selected = app_state.list_state.selected().unwrap_or(0);
                        if selected > 0 {
                            app_state.list_state.select(Some(selected - 1));
                        }
                    }
                    KeyCode::Down => {
                        let selected = app_state.list_state.selected().unwrap_or(0);
                        if selected < app_state.logs.len().saturating_sub(1) {
                            app_state.list_state.select(Some(selected + 1));
                            // Re-enable auto-scroll if at bottom
                            if selected + 1 >= app_state.logs.len().saturating_sub(1) {
                                app_state.auto_scroll = true;
                            }
                        }
                    }
                    KeyCode::PageUp => {
                        app_state.auto_scroll = false;
                        let selected = app_state.list_state.selected().unwrap_or(0);
                        app_state.list_state.select(Some(selected.saturating_sub(10)));
                    }
                    KeyCode::PageDown => {
                        let selected = app_state.list_state.selected().unwrap_or(0);
                        let new_pos = (selected + 10).min(app_state.logs.len().saturating_sub(1));
                        app_state.list_state.select(Some(new_pos));
                        // Re-enable auto-scroll if at bottom
                        if new_pos >= app_state.logs.len().saturating_sub(1) {
                            app_state.auto_scroll = true;
                        }
                    }
                    KeyCode::Home => {
                        app_state.auto_scroll = false;
                        app_state.list_state.select(Some(0));
                    }
                    KeyCode::End => {
                        app_state.auto_scroll = true;
                        if !app_state.logs.is_empty() {
                            app_state.list_state.select(Some(app_state.logs.len() - 1));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn ui(f: &mut Frame, app_state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),  // Banner area (increased for tui-banner)
            Constraint::Min(0),       // Log area
        ])
        .split(f.area());

    // Render banner with ANSI-to-TUI conversion
    let banner_text = if let Ok(frames) = app_state.banner_frames.lock() {
        frames.get(0).cloned().unwrap_or_else(|| "pack313".to_string())
    } else {
        "pack313".to_string()
    };

    // Convert ANSI codes to ratatui Text
    let banner_styled = ansi_to_tui::IntoText::into_text(&banner_text)
        .unwrap_or_else(|_| Text::raw("pack313"));

    let banner = Paragraph::new(banner_styled)
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(banner, chunks[0]);

    // Render logs with scrolling
    let log_items: Vec<ListItem> = app_state
        .logs
        .iter()
        .map(|log| ListItem::new(log.clone()))
        .collect();

    // Animated spinner characters
    let spinner_chars = ['⠋', '⠙', '⠹', '⠸'];
    let spinner = if !app_state.done {
        format!(" {} ", spinner_chars[app_state.spinner_frame])
    } else {
        " ✓ ".to_string()
    };

    let title = format!("{}Progress (↑↓ to scroll)", spinner);

    let logs = List::new(log_items)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::default().fg(Color::Cyan)))
        .highlight_style(Style::default().fg(Color::Yellow));

    f.render_stateful_widget(logs, chunks[1], &mut app_state.list_state);
}

fn process_files(cli: Cli, tx: Sender<UiMessage>) -> Result<(), Box<dyn std::error::Error>> {
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
            let _ = tx.send(UiMessage::FileAdded(rel_str.replace('\\', "/")));
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

        // Add delay if slow mode enabled
        if cli.extra_slow {
            thread::sleep(Duration::from_millis(10000));
        } else if cli.slow {
            thread::sleep(Duration::from_millis(1000));
        }
    }

    zip.finish()?;

    let _ = tx.send(UiMessage::Complete {
        file_count,
        folder: output_folder.to_string_lossy().to_string(),
        zip: output_zip.to_string_lossy().to_string(),
    });

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

    if matches!(ext.as_str(), "sln" | "cpp" | "h") {
        return true;
    }

    let lower_name = file_name.to_ascii_lowercase();
    if lower_name.contains(".vc") && lower_name.contains("proj") {
        return true;
    }

    false
}

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
