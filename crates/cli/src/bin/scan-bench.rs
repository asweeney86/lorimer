use lorimer_core::ScanService;
use std::{env, path::PathBuf, process, time::Instant};

fn main() {
    let path = match env::args_os().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: scan-bench <path>");
            process::exit(2);
        }
    };

    let started_at = Instant::now();
    let result = match ScanService::scan(path.clone()) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    };
    let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
    let stats = result.stats();

    println!(
        "path={}\telapsed_ms={elapsed_ms:.2}\ttotal_size={}\tdirectories={}\tfiles={}\tskipped={}\terrors={}",
        path.display(),
        stats.total_size,
        stats.directory_count,
        stats.file_count,
        stats.skipped_count,
        stats.error_count,
    );
}
