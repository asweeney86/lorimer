use lorimer_core::{MetadataRequest, MetadataService, NodeKind, ScanService};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(first) = args.next() else {
        print_usage();
        return Ok(());
    };

    match first.as_str() {
        "scan" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| "Usage: lorimer-cli scan <path>".to_string())?;
            run_scan(path)
        }
        "metadata" => {
            let path = args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| "Usage: lorimer-cli metadata <path>".to_string())?;
            run_metadata(path)
        }
        "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        value => run_scan(PathBuf::from(value)),
    }
}

fn run_scan(path: PathBuf) -> Result<(), String> {
    let snapshot = ScanService::scan(path)?;
    let stats = snapshot.stats();

    println!("Scanned: {}", stats.scanned_path.display());
    println!("Total size: {}", format_bytes(stats.total_size));
    println!("Folders: {}", stats.directory_count);
    println!("Files: {}", stats.file_count);
    println!("Skipped: {}", stats.skipped_count);
    println!("Errors: {}", stats.error_count);
    println!("Elapsed: {}", format_duration(stats.duration));

    if !snapshot.largest_files().is_empty() {
        println!();
        println!("Largest files:");
        for entry in snapshot.largest_files().iter().take(10) {
            println!(
                "  {:>10}  {}",
                format_bytes(entry.size),
                entry.path.display()
            );
        }
    }

    Ok(())
}

fn run_metadata(path: PathBuf) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("Unable to read metadata for {}: {error}", path.display()))?;
    let kind = node_kind(&metadata);
    let request = MetadataRequest {
        path: path.clone(),
        name: display_name(&path),
        kind,
        apparent_size: metadata.len(),
        item_count: None,
        error_count: 0,
        skipped_count: 0,
        is_virtual: false,
    };
    let info = MetadataService::read(&request)?;

    println!("Path: {}", info.path.display());
    println!("Name: {}", info.name);
    println!("Kind: {}", node_kind_label(info.kind));
    println!("Size: {}", format_bytes(info.apparent_size));
    println!(
        "Allocated: {}",
        info.allocated_size
            .map(format_bytes)
            .unwrap_or_else(|| "Unavailable".to_string())
    );
    println!(
        "Modified: {}",
        info.modified_at
            .unwrap_or_else(|| "Unavailable".to_string())
    );
    println!(
        "Created: {}",
        info.created_at.unwrap_or_else(|| "Unavailable".to_string())
    );
    println!(
        "Permissions: {}",
        info.permissions
            .unwrap_or_else(|| "Unavailable".to_string())
    );
    println!("Read only: {}", if info.read_only { "yes" } else { "no" });

    Ok(())
}

fn node_kind(metadata: &fs::Metadata) -> NodeKind {
    if metadata.is_dir() {
        NodeKind::Directory
    } else if metadata.is_file() {
        NodeKind::File
    } else if metadata.file_type().is_symlink() {
        NodeKind::Symlink
    } else {
        NodeKind::Other
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn node_kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Directory => "Directory",
        NodeKind::File => "File",
        NodeKind::Symlink => "Symlink",
        NodeKind::Other => "Item",
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    if duration.as_secs_f32() < 1.0 {
        format!("{} ms", duration.as_millis())
    } else {
        format!("{:.1} s", duration.as_secs_f32())
    }
}

fn format_bytes(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit_index = 0;

    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size, UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn print_usage() {
    println!("lorimer-cli");
    println!();
    println!("Usage:");
    println!("  lorimer-cli scan <path>");
    println!("  lorimer-cli metadata <path>");
    println!("  lorimer-cli <path>");
}
