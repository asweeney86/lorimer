//! The scanner and data model behind Lorimer.
//!
//! [`ScanService`] walks a directory tree in parallel and returns an immutable
//! [`ScanSnapshot`]: a tree of [`ScanNode`]s addressed by [`NodeId`], plus totals and the
//! largest files found. [`MetadataService`], [`SystemActions`], and [`VolumeService`] cover
//! the per-file details, the open and reveal actions, and disk capacity.
//!
//! The crate has no dependency on any user interface toolkit, so the desktop app and the
//! command-line tool share it unchanged.
//!
//! ```no_run
//! use lorimer_core::ScanService;
//!
//! let snapshot = ScanService::scan("/Users/alex/Movies".into())?;
//! println!("{} bytes in {} files", snapshot.stats().total_size, snapshot.stats().file_count);
//! # Ok::<(), String>(())
//! ```

use crossbeam_channel::Sender;
use rayon::prelude::*;
#[cfg(unix)]
use std::collections::HashSet;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};
#[cfg(target_os = "macos")]
use std::{io, mem::size_of, os::fd::AsRawFd, ptr};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub type ScanError = String;

const FILE_DETAIL_DEPTH_LIMIT: usize = 2;
const MAX_RETAINED_FILE_NODES_PER_DIRECTORY: usize = 24;
const MAX_GLOBAL_LARGEST_FILES: usize = 20;

#[cfg(target_os = "macos")]
const ATTR_CMN_ERROR: libc::attrgroup_t = 0x2000_0000;
#[cfg(target_os = "macos")]
const BULK_BUFFER_SIZE: usize = 8 * 1024;
#[cfg(target_os = "macos")]
const VTYPE_FILE: u32 = 1;
#[cfg(target_os = "macos")]
const VTYPE_DIRECTORY: u32 = 2;
#[cfg(target_os = "macos")]
const VTYPE_SYMLINK: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    fn from_index(index: usize) -> Self {
        Self(index as u32)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Directory,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone)]
pub struct ScanStats {
    pub scanned_path: PathBuf,
    pub total_size: u64,
    pub directory_count: u64,
    pub file_count: u64,
    pub skipped_count: u64,
    pub error_count: u64,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct ScanNode {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub kind: NodeKind,
    pub children: Vec<NodeId>,
    pub item_count: u64,
    pub error_count: u64,
    pub skipped_count: u64,
    pub is_virtual: bool,
}

impl ScanNode {
    pub fn is_directory(&self) -> bool {
        matches!(self.kind, NodeKind::Directory)
    }
}

#[derive(Debug, Clone)]
pub struct LargestFileEntry {
    pub name: String,
    pub path: PathBuf,
    pub parent_path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct ScanSnapshot {
    root_id: NodeId,
    nodes: Vec<ScanNode>,
    path_index: HashMap<PathBuf, NodeId>,
    largest_files: Vec<LargestFileEntry>,
    stats: ScanStats,
}

impl ScanSnapshot {
    pub fn root_id(&self) -> NodeId {
        self.root_id
    }

    pub fn root(&self) -> &ScanNode {
        &self.nodes[self.root_id.index()]
    }

    pub fn node(&self, id: NodeId) -> Option<&ScanNode> {
        self.nodes.get(id.index())
    }

    pub fn stats(&self) -> &ScanStats {
        &self.stats
    }

    pub fn largest_files(&self) -> &[LargestFileEntry] {
        &self.largest_files
    }

    pub fn find_node(&self, path: &Path) -> Option<NodeId> {
        self.path_index.get(path).copied()
    }

    pub fn nearest_node_for_path(&self, path: &Path) -> Option<NodeId> {
        let mut current = Some(path);
        while let Some(candidate) = current {
            if let Some(id) = self.find_node(candidate) {
                return Some(id);
            }
            current = candidate.parent();
        }
        None
    }

    pub fn children(&self, id: NodeId) -> Vec<&ScanNode> {
        self.node(id)
            .map(|node| {
                node.children
                    .iter()
                    .filter_map(|child_id| self.node(*child_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn breadcrumbs(&self, id: NodeId) -> Vec<&ScanNode> {
        let mut trail = Vec::new();
        let mut current = Some(id);
        while let Some(node_id) = current {
            if let Some(node) = self.node(node_id) {
                trail.push(node);
                current = node.parent_id;
            } else {
                break;
            }
        }
        trail.reverse();
        trail
    }
}

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub discovered_directories: u64,
    pub completed_directories: u64,
    pub current_path: Option<PathBuf>,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub enum ScanEvent {
    Started {
        scanned_path: PathBuf,
    },
    Progress(ScanProgress),
    Finished(ScanSnapshot),
    Failed(String),
    /// The scan stopped early because its [`ScanCancel`] was triggered.
    Cancelled,
}

/// A handle for stopping a scan that is in progress. Clones share the same signal, so one can
/// be given to the scan and another kept by whoever may want to stop it.
#[derive(Debug, Clone, Default)]
pub struct ScanCancel(Arc<AtomicBool>);

impl ScanCancel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the scan to stop. It winds down within a few directories and reports
    /// [`ScanEvent::Cancelled`]; no partial result is produced.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

const SCAN_CANCELLED: &str = "The scan was cancelled.";

pub struct ScanService;

impl ScanService {
    pub fn scan(path: PathBuf) -> Result<ScanSnapshot, ScanError> {
        scan_directory_internal(path, None, ScanCancel::new())
    }

    pub fn stream(path: PathBuf, sender: Sender<ScanEvent>) -> Result<(), ScanError> {
        Self::stream_with_cancel(path, sender, ScanCancel::new())
    }

    /// Like [`ScanService::stream`], but stops early once `cancel` is triggered. A cancelled
    /// scan sends [`ScanEvent::Cancelled`] as its last event and returns an error.
    pub fn stream_with_cancel(
        path: PathBuf,
        sender: Sender<ScanEvent>,
        cancel: ScanCancel,
    ) -> Result<(), ScanError> {
        match scan_directory_internal(path, Some(sender.clone()), cancel.clone()) {
            Ok(snapshot) => {
                let _ = sender.send(ScanEvent::Finished(snapshot));
                Ok(())
            }
            Err(error) if cancel.is_cancelled() => {
                let _ = sender.send(ScanEvent::Cancelled);
                Err(error)
            }
            Err(error) => {
                let _ = sender.send(ScanEvent::Failed(error.clone()));
                Err(error)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetadataRequest {
    pub path: PathBuf,
    pub name: String,
    pub kind: NodeKind,
    pub apparent_size: u64,
    pub item_count: Option<u64>,
    pub error_count: u64,
    pub skipped_count: u64,
    pub is_virtual: bool,
}

#[derive(Debug, Clone)]
pub struct NodeMetadata {
    pub path: PathBuf,
    pub name: String,
    pub kind: NodeKind,
    pub apparent_size: u64,
    pub allocated_size: Option<u64>,
    pub item_count: Option<u64>,
    pub modified_at: Option<String>,
    pub created_at: Option<String>,
    pub permissions: Option<String>,
    pub read_only: bool,
    pub error_count: u64,
    pub skipped_count: u64,
    pub is_virtual: bool,
}

pub struct MetadataService;

impl MetadataService {
    pub fn read(request: &MetadataRequest) -> Result<NodeMetadata, ScanError> {
        if request.is_virtual {
            return Ok(NodeMetadata {
                path: request.path.clone(),
                name: request.name.clone(),
                kind: request.kind,
                apparent_size: request.apparent_size,
                allocated_size: None,
                item_count: request.item_count,
                modified_at: None,
                created_at: None,
                permissions: None,
                read_only: false,
                error_count: request.error_count,
                skipped_count: request.skipped_count,
                is_virtual: true,
            });
        }

        let metadata = fs::symlink_metadata(&request.path).map_err(|error| {
            format!(
                "Unable to read metadata for {}: {error}",
                request.path.display()
            )
        })?;

        Ok(NodeMetadata {
            path: request.path.clone(),
            name: request.name.clone(),
            kind: request.kind,
            apparent_size: request.apparent_size,
            allocated_size: allocated_size(&metadata),
            item_count: request.item_count,
            modified_at: format_system_time(metadata.modified().ok()),
            created_at: format_system_time(metadata.created().ok()),
            permissions: permissions_string(&metadata),
            read_only: metadata.permissions().readonly(),
            error_count: request.error_count,
            skipped_count: request.skipped_count,
            is_virtual: false,
        })
    }
}

pub struct SystemActions;

impl SystemActions {
    pub fn open(path: &Path) -> Result<(), ScanError> {
        #[cfg(target_os = "macos")]
        {
            return run_command("open", [path.as_os_str()]);
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            return run_command("xdg-open", [path.as_os_str()]);
        }

        #[cfg(windows)]
        {
            // Explorer opens folders itself and hands files to their default application.
            return spawn_explorer(|command| {
                command.arg(path);
            });
        }

        #[allow(unreachable_code)]
        {
            let _ = path;
            Err("Open is not implemented on this platform.".to_string())
        }
    }

    pub fn reveal(path: &Path) -> Result<(), ScanError> {
        #[cfg(target_os = "macos")]
        {
            if path.is_dir() {
                return run_command("open", [path.as_os_str()]);
            }
            return run_command("open", [std::ffi::OsStr::new("-R"), path.as_os_str()]);
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let target = if path.is_dir() {
                path
            } else {
                path.parent().unwrap_or(path)
            };
            return run_command("xdg-open", [target.as_os_str()]);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;

            // `/select,` must be followed by the quoted path as one raw argument; the normal
            // argument quoting would wrap the whole switch in quotes and Explorer ignores it.
            return spawn_explorer(|command| {
                command.raw_arg(format!("/select,\"{}\"", path.display()));
            });
        }

        #[allow(unreachable_code)]
        {
            let _ = path;
            Err("Reveal is not implemented on this platform.".to_string())
        }
    }

    pub fn reveal_label() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            return "Reveal in Finder";
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            return "Open in File Manager";
        }

        #[cfg(windows)]
        {
            return "Show in Explorer";
        }

        #[allow(unreachable_code)]
        "Reveal"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl VolumeUsage {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }

    pub fn used_fraction(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }

        (self.used_bytes() as f64 / self.total_bytes as f64) as f32
    }
}

pub struct VolumeService;

impl VolumeService {
    /// Capacity of the volume that contains `path`.
    pub fn usage(path: &Path) -> Result<VolumeUsage, ScanError> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;

            let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
                .map_err(|_| format!("Invalid path {}.", path.display()))?;
            let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();

            // SAFETY: `c_path` is a valid NUL-terminated string and `stats` points to
            // writable memory of the right size; it is only read after a successful call.
            let stats = unsafe {
                if libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) != 0 {
                    return Err(format!(
                        "Unable to read volume usage for {}: {}",
                        path.display(),
                        std::io::Error::last_os_error()
                    ));
                }
                stats.assume_init()
            };

            // The field types differ between platforms, so the casts are not always needed.
            #[allow(clippy::unnecessary_cast)]
            let (block_size, blocks, available) = (
                stats.f_frsize as u64,
                stats.f_blocks as u64,
                stats.f_bavail as u64,
            );
            let total_bytes = blocks.saturating_mul(block_size);
            let available_bytes = available.saturating_mul(block_size).min(total_bytes);

            return Ok(VolumeUsage {
                total_bytes,
                available_bytes,
            });
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut available_bytes = 0_u64;
            let mut total_bytes = 0_u64;

            // SAFETY: `wide` is a valid NUL-terminated UTF-16 string, and the two out pointers
            // refer to live `u64` values. The third out parameter is optional and left null.
            let succeeded = unsafe {
                GetDiskFreeSpaceExW(
                    wide.as_ptr(),
                    &mut available_bytes,
                    &mut total_bytes,
                    std::ptr::null_mut(),
                )
            };
            if succeeded == 0 {
                return Err(format!(
                    "Unable to read volume usage for {}: {}",
                    path.display(),
                    std::io::Error::last_os_error()
                ));
            }

            return Ok(VolumeUsage {
                total_bytes,
                available_bytes: available_bytes.min(total_bytes),
            });
        }

        #[allow(unreachable_code)]
        {
            let _ = path;
            Err("Volume usage is not implemented on this platform.".to_string())
        }
    }
}

#[derive(Debug, Default, Clone)]
struct Totals {
    directories: u64,
    files: u64,
    skipped: u64,
    errors: u64,
}

impl Totals {
    fn merge(&mut self, other: Self) {
        self.directories = self.directories.saturating_add(other.directories);
        self.files = self.files.saturating_add(other.files);
        self.skipped = self.skipped.saturating_add(other.skipped);
        self.errors = self.errors.saturating_add(other.errors);
    }
}

#[derive(Debug)]
struct ScanOutcome {
    node: TreeNode,
    totals: Totals,
}

#[derive(Debug, Clone)]
struct TreeNode {
    name: String,
    path: PathBuf,
    size: u64,
    kind: NodeKind,
    children: Vec<TreeNode>,
    item_count: u64,
    error_count: u64,
    skipped_count: u64,
    is_virtual: bool,
}

#[derive(Debug, Clone)]
struct ChildEntry {
    kind: NodeKind,
    path: PathBuf,
    size_hint: Option<u64>,
    hardlink_info: Option<HardlinkInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FileIdentity {
    #[cfg(unix)]
    device_id: u64,
    #[cfg(unix)]
    file_id: u64,
}

// Hardlinks are only tracked on Unix, where the link count comes for free with the metadata.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
struct HardlinkInfo {
    identity: FileIdentity,
    link_count: u64,
}

#[derive(Debug, Clone)]
struct ProgressState {
    started_at: Instant,
    discovered_directories: u64,
    completed_directories: u64,
}

struct ScanContext {
    cancel: ScanCancel,
    events: Option<Sender<ScanEvent>>,
    progress: Mutex<ProgressState>,
    largest_files: Mutex<Vec<LargestFileEntry>>,
    #[cfg(unix)]
    seen_hardlinks: Mutex<HashSet<FileIdentity>>,
    /// Whether the scan started at `/`, where the data volume is also reachable through
    /// firmlinks.
    #[cfg(target_os = "macos")]
    scans_whole_disk: bool,
    /// Device of the scan root. Directories on other devices are mount points and are skipped.
    #[cfg(unix)]
    root_device: u64,
}

impl ScanContext {
    fn new(
        events: Option<Sender<ScanEvent>>,
        cancel: ScanCancel,
        root: &Path,
        root_metadata: &fs::Metadata,
    ) -> Self {
        #[cfg(not(target_os = "macos"))]
        let _ = root;
        #[cfg(not(unix))]
        let _ = root_metadata;

        Self {
            cancel,
            #[cfg(target_os = "macos")]
            scans_whole_disk: root == Path::new("/"),
            #[cfg(unix)]
            root_device: root_metadata.dev(),
            events,
            progress: Mutex::new(ProgressState {
                started_at: Instant::now(),
                discovered_directories: 1,
                completed_directories: 0,
            }),
            largest_files: Mutex::new(Vec::new()),
            #[cfg(unix)]
            seen_hardlinks: Mutex::new(HashSet::new()),
        }
    }

    /// Whether the directory at `path` should be left out of the scan: either it is a mount
    /// point for another filesystem, or its contents are already counted under another path.
    ///
    /// Staying on one filesystem keeps a scan of `/` away from other disks, network shares,
    /// and virtual filesystems such as `/proc`, whose reported sizes are meaningless.
    fn is_outside_scan(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            let Ok(metadata) = fs::symlink_metadata(path) else {
                // Leave unreadable directories to the scan itself, which records the error.
                return false;
            };

            if metadata.dev() != self.root_device {
                return true;
            }

            #[cfg(target_os = "macos")]
            if self.scans_whole_disk && is_firmlink_duplicate(path, &metadata) {
                return true;
            }
        }

        #[cfg(not(unix))]
        let _ = path;

        false
    }

    fn counted_file_size(&self, size: u64, hardlink_info: Option<HardlinkInfo>) -> u64 {
        #[cfg(not(unix))]
        let _ = hardlink_info;

        #[cfg(unix)]
        if let Some(hardlink_info) = hardlink_info {
            if hardlink_info.link_count > 1 {
                let mut seen = self
                    .seen_hardlinks
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !seen.insert(hardlink_info.identity) {
                    return 0;
                }
            }
        }

        size
    }

    fn note_discovered(&self, current_path: &Path, count: u64) {
        let progress = {
            let mut state = self
                .progress
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.discovered_directories = state.discovered_directories.saturating_add(count);
            ScanProgress {
                discovered_directories: state.discovered_directories,
                completed_directories: state.completed_directories,
                current_path: Some(current_path.to_path_buf()),
                elapsed: state.started_at.elapsed(),
            }
        };
        emit(self, ScanEvent::Progress(progress));
    }

    fn note_completed(&self, current_path: &Path) {
        let progress = {
            let mut state = self
                .progress
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.completed_directories = state.completed_directories.saturating_add(1);
            ScanProgress {
                discovered_directories: state.discovered_directories,
                completed_directories: state.completed_directories,
                current_path: Some(current_path.to_path_buf()),
                elapsed: state.started_at.elapsed(),
            }
        };
        emit(self, ScanEvent::Progress(progress));
    }

    fn largest_files(&self) -> Vec<LargestFileEntry> {
        let mut files = self
            .largest_files
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        files.sort_unstable_by(|left, right| {
            right
                .size
                .cmp(&left.size)
                .then_with(|| left.name.cmp(&right.name))
        });
        files
    }

    fn track_file(&self, path: &Path, size: u64) {
        if size == 0 {
            return;
        }

        let mut files = self
            .largest_files
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = LargestFileEntry {
            name: display_name(path),
            path: path.to_path_buf(),
            parent_path: path.parent().unwrap_or(path).to_path_buf(),
            size,
        };

        if files.len() < MAX_GLOBAL_LARGEST_FILES {
            files.push(entry);
            return;
        }

        if let Some((smallest_index, smallest)) =
            files.iter().enumerate().min_by_key(|(_, file)| file.size)
        {
            if entry.size > smallest.size {
                files[smallest_index] = entry;
            }
        }
    }
}

fn scan_directory_internal(
    path: PathBuf,
    events: Option<Sender<ScanEvent>>,
    cancel: ScanCancel,
) -> Result<ScanSnapshot, ScanError> {
    let canonical = canonicalize_input(path)?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|error| format!("Unable to inspect {}: {error}", canonical.display()))?;

    if !metadata.is_dir() {
        return Err(format!("{} is not a directory.", canonical.display()));
    }

    let context = Arc::new(ScanContext::new(events, cancel, &canonical, &metadata));
    emit(
        &context,
        ScanEvent::Started {
            scanned_path: canonical.clone(),
        },
    );
    context.note_discovered(&canonical, 0);

    let started_at = Instant::now();
    let outcome = scan_node(&canonical, &context)?;
    // Directories abandoned by a cancelled scan look like read errors to their parents, so the
    // tree built so far is incomplete and must not be returned as a result.
    if context.cancel.is_cancelled() {
        return Err(SCAN_CANCELLED.to_string());
    }
    let largest_files = context.largest_files();
    let stats = ScanStats {
        scanned_path: canonical,
        total_size: outcome.node.size,
        directory_count: outcome.totals.directories,
        file_count: outcome.totals.files,
        skipped_count: outcome.totals.skipped,
        error_count: outcome.totals.errors,
        duration: started_at.elapsed(),
    };
    Ok(flatten_snapshot(outcome.node, stats, largest_files))
}

fn canonicalize_input(path: PathBuf) -> Result<PathBuf, ScanError> {
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|error| format!("Unable to read current working directory: {error}"))?
            .join(path)
    };

    let canonical = absolute
        .canonicalize()
        .map_err(|error| format!("Unable to access {}: {error}", absolute.display()))?;

    // Windows canonicalizes to verbatim paths such as `\\?\C:\Users`, which are unpleasant
    // to read and which Explorer does not accept.
    #[cfg(windows)]
    let canonical = canonical
        .to_str()
        .and_then(strip_verbatim_prefix)
        .map(PathBuf::from)
        .unwrap_or(canonical);

    Ok(canonical)
}

/// Turns a Windows verbatim path into its everyday form: `\\?\C:\Users` becomes `C:\Users`
/// and `\\?\UNC\server\share` becomes `\\server\share`. Returns `None` for anything else.
#[cfg(any(windows, test))]
fn strip_verbatim_prefix(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;

    if let Some(share) = rest.strip_prefix(r"UNC\") {
        return Some(format!(r"\\{share}"));
    }

    let mut characters = rest.chars();
    let is_drive = characters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && characters.next() == Some(':')
        && matches!(characters.next(), Some('\\') | None);

    is_drive.then(|| rest.to_string())
}

fn scan_node(path: &Path, context: &Arc<ScanContext>) -> Result<ScanOutcome, ScanError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
    let name = display_name(path);

    if metadata.file_type().is_symlink() {
        return Ok(skipped_outcome(path.to_path_buf(), name, NodeKind::Symlink));
    }

    if metadata.is_file() {
        let size =
            context.counted_file_size(metadata.len(), hardlink_info_from_metadata(&metadata));
        context.track_file(path, size);
        return Ok(file_outcome(path.to_path_buf(), name, size));
    }

    if !metadata.is_dir() {
        return Ok(skipped_outcome(path.to_path_buf(), name, NodeKind::Other));
    }

    scan_directory_node(path, name, 0, context)
}

fn scan_directory_node(
    path: &Path,
    name: String,
    depth: usize,
    context: &Arc<ScanContext>,
) -> Result<ScanOutcome, ScanError> {
    if context.cancel.is_cancelled() {
        return Err(SCAN_CANCELLED.to_string());
    }

    let (child_entries, local_errors) = enumerate_directory_entries(path)?;
    let discovered = child_entries
        .iter()
        .filter(|entry| matches!(entry.kind, NodeKind::Directory))
        .count() as u64;
    if discovered > 0 {
        context.note_discovered(path, discovered);
    }

    let child_results: Vec<_> = child_entries
        .into_par_iter()
        .map(|entry| {
            let context = Arc::clone(context);
            let result = scan_child(entry.clone(), depth + 1, &context);
            (entry, result)
        })
        .collect();

    let mut totals = Totals {
        directories: 1,
        errors: local_errors,
        ..Default::default()
    };
    let mut directory_children = Vec::new();
    let mut file_children = Vec::new();
    let mut size = 0_u64;
    let mut item_count = 1_u64;
    let mut error_count = local_errors;
    let mut skipped_count = 0_u64;

    for (entry, child) in child_results {
        match child {
            Ok(outcome) => {
                size = size.saturating_add(outcome.node.size);
                item_count = item_count.saturating_add(outcome.node.item_count);
                error_count = error_count.saturating_add(outcome.node.error_count);
                skipped_count = skipped_count.saturating_add(outcome.node.skipped_count);
                totals.merge(outcome.totals);
                match outcome.node.kind {
                    NodeKind::Directory => directory_children.push(outcome.node),
                    NodeKind::File => file_children.push(outcome.node),
                    NodeKind::Symlink | NodeKind::Other => {}
                }
            }
            Err(_) => {
                totals.errors = totals.errors.saturating_add(1);
                error_count = error_count.saturating_add(1);
                if matches!(entry.kind, NodeKind::Directory) {
                    context.note_completed(&entry.path);
                }
            }
        }
    }

    let mut children = retain_significant_children(path, depth, directory_children, file_children);
    children.sort_unstable_by(|left, right| {
        right
            .size
            .cmp(&left.size)
            .then_with(|| left.name.cmp(&right.name))
    });

    context.note_completed(path);

    Ok(ScanOutcome {
        node: TreeNode {
            name,
            path: path.to_path_buf(),
            size,
            kind: NodeKind::Directory,
            children,
            item_count,
            error_count,
            skipped_count,
            is_virtual: false,
        },
        totals,
    })
}

fn scan_child(
    entry: ChildEntry,
    depth: usize,
    context: &Arc<ScanContext>,
) -> Result<ScanOutcome, ScanError> {
    match entry.kind {
        NodeKind::Directory => {
            if context.is_outside_scan(&entry.path) {
                context.note_completed(&entry.path);
                return Ok(skipped_outcome(
                    entry.path.clone(),
                    display_name(&entry.path),
                    NodeKind::Other,
                ));
            }

            scan_directory_node(&entry.path, display_name(&entry.path), depth, context)
        }
        NodeKind::File => {
            if context.cancel.is_cancelled() {
                return Err(SCAN_CANCELLED.to_string());
            }

            let (size, hardlink_info) = file_size_and_hardlink_info(&entry)?;
            let counted_size = context.counted_file_size(size, hardlink_info);
            context.track_file(&entry.path, counted_size);
            Ok(file_outcome(
                entry.path.clone(),
                display_name(&entry.path),
                counted_size,
            ))
        }
        NodeKind::Symlink => Ok(skipped_outcome(
            entry.path.clone(),
            display_name(&entry.path),
            NodeKind::Symlink,
        )),
        NodeKind::Other => scan_node(&entry.path, context),
    }
}

/// Where macOS keeps user data. Much of it is also reachable from `/` through firmlinks:
/// `/Users` and `/System/Volumes/Data/Users` are the same directory.
#[cfg(target_os = "macos")]
const DATA_VOLUME: &str = "/System/Volumes/Data";

/// The path at which a directory inside the macOS data volume would appear through a firmlink,
/// for example `/Users` for `/System/Volumes/Data/Users`.
#[cfg(target_os = "macos")]
fn firmlink_alias(path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(DATA_VOLUME).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }

    Some(Path::new("/").join(relative))
}

/// Whether `path` is a data volume directory that a scan of `/` also reaches through a
/// firmlink. Skipping it there keeps a whole-disk scan from counting user data twice.
#[cfg(target_os = "macos")]
fn is_firmlink_duplicate(path: &Path, metadata: &fs::Metadata) -> bool {
    firmlink_alias(path)
        .and_then(|alias| fs::symlink_metadata(alias).ok())
        .is_some_and(|alias| alias.dev() == metadata.dev() && alias.ino() == metadata.ino())
}

fn file_outcome(path: PathBuf, name: String, size: u64) -> ScanOutcome {
    ScanOutcome {
        node: TreeNode {
            name,
            path,
            size,
            kind: NodeKind::File,
            children: Vec::new(),
            item_count: 1,
            error_count: 0,
            skipped_count: 0,
            is_virtual: false,
        },
        totals: Totals {
            files: 1,
            ..Default::default()
        },
    }
}

fn skipped_outcome(path: PathBuf, name: String, kind: NodeKind) -> ScanOutcome {
    ScanOutcome {
        node: TreeNode {
            name,
            path,
            size: 0,
            kind,
            children: Vec::new(),
            item_count: 1,
            error_count: 0,
            skipped_count: 1,
            is_virtual: false,
        },
        totals: Totals {
            skipped: 1,
            ..Default::default()
        },
    }
}

fn enumerate_directory_entries(path: &Path) -> Result<(Vec<ChildEntry>, u64), ScanError> {
    #[cfg(target_os = "macos")]
    if let Ok(entries) = enumerate_directory_entries_macos(path) {
        return Ok(entries);
    }

    enumerate_directory_entries_fallback(path)
}

fn enumerate_directory_entries_fallback(path: &Path) -> Result<(Vec<ChildEntry>, u64), ScanError> {
    let entries = fs::read_dir(path)
        .map_err(|error| format!("Unable to enumerate {}: {error}", path.display()))?;
    let mut child_entries = Vec::new();
    let mut local_errors = 0_u64;

    for entry in entries {
        match entry {
            Ok(entry) => {
                let kind = entry_kind(&entry);
                child_entries.push(ChildEntry {
                    size_hint: entry_size_hint(&entry, kind),
                    kind,
                    path: entry.path(),
                    hardlink_info: None,
                });
            }
            Err(_) => {
                local_errors = local_errors.saturating_add(1);
            }
        }
    }

    Ok((child_entries, local_errors))
}

#[cfg(target_os = "macos")]
fn enumerate_directory_entries_macos(path: &Path) -> Result<(Vec<ChildEntry>, u64), ScanError> {
    let directory = fs::File::open(path)
        .map_err(|error| format!("Unable to open {}: {error}", path.display()))?;
    let mut buffer = vec![0_u8; BULK_BUFFER_SIZE];
    let mut attr_list = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_DEVID
            | ATTR_CMN_ERROR
            | libc::ATTR_CMN_OBJTYPE
            | libc::ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_DATALENGTH,
        forkattr: 0,
    };
    let mut child_entries = Vec::new();
    let mut local_errors = 0_u64;

    loop {
        let entry_count = unsafe {
            libc::getattrlistbulk(
                directory.as_raw_fd(),
                (&mut attr_list as *mut libc::attrlist).cast::<libc::c_void>(),
                buffer.as_mut_ptr().cast::<libc::c_void>(),
                buffer.len(),
                libc::FSOPT_PACK_INVAL_ATTRS as u64,
            )
        };

        if entry_count < 0 {
            return Err(format!(
                "Unable to enumerate {} with getattrlistbulk: {}",
                path.display(),
                io::Error::last_os_error()
            ));
        }

        if entry_count == 0 {
            break;
        }

        let mut offset = 0_usize;
        for _ in 0..entry_count {
            let record_length = read_unaligned_value::<u32>(&buffer, offset)? as usize;
            if record_length < size_of::<u32>() {
                return Err(format!(
                    "getattrlistbulk returned an invalid record for {}",
                    path.display()
                ));
            }

            let record_end = offset
                .checked_add(record_length)
                .ok_or_else(|| format!("Record overflow while enumerating {}", path.display()))?;
            let record = buffer.get(offset..record_end).ok_or_else(|| {
                format!(
                    "getattrlistbulk truncated a record while enumerating {}",
                    path.display()
                )
            })?;
            offset = record_end;

            match parse_bulk_entry(path, record)? {
                BulkEntry::Entry(entry) => child_entries.push(entry),
                BulkEntry::Skipped => {}
                BulkEntry::Errored => {
                    local_errors = local_errors.saturating_add(1);
                }
            }
        }
    }

    Ok((child_entries, local_errors))
}

#[cfg(target_os = "macos")]
enum BulkEntry {
    Entry(ChildEntry),
    Skipped,
    Errored,
}

#[cfg(target_os = "macos")]
fn parse_bulk_entry(parent_path: &Path, record: &[u8]) -> Result<BulkEntry, ScanError> {
    let mut cursor = size_of::<u32>();
    let returned = read_packed_value::<libc::attribute_set_t>(record, &mut cursor)?;
    let entry_error = read_packed_value::<u32>(record, &mut cursor)?;
    let name_offset = cursor;
    let name_reference = read_packed_value::<libc::attrreference_t>(record, &mut cursor)?;
    let device_id = read_packed_value::<libc::dev_t>(record, &mut cursor)?;
    let object_type = read_packed_value::<u32>(record, &mut cursor)?;
    let file_id = read_packed_value::<u64>(record, &mut cursor)?;
    let file_link_count = read_packed_value::<u32>(record, &mut cursor)?;
    let file_size = read_packed_value::<u64>(record, &mut cursor)?;

    if entry_error != 0 {
        return Ok(BulkEntry::Errored);
    }

    if returned.commonattr & libc::ATTR_CMN_NAME == 0 {
        return Err("getattrlistbulk did not return a child name.".to_string());
    }

    let name = decode_attr_name(record, name_offset, name_reference)?;
    if name == "." || name == ".." {
        return Ok(BulkEntry::Skipped);
    }

    let kind = if returned.commonattr & libc::ATTR_CMN_OBJTYPE != 0 {
        macos_node_kind(object_type)
    } else {
        NodeKind::Other
    };
    let size_hint =
        if matches!(kind, NodeKind::File) && returned.fileattr & libc::ATTR_FILE_DATALENGTH != 0 {
            Some(file_size)
        } else {
            None
        };
    let hardlink_info = if matches!(kind, NodeKind::File)
        && returned.commonattr & libc::ATTR_CMN_DEVID != 0
        && returned.commonattr & libc::ATTR_CMN_FILEID != 0
        && returned.fileattr & libc::ATTR_FILE_LINKCOUNT != 0
        && file_link_count > 1
    {
        Some(HardlinkInfo {
            identity: FileIdentity {
                #[cfg(unix)]
                device_id: device_id as u64,
                #[cfg(unix)]
                file_id,
            },
            link_count: file_link_count as u64,
        })
    } else {
        None
    };

    Ok(BulkEntry::Entry(ChildEntry {
        kind,
        path: parent_path.join(name),
        size_hint,
        hardlink_info,
    }))
}

#[cfg(target_os = "macos")]
fn decode_attr_name(
    record: &[u8],
    field_offset: usize,
    reference: libc::attrreference_t,
) -> Result<String, ScanError> {
    if reference.attr_dataoffset < 0 {
        return Err("getattrlistbulk returned a negative name offset.".to_string());
    }

    let start = field_offset
        .checked_add(reference.attr_dataoffset as usize)
        .ok_or_else(|| "getattrlistbulk name offset overflowed.".to_string())?;
    let end = start
        .checked_add(reference.attr_length as usize)
        .ok_or_else(|| "getattrlistbulk name length overflowed.".to_string())?;
    let bytes = record
        .get(start..end)
        .ok_or_else(|| "getattrlistbulk returned a truncated name buffer.".to_string())?;
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);

    Ok(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(target_os = "macos")]
fn macos_node_kind(object_type: u32) -> NodeKind {
    match object_type {
        VTYPE_DIRECTORY => NodeKind::Directory,
        VTYPE_FILE => NodeKind::File,
        VTYPE_SYMLINK => NodeKind::Symlink,
        _ => NodeKind::Other,
    }
}

#[cfg(target_os = "macos")]
fn read_packed_value<T: Copy>(record: &[u8], cursor: &mut usize) -> Result<T, ScanError> {
    let value = read_unaligned_value(record, *cursor)?;
    *cursor = align_to_4(*cursor + size_of::<T>());
    Ok(value)
}

#[cfg(target_os = "macos")]
fn read_unaligned_value<T: Copy>(record: &[u8], offset: usize) -> Result<T, ScanError> {
    let end = offset
        .checked_add(size_of::<T>())
        .ok_or_else(|| "Packed attribute offset overflowed.".to_string())?;
    let bytes = record
        .get(offset..end)
        .ok_or_else(|| "Packed attribute buffer was truncated.".to_string())?;

    Ok(unsafe { ptr::read_unaligned(bytes.as_ptr().cast::<T>()) })
}

#[cfg(target_os = "macos")]
fn align_to_4(offset: usize) -> usize {
    (offset + 3) & !3
}

fn file_size_and_hardlink_info(
    entry: &ChildEntry,
) -> Result<(u64, Option<HardlinkInfo>), ScanError> {
    #[cfg(unix)]
    let need_metadata = entry.size_hint.is_none() || entry.hardlink_info.is_none();
    #[cfg(not(unix))]
    let need_metadata = entry.size_hint.is_none();

    if !need_metadata {
        return Ok((entry.size_hint.unwrap_or(0), entry.hardlink_info));
    }

    let metadata = fs::symlink_metadata(&entry.path)
        .map_err(|error| format!("Unable to read {}: {error}", entry.path.display()))?;

    Ok((
        entry.size_hint.unwrap_or(metadata.len()),
        entry
            .hardlink_info
            .or_else(|| hardlink_info_from_metadata(&metadata)),
    ))
}

#[cfg(unix)]
fn hardlink_info_from_metadata(metadata: &fs::Metadata) -> Option<HardlinkInfo> {
    let link_count = metadata.nlink();
    (link_count > 1).then_some(HardlinkInfo {
        identity: FileIdentity {
            device_id: metadata.dev(),
            file_id: metadata.ino(),
        },
        link_count,
    })
}

#[cfg(not(unix))]
fn hardlink_info_from_metadata(_metadata: &fs::Metadata) -> Option<HardlinkInfo> {
    None
}

/// File size that came with the directory listing, when the platform provides one for free.
fn entry_size_hint(entry: &fs::DirEntry, kind: NodeKind) -> Option<u64> {
    // Windows returns each entry's size while listing the directory, so asking here costs
    // nothing and saves opening every file later. On Unix this would be an extra `stat`.
    #[cfg(windows)]
    if kind == NodeKind::File {
        return entry.metadata().ok().map(|metadata| metadata.len());
    }

    let _ = (entry, kind);
    None
}

fn entry_kind(entry: &fs::DirEntry) -> NodeKind {
    match entry.file_type() {
        Ok(file_type) if file_type.is_dir() => NodeKind::Directory,
        Ok(file_type) if file_type.is_file() => NodeKind::File,
        Ok(file_type) if file_type.is_symlink() => NodeKind::Symlink,
        Ok(_) | Err(_) => NodeKind::Other,
    }
}

fn retain_significant_children(
    parent_path: &Path,
    depth: usize,
    mut directories: Vec<TreeNode>,
    mut files: Vec<TreeNode>,
) -> Vec<TreeNode> {
    if depth >= FILE_DETAIL_DEPTH_LIMIT {
        let omitted_file_count = files.len();
        let omitted_size = files
            .iter()
            .fold(0_u64, |total, node| total.saturating_add(node.size));
        let omitted_items = files
            .iter()
            .fold(0_u64, |total, node| total.saturating_add(node.item_count));
        files.clear();
        if omitted_file_count > 0 {
            files.push(aggregated_files_node(
                parent_path,
                omitted_file_count,
                omitted_size,
                omitted_items,
            ));
        }
    } else if files.len() > MAX_RETAINED_FILE_NODES_PER_DIRECTORY {
        files.sort_unstable_by(|left, right| {
            right
                .size
                .cmp(&left.size)
                .then_with(|| left.name.cmp(&right.name))
        });

        let overflow = files.split_off(MAX_RETAINED_FILE_NODES_PER_DIRECTORY);
        let omitted_file_count = overflow.len();
        let omitted_size = overflow
            .iter()
            .fold(0_u64, |total, node| total.saturating_add(node.size));
        let omitted_items = overflow
            .iter()
            .fold(0_u64, |total, node| total.saturating_add(node.item_count));

        if omitted_file_count > 0 {
            files.push(aggregated_files_node(
                parent_path,
                omitted_file_count,
                omitted_size,
                omitted_items,
            ));
        }
    }

    directories.extend(files);
    directories
}

fn aggregated_files_node(
    parent_path: &Path,
    omitted_file_count: usize,
    omitted_size: u64,
    omitted_items: u64,
) -> TreeNode {
    let name = if omitted_file_count == 1 {
        "1 smaller file".to_string()
    } else {
        format!("{omitted_file_count} smaller files")
    };

    TreeNode {
        name: name.clone(),
        path: parent_path.join(format!("__lorimer_aggregated_files_{omitted_file_count}")),
        size: omitted_size,
        kind: NodeKind::File,
        children: Vec::new(),
        item_count: omitted_items.max(1),
        error_count: 0,
        skipped_count: 0,
        is_virtual: true,
    }
}

fn flatten_snapshot(
    root: TreeNode,
    stats: ScanStats,
    largest_files: Vec<LargestFileEntry>,
) -> ScanSnapshot {
    let mut nodes = Vec::new();
    let mut path_index = HashMap::new();
    let root_id = flatten_node(root, None, &mut nodes, &mut path_index);
    ScanSnapshot {
        root_id,
        nodes,
        path_index,
        largest_files,
        stats,
    }
}

fn flatten_node(
    node: TreeNode,
    parent_id: Option<NodeId>,
    nodes: &mut Vec<ScanNode>,
    path_index: &mut HashMap<PathBuf, NodeId>,
) -> NodeId {
    let node_id = NodeId::from_index(nodes.len());
    let path = node.path.clone();
    nodes.push(ScanNode {
        id: node_id,
        parent_id,
        name: node.name.clone(),
        path: node.path.clone(),
        size: node.size,
        kind: node.kind,
        children: Vec::new(),
        item_count: node.item_count,
        error_count: node.error_count,
        skipped_count: node.skipped_count,
        is_virtual: node.is_virtual,
    });

    let mut child_ids = Vec::with_capacity(node.children.len());
    for child in node.children {
        child_ids.push(flatten_node(child, Some(node_id), nodes, path_index));
    }
    nodes[node_id.index()].children = child_ids;
    path_index.insert(path, node_id);
    node_id
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|segment| segment.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn emit(context: &ScanContext, event: ScanEvent) {
    if let Some(sender) = &context.events {
        let _ = sender.send(event);
    }
}

#[cfg(unix)]
fn allocated_size(metadata: &fs::Metadata) -> Option<u64> {
    Some(metadata.blocks().saturating_mul(512))
}

#[cfg(not(unix))]
fn allocated_size(_metadata: &fs::Metadata) -> Option<u64> {
    None
}

#[cfg(unix)]
fn permissions_string(metadata: &fs::Metadata) -> Option<String> {
    Some(format!("{:o}", metadata.permissions().mode() & 0o777))
}

#[cfg(not(unix))]
fn permissions_string(_metadata: &fs::Metadata) -> Option<String> {
    None
}

fn format_system_time(value: Option<SystemTime>) -> Option<String> {
    value.and_then(|time| {
        let datetime = OffsetDateTime::from(time);
        datetime.format(&Rfc3339).ok()
    })
}

/// Starts Windows Explorer with arguments added by `configure`. Explorer exits with a non-zero
/// status even when it succeeds, so only a failure to start it is reported.
#[cfg(windows)]
fn spawn_explorer(configure: impl FnOnce(&mut Command)) -> Result<(), ScanError> {
    let mut command = Command::new("explorer.exe");
    configure(&mut command);

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Unable to launch Explorer: {error}"))
}

#[cfg(unix)]
fn run_command<I, S>(program: &str, args: I) -> Result<(), ScanError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|error| format!("Unable to launch {program}: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with status {status}."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    #[test]
    fn scan_directory_aggregates_nested_sizes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).expect("nested dir");
        fs::write(temp.path().join("root.bin"), vec![1_u8; 4]).expect("root file");
        fs::write(nested.join("child.bin"), vec![1_u8; 7]).expect("child file");

        let snapshot = ScanService::scan(temp.path().to_path_buf()).expect("scan result");

        assert_eq!(snapshot.stats().total_size, 11);
        assert_eq!(snapshot.stats().file_count, 2);
        assert!(snapshot.stats().directory_count >= 2);
        assert_eq!(snapshot.children(snapshot.root_id()).len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn scan_directory_deduplicates_hardlinked_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        let original = temp.path().join("root.bin");
        let linked = nested.join("linked.bin");
        fs::create_dir(&nested).expect("nested dir");
        fs::write(&original, vec![1_u8; 4]).expect("root file");
        fs::hard_link(&original, &linked).expect("hard link");

        let snapshot = ScanService::scan(temp.path().to_path_buf()).expect("scan result");

        assert_eq!(snapshot.stats().total_size, 4);
        assert_eq!(snapshot.stats().file_count, 2);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_bulk_enumerator_returns_child_kinds_and_sizes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        let root_file = temp.path().join("root.bin");
        fs::create_dir(&nested).expect("nested dir");
        fs::write(&root_file, vec![1_u8; 4]).expect("root file");

        let (entries, local_errors) =
            enumerate_directory_entries_macos(temp.path()).expect("bulk enumerate");

        assert_eq!(local_errors, 0);

        let file_entry = entries
            .iter()
            .find(|entry| entry.path == root_file)
            .expect("file entry");
        assert_eq!(file_entry.kind, NodeKind::File);
        assert_eq!(file_entry.size_hint, Some(4));

        let directory_entry = entries
            .iter()
            .find(|entry| entry.path == nested)
            .expect("directory entry");
        assert_eq!(directory_entry.kind, NodeKind::Directory);
        assert_eq!(directory_entry.size_hint, None);
    }

    #[test]
    fn stream_scan_emits_progress_and_finish() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).expect("nested dir");
        fs::write(temp.path().join("root.bin"), vec![1_u8; 4]).expect("root file");
        fs::write(nested.join("child.bin"), vec![1_u8; 7]).expect("child file");

        let (sender, receiver) = unbounded();
        ScanService::stream(temp.path().to_path_buf(), sender).expect("stream scan");

        let events: Vec<_> = receiver.try_iter().collect();

        assert!(matches!(events.first(), Some(ScanEvent::Started { .. })));
        assert!(events
            .iter()
            .any(|event| matches!(event, ScanEvent::Progress(progress) if progress.discovered_directories >= 2)));
        assert!(matches!(
            events.last(),
            Some(ScanEvent::Finished(snapshot)) if snapshot.stats().total_size == 11
        ));
    }

    #[test]
    fn cancelled_scan_reports_cancellation_instead_of_a_result() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("a/b")).expect("mkdir");
        fs::write(temp.path().join("a/b/file.bin"), vec![0_u8; 32]).expect("write");

        let cancel = ScanCancel::new();
        cancel.cancel();
        let (sender, receiver) = unbounded();

        let result = ScanService::stream_with_cancel(temp.path().to_path_buf(), sender, cancel);
        let events: Vec<_> = receiver.try_iter().collect();

        assert!(result.is_err());
        assert!(matches!(events.last(), Some(ScanEvent::Cancelled)));
        assert!(!events
            .iter()
            .any(|event| matches!(event, ScanEvent::Finished(_) | ScanEvent::Failed(_))));
    }

    #[test]
    fn untriggered_cancel_handle_does_not_affect_a_scan() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("file.bin"), vec![0_u8; 32]).expect("write");

        let cancel = ScanCancel::new();
        let (sender, receiver) = unbounded();

        ScanService::stream_with_cancel(temp.path().to_path_buf(), sender, cancel.clone())
            .expect("scan");

        assert!(!cancel.is_cancelled());
        assert!(matches!(
            receiver.try_iter().last(),
            Some(ScanEvent::Finished(snapshot)) if snapshot.stats().total_size == 32
        ));
    }

    #[test]
    fn cancel_handles_share_one_signal() {
        let cancel = ScanCancel::new();
        let clone = cancel.clone();

        assert!(!clone.is_cancelled());
        cancel.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn scan_directory_compacts_smaller_files_and_tracks_global_largest() {
        let temp = tempfile::tempdir().expect("tempdir");
        for index in 0..150_u8 {
            fs::write(
                temp.path().join(format!("file-{index:03}.bin")),
                vec![1_u8; index as usize + 1],
            )
            .expect("file");
        }

        let snapshot = ScanService::scan(temp.path().to_path_buf()).expect("scan result");
        let root = snapshot.root();

        assert_eq!(snapshot.stats().file_count, 150);
        assert_eq!(
            root.children.len(),
            MAX_RETAINED_FILE_NODES_PER_DIRECTORY + 1
        );
        assert_eq!(snapshot.largest_files().len(), MAX_GLOBAL_LARGEST_FILES);
        assert!(snapshot
            .largest_files()
            .windows(2)
            .all(|pair| pair[0].size >= pair[1].size));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn firmlink_alias_maps_data_volume_paths_to_the_root() {
        assert_eq!(
            firmlink_alias(Path::new("/System/Volumes/Data/Users/alex")),
            Some(PathBuf::from("/Users/alex"))
        );
        assert_eq!(firmlink_alias(Path::new("/System/Volumes/Data")), None);
        assert_eq!(firmlink_alias(Path::new("/Users/alex")), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn data_volume_directories_behind_firmlinks_are_duplicates() {
        let users = Path::new("/System/Volumes/Data/Users");
        let Ok(metadata) = fs::symlink_metadata(users) else {
            // Not a system with a separate data volume.
            return;
        };
        assert!(is_firmlink_duplicate(users, &metadata));

        // A directory that only exists on the data volume is not a duplicate of anything.
        let temp = tempfile::tempdir().expect("tempdir");
        let metadata = fs::symlink_metadata(temp.path()).expect("metadata");
        assert!(!is_firmlink_duplicate(
            &Path::new(DATA_VOLUME).join("lorimer-does-not-exist"),
            &metadata
        ));
    }

    #[cfg(unix)]
    #[test]
    fn directories_on_the_scanned_filesystem_stay_in_the_scan() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).expect("mkdir");
        fs::write(nested.join("file.bin"), vec![0_u8; 64]).expect("write");

        let root = temp.path().canonicalize().expect("canonicalize");
        let metadata = fs::symlink_metadata(&root).expect("metadata");
        let context = ScanContext::new(None, ScanCancel::new(), &root, &metadata);
        assert!(!context.is_outside_scan(&root.join("nested")));

        let snapshot = ScanService::scan(root).expect("scan");
        assert_eq!(snapshot.stats().total_size, 64);
        assert_eq!(snapshot.stats().skipped_count, 0);
    }

    #[cfg(unix)]
    #[test]
    fn directories_on_other_filesystems_are_outside_the_scan() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().canonicalize().expect("canonicalize");
        let metadata = fs::symlink_metadata(&root).expect("metadata");
        let mut context = ScanContext::new(None, ScanCancel::new(), &root, &metadata);

        // Pretend the scan started on a different device than the one the directory is on.
        context.root_device = metadata.dev().wrapping_add(1);

        assert!(context.is_outside_scan(&root));
    }

    #[test]
    fn verbatim_prefixes_are_stripped_from_windows_paths() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\Users\alex").as_deref(),
            Some(r"C:\Users\alex")
        );
        assert_eq!(strip_verbatim_prefix(r"\\?\D:").as_deref(), Some("D:"));
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\folder").as_deref(),
            Some(r"\\server\share\folder")
        );
        // Already in everyday form, or a verbatim path with no everyday equivalent.
        assert_eq!(strip_verbatim_prefix(r"C:\Users\alex"), None);
        assert_eq!(strip_verbatim_prefix(r"\\?\Volume{1234}\folder"), None);
        assert_eq!(strip_verbatim_prefix("/Users/alex"), None);
    }

    #[test]
    fn volume_usage_reports_capacity_for_existing_paths() {
        let temp = tempfile::tempdir().expect("tempdir");

        let usage = VolumeService::usage(temp.path()).expect("volume usage");

        assert!(usage.total_bytes > 0);
        assert!(usage.available_bytes <= usage.total_bytes);
        assert!((0.0..=1.0).contains(&usage.used_fraction()));
    }

    #[test]
    fn volume_usage_fails_for_missing_paths() {
        let temp = tempfile::tempdir().expect("tempdir");

        assert!(VolumeService::usage(&temp.path().join("missing")).is_err());
    }

    #[test]
    fn metadata_for_virtual_nodes_skips_filesystem_lookup() {
        let metadata = MetadataService::read(&MetadataRequest {
            path: PathBuf::from("/tmp/virtual"),
            name: "10 smaller files".to_string(),
            kind: NodeKind::File,
            apparent_size: 42,
            item_count: Some(10),
            error_count: 0,
            skipped_count: 0,
            is_virtual: true,
        })
        .expect("metadata");

        assert!(metadata.is_virtual);
        assert_eq!(metadata.apparent_size, 42);
        assert!(metadata.modified_at.is_none());
    }
}
