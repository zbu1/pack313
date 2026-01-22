# pack313

A simple tool to create filtered zip archives of C++/Visual Studio solution trees.

## Description

Pack313 recursively scans a C++ project directory and creates a zip archive containing only:
- Solution files (`.sln`)
- C++ source files (`.cpp`)
- Header files (`.h`)
- Visual Studio project files (`.vcxproj`, `.vcxproj.filters`, etc.)

The tool creates both a folder copy and a zip archive with preserved file timestamps.

## Installation

### Using GitHub Release

Download the latest `pack313.exe` from the [Releases](https://github.com/zanbowie138/pack313/releases) page and place it in your PATH.

### Build from Source

```bash
cargo install --path .
```

Or:

```bash
cargo build --release
```

## Usage

```bash
pack313 <input> [OPTIONS]
```

### Examples

```bash
# Basic usage - creates <foldername>_submission.zip in current directory
pack313 ./my-project

# Specify custom output path
pack313 ./my-project -o ./output.zip

# Custom output name
pack313 ./my-project -n my_submission

# Quiet mode (suppress file listing)
pack313 ./my-project -q
```

## Options

- `input` - Root directory to scan for C++ project files (required)
- `-o, --output <PATH>` - Output path for zip archive or folder
- `-n, --name <NAME>` - Base name for output folder and zip file
- `-q, --quiet` - Suppress verbose output about files being added

## License

This project is licensed under the MIT License. See [LICENSE.md](LICENSE.md) for details.
