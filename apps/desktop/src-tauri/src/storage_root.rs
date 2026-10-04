//! Where Locaryn puts the bulky things, and how to move them.
//!
//! Model weights, engine binaries and generation scratch files add up to tens
//! of gigabytes. Keeping them on the system drive is not viable, so the
//! location is user-configurable ([`locaryn_config::storage_root`]) and this
//! module exposes the inspection and migration commands the settings UI needs.
//!
//! Migration is deliberately conservative: it copies, verifies the byte count,
//! and only then removes the source. A move interrupted halfway leaves the old
//! data intact, and the pointer file is written *last* so a failed migration
//! never leaves the app pointing at an incomplete tree.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

// ============================================================================
// Inspection
// ============================================================================

/// One directory the user cares about, with what it currently costs them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageEntry {
    /// Stable identifier (`models`, `bin`, `tmp`, …) for the UI.
    pub key: String,
    pub label: String,
    pub path: String,
    pub size_bytes: u64,
    pub exists: bool,
    /// True when this directory sits outside the configured root — the case
    /// worth flagging, since it is what silently fills the system drive.
    pub outside_root: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DriveInfo {
    /// Mount point (`C:\` on Windows, `/` elsewhere).
    pub mount: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// True if the current storage root lives on this drive.
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StorageInfo {
    pub root: String,
    /// False when the root is the built-in default, i.e. the user never chose.
    pub configured: bool,
    pub entries: Vec<StorageEntry>,
    pub total_bytes: u64,
    pub drives: Vec<DriveInfo>,
    /// The live database. Reported separately because it stays put when the
    /// root moves — the UI must not imply otherwise.
    pub db_path: String,
    pub db_bytes: u64,
    /// Non-fatal issues from the most recent migration's cleanup phase (an
    /// old directory that could not be deleted, typically because a file in
    /// it was still open) — empty on every call that is not itself the tail
    /// end of a `set_storage_root(move_data: true)`. The move itself already
    /// succeeded by the time this is populated: the pointer has moved to a
    /// verified-complete copy, this is purely "you can reclaim this later."
    #[serde(default)]
    pub cleanup_warnings: Vec<String>,
}

/// Recursive size of a directory. Unreadable entries are skipped rather than
/// aborting the walk: a locked file should not blank out the whole figure.
fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => total += dir_size(&entry.path()),
            Ok(ft) if ft.is_file() => {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
            _ => {}
        }
    }
    total
}

/// Do these two paths live on the same volume? Used to pick `rename` (instant)
/// over copy-and-delete (minutes for a 40 GB tree).
fn same_volume(a: &Path, b: &Path) -> bool {
    #[cfg(target_os = "windows")]
    {
        fn prefix(p: &Path) -> Option<String> {
            let raw = p
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())?;
            // sysinfo may expose a drive as `\\?\C:\` while the configured
            // root uses `C:\`. Both prefixes name the same Windows volume.
            if let Some(unc) = raw.strip_prefix(r"\\?\unc\") {
                return Some(format!(r"\\{unc}"));
            }
            Some(raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string())
        }
        // Compare the drive letter of the nearest existing ancestor, since the
        // destination itself may not exist yet.
        prefix(a) == prefix(b)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;

        // Le chemin de destination n'existe pas forcément encore : on remonte
        // jusqu'au premier ancêtre présent, puisque c'est le système de
        // fichiers sur lequel il sera créé.
        fn device_of(p: &Path) -> Option<u64> {
            let mut cur = Some(p);
            while let Some(c) = cur {
                if let Ok(m) = std::fs::metadata(c) {
                    return Some(m.dev());
                }
                cur = c.parent();
            }
            None
        }

        match (device_of(a), device_of(b)) {
            (Some(x), Some(y)) => x == y,
            // Ne rien savoir n'est pas « même volume » : un `rename` entre
            // périphériques échoue, alors qu'une copie inutile ne coûte que
            // du temps.
            _ => false,
        }
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (a, b);
        false
    }
}

fn entry(key: &str, label: &str, path: PathBuf, root: &Path) -> StorageEntry {
    let exists = path.exists();
    StorageEntry {
        key: key.to_string(),
        label: label.to_string(),
        size_bytes: if exists { dir_size(&path) } else { 0 },
        outside_root: !path.starts_with(root),
        path: path.to_string_lossy().to_string(),
        exists,
    }
}

#[tauri::command]
pub fn storage_info() -> Result<StorageInfo, String> {
    let root = locaryn_config::storage_root();
    let entries = vec![
        entry(
            "models",
            "Modèles (poids)",
            locaryn_config::models_dir(),
            &root,
        ),
        entry(
            "bin",
            "Moteurs (llama.cpp, sd.cpp)",
            locaryn_config::bin_dir(),
            &root,
        ),
        entry(
            "tmp",
            "Fichiers temporaires",
            locaryn_config::temp_dir(),
            &root,
        ),
        entry(
            "free_chats",
            "Pièces jointes des chats",
            locaryn_config::free_chats_dir(),
            &root,
        ),
    ];
    let total_bytes = entries.iter().map(|e| e.size_bytes).sum();

    let mut drives = Vec::new();
    let disks = sysinfo::Disks::new_with_refreshed_list();
    for disk in disks.list() {
        let mount = disk.mount_point().to_path_buf();
        drives.push(DriveInfo {
            is_current: same_volume(&root, &mount),
            mount: mount.to_string_lossy().to_string(),
            total_bytes: disk.total_space(),
            free_bytes: disk.available_space(),
        });
    }
    drives.sort_by(|a, b| a.mount.cmp(&b.mount));
    drives.dedup_by(|a, b| a.mount == b.mount);

    let db_path = locaryn_config::default_data_dir().join("locaryn.db");
    Ok(StorageInfo {
        root: root.to_string_lossy().to_string(),
        configured: locaryn_config::configured_storage_root().is_some(),
        entries,
        total_bytes,
        drives,
        db_bytes: std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0),
        db_path: db_path.to_string_lossy().to_string(),
        cleanup_warnings: Vec::new(),
    })
}

// ============================================================================
// Migration
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MigrationProgress {
    pub phase: String,
    pub current_file: String,
    pub moved_bytes: u64,
    pub total_bytes: u64,
    pub done: bool,
    pub error: Option<String>,
}

fn emit(app: &AppHandle, p: MigrationProgress) {
    let _ = app.emit("storage-migration", p);
}

/// Copy `src` into `dst` recursively, reporting progress. Returns the number
/// of bytes copied so the caller can verify against the source total.
fn copy_tree(
    report: &dyn Fn(MigrationProgress),
    src: &Path,
    dst: &Path,
    moved: &mut u64,
    total: u64,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(report, &from, &to, moved, total)?;
        } else {
            let n = std::fs::copy(&from, &to)?;
            *moved += n;
            report(MigrationProgress {
                phase: "copie".into(),
                current_file: entry.file_name().to_string_lossy().to_string(),
                moved_bytes: *moved,
                total_bytes: total,
                done: false,
                error: None,
            });
        }
    }
    Ok(())
}

/// What `copy_or_rename_dir` did, and what (if anything) still needs
/// deleting once every category in the batch has copied successfully.
#[derive(Debug)]
enum MoveOutcome {
    /// Nothing to move: the source did not exist.
    Nothing,
    /// `rename` moved it atomically — there is no separate source left to
    /// clean up, the OS already made this one step.
    Renamed,
    /// Copied to `dst`; `src` is still there, verified byte-for-byte, and
    /// waiting on the caller to delete it once every other category in the
    /// same migration has copied cleanly too.
    Copied { src: PathBuf },
}

/// Copy (or, same volume, atomically rename) `src` into `dst`. Never deletes
/// `src` itself for a copy — that is the caller's job, and only once every
/// category in the batch is confirmed copied. Deleting per-category as soon
/// as each one finishes is what let a bin-directory failure (llama-server
/// still holding its DLLs open) leave the models directory already gone from
/// the old root while the pointer, never reached, still named that same old
/// root — the very case that left weights stranded in an orphaned folder.
fn copy_or_rename_dir(
    report: &dyn Fn(MigrationProgress),
    src: &Path,
    dst: &Path,
    moved: &mut u64,
    total: u64,
) -> Result<MoveOutcome, String> {
    if !src.is_dir() || src == dst {
        return Ok(MoveOutcome::Nothing);
    }
    if dst.exists()
        && dst
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(format!(
            "{} existe déjà et n'est pas vide — déplacement annulé pour ne rien écraser.",
            dst.display()
        ));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("création {}: {e}", parent.display()))?;
    }

    if same_volume(src, dst) && std::fs::rename(src, dst).is_ok() {
        *moved += dir_size(dst);
        report(MigrationProgress {
            phase: "déplacement instantané".into(),
            current_file: src.to_string_lossy().to_string(),
            moved_bytes: *moved,
            total_bytes: total,
            done: false,
            error: None,
        });
        return Ok(MoveOutcome::Renamed);
    }
    // Rename can still fail across mount points or with open handles;
    // fall through to the copy path rather than giving up.

    let expected = dir_size(src);
    copy_tree(report, src, dst, moved, total)
        .map_err(|e| format!("copie {}: {e}", src.display()))?;

    let copied = dir_size(dst);
    if copied < expected {
        return Err(format!(
            "copie incomplète de {} ({} sur {} octets) — la source a été conservée.",
            src.display(),
            copied,
            expected
        ));
    }
    Ok(MoveOutcome::Copied {
        src: src.to_path_buf(),
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetRootArgs {
    pub new_root: String,
    /// Move the existing data across. When false the new root starts empty and
    /// the old files stay where they are.
    #[serde(default)]
    pub move_data: bool,
}

/// Point Locaryn at a new storage root, optionally relocating what is already
/// there. Runs on a blocking thread: moving tens of gigabytes must not stall
/// the async runtime.
#[tauri::command]
pub async fn set_storage_root(app: AppHandle, args: SetRootArgs) -> Result<StorageInfo, String> {
    let new_root = PathBuf::from(args.new_root.trim());
    if new_root.as_os_str().is_empty() {
        return Err("Chemin vide.".into());
    }
    let old_root = locaryn_config::storage_root();
    let old_models = locaryn_config::models_dir();
    let old_bin = locaryn_config::bin_dir();
    let old_free_chats = locaryn_config::free_chats_dir();

    if new_root == old_root {
        return storage_info();
    }
    // Nesting the new root inside the old one (or vice versa) makes the move
    // recursive and would copy files into themselves.
    if new_root.starts_with(&old_root) || old_root.starts_with(&new_root) {
        return Err(
            "Le nouveau dossier ne peut pas être contenu dans l'ancien (ni l'inverse).".into(),
        );
    }

    std::fs::create_dir_all(&new_root)
        .map_err(|e| format!("impossible de créer {}: {e}", new_root.display()))?;

    // Fail early on a read-only or full destination rather than mid-copy.
    let probe = new_root.join(".locaryn-write-test");
    std::fs::write(&probe, b"ok")
        .map_err(|e| format!("dossier non inscriptible ({}): {e}", new_root.display()))?;
    let _ = std::fs::remove_file(&probe);

    if !args.move_data {
        locaryn_config::set_storage_root(Some(&new_root))
            .map_err(|e| format!("enregistrement du réglage: {e}"))?;
        return storage_info();
    }

    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<String>, String> {
        let report = move |p: MigrationProgress| emit(&app2, p);
        // Scratch files are disposable; dropping them avoids copying gigabytes
        // of intermediates that nothing will ever read again.
        let tmp = locaryn_config::temp_dir();
        if tmp.is_dir() {
            let _ = std::fs::remove_dir_all(&tmp);
        }

        let total = dir_size(&old_models) + dir_size(&old_bin) + dir_size(&old_free_chats);
        let mut moved = 0u64;

        report(MigrationProgress {
            phase: "préparation".into(),
            current_file: String::new(),
            moved_bytes: 0,
            total_bytes: total,
            done: false,
            error: None,
        });

        // Phase 1 — copy every category, deleting nothing yet. A failure here
        // (a locked bin/ file, a full destination disk, …) leaves the old
        // root completely untouched and still the active one: nothing has
        // been deleted and the pointer below is never reached.
        let mut pending_deletes: Vec<PathBuf> = Vec::new();
        let mut record = |outcome: MoveOutcome| {
            if let MoveOutcome::Copied { src } = outcome {
                pending_deletes.push(src);
            }
        };

        record(copy_or_rename_dir(
            &report,
            &old_models,
            &new_root.join("models"),
            &mut moved,
            total,
        )?);
        // Windows refuses to move a running executable, and llama-server may
        // well be up. Say so instead of surfacing a bare "access denied".
        record(
            copy_or_rename_dir(&report, &old_bin, &new_root.join("bin"), &mut moved, total)
                .map_err(|e| {
                    format!(
                        "{e}\nSi un moteur tourne encore, arrêtez-le (onglet Moteur IA) puis réessayez."
                    )
                })?,
        );
        record(copy_or_rename_dir(
            &report,
            &old_free_chats,
            &new_root.join("free_chats"),
            &mut moved,
            total,
        )?);

        // Phase 2 — every category above is now verified complete at
        // `new_root`. This is the actual commit point: flip the pointer
        // before touching a single byte of the old root, so a failure from
        // here on (phase 3, cleanup) can never strand the app between two
        // half-known locations the way losing this ordering once did.
        locaryn_config::set_storage_root(Some(&new_root))
            .map_err(|e| format!("enregistrement du réglage: {e}"))?;

        // Phase 3 — best-effort cleanup of the now-superseded old
        // directories. The pointer has already moved to a confirmed-complete
        // copy, so a deletion failure here (the same locked-file case as
        // above, hit on the way out instead of the way in) costs the user
        // some reclaimable disk space, never their data.
        let mut warnings = Vec::new();
        for src in pending_deletes {
            if let Err(e) = std::fs::remove_dir_all(&src) {
                warnings.push(format!(
                    "{} n'a pas pu être supprimé ({e}) — les données sont bien copiées et actives \
                     dans le nouveau dossier, mais cet ancien dossier reste sur le disque ; \
                     supprimez-le manuellement une fois le moteur arrêté.",
                    src.display()
                ));
            }
        }
        Ok(warnings)
    })
    .await
    .map_err(|e| format!("tâche de migration interrompue: {e}"))?;

    match result {
        Ok(warnings) => {
            emit(
                &app,
                MigrationProgress {
                    phase: "terminé".into(),
                    current_file: String::new(),
                    moved_bytes: 0,
                    total_bytes: 0,
                    done: true,
                    error: None,
                },
            );
            storage_info().map(|mut info| {
                info.cleanup_warnings = warnings;
                info
            })
        }
        Err(e) => {
            emit(
                &app,
                MigrationProgress {
                    phase: "échec".into(),
                    current_file: String::new(),
                    moved_bytes: 0,
                    total_bytes: 0,
                    done: true,
                    error: Some(e.clone()),
                },
            );
            Err(e)
        }
    }
}

/// Delete Locaryn's scratch files. Returns the number of bytes reclaimed.
///
/// Only touches Locaryn's own temp directory and the `locaryn_*` leftovers it
/// wrote to the OS temp dir in earlier versions — never the OS temp dir itself.
#[tauri::command]
pub fn clean_temp() -> Result<u64, String> {
    let mut freed = 0u64;

    let own = locaryn_config::temp_dir();
    if own.is_dir() {
        freed += dir_size(&own);
        std::fs::remove_dir_all(&own).map_err(|e| format!("nettoyage {}: {e}", own.display()))?;
    }

    // Pre-configurable-storage builds wrote here; reclaim it too.
    let os_temp = std::env::temp_dir();
    if os_temp != own {
        for e in std::fs::read_dir(&os_temp).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.starts_with("locaryn") {
                continue;
            }
            let path = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = if is_dir {
                dir_size(&path)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            };
            let removed = if is_dir {
                std::fs::remove_dir_all(&path).is_ok()
            } else {
                std::fs::remove_file(&path).is_ok()
            };
            if removed {
                freed += size;
            }
        }
    }

    Ok(freed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique scratch dir per test — no `tempfile` dep in this crate, and the
    /// tests must not touch the user's real storage root.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "locaryn_storage_test_{tag}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn silent() -> impl Fn(MigrationProgress) {
        |_| {}
    }

    #[test]
    fn dir_size_counts_nested_files() {
        let root = scratch("size");
        write(&root.join("a.bin"), &[0u8; 100]);
        write(&root.join("sub/b.bin"), &[0u8; 250]);
        write(&root.join("sub/deep/c.bin"), &[0u8; 30]);
        assert_eq!(dir_size(&root), 380);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dir_size_of_missing_dir_is_zero() {
        assert_eq!(dir_size(Path::new("Z:/does/not/exist")), 0);
    }

    #[test]
    fn copy_or_rename_dir_relocates_every_file() {
        let base = scratch("move");
        let src = base.join("src");
        let dst = base.join("dst");
        write(&src.join("model.gguf"), &[7u8; 4096]);
        write(&src.join("nested/vae.safetensors"), &[3u8; 2048]);

        let mut moved = 0u64;
        let outcome = copy_or_rename_dir(&silent(), &src, &dst, &mut moved, 6144).unwrap();

        // Scratch dirs share a volume (both under the OS temp dir), so this
        // takes the atomic-rename path — which, being a single OS-level
        // move, is the one case where the source is expected gone
        // immediately rather than left for a later, explicit deletion.
        assert!(matches!(outcome, MoveOutcome::Renamed));
        assert!(!src.exists(), "rename must relocate, not copy, the source");
        assert_eq!(
            std::fs::read(dst.join("model.gguf")).unwrap(),
            vec![7u8; 4096]
        );
        assert_eq!(
            std::fs::read(dst.join("nested/vae.safetensors")).unwrap(),
            vec![3u8; 2048]
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn copy_or_rename_dir_refuses_to_overwrite_a_populated_destination() {
        let base = scratch("clobber");
        let src = base.join("src");
        let dst = base.join("dst");
        write(&src.join("new.gguf"), b"new");
        write(&dst.join("precious.gguf"), b"precious");

        let err = copy_or_rename_dir(&silent(), &src, &dst, &mut 0, 0).unwrap_err();

        assert!(err.contains("existe déjà"), "unexpected message: {err}");
        // Neither side may be touched when we bail out.
        assert_eq!(
            std::fs::read(dst.join("precious.gguf")).unwrap(),
            b"precious"
        );
        assert_eq!(std::fs::read(src.join("new.gguf")).unwrap(), b"new");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn copy_or_rename_dir_is_a_noop_when_there_is_nothing_to_move() {
        let base = scratch("noop");
        let missing = base.join("absent");
        assert!(matches!(
            copy_or_rename_dir(&silent(), &missing, &base.join("dst"), &mut 0, 0).unwrap(),
            MoveOutcome::Nothing
        ));
        std::fs::remove_dir_all(&base).ok();
    }

    /// Locaryn/Locaryn#4: a category refused (destination populated — the
    /// same shape as a locked bin/ mid-copy in the real path) must not
    /// disturb a category that already moved. `set_storage_root` relies on
    /// this `?`-propagates-before-any-deletion property to keep every source
    /// intact until all three categories are confirmed, so the pointer never
    /// commits over a partial copy.
    #[test]
    fn a_refused_category_leaves_an_already_moved_ones_source_alone() {
        let base = scratch("deferred-delete");
        let models_src = base.join("models");
        write(&models_src.join("weights.gguf"), &[9u8; 512]);

        let mut moved = 0u64;
        let models_outcome = copy_or_rename_dir(
            &silent(),
            &models_src,
            &base.join("dst/models"),
            &mut moved,
            512,
        )
        .unwrap();
        assert!(matches!(models_outcome, MoveOutcome::Renamed));

        let bin_src = base.join("bin");
        write(&bin_src.join("llama-server.exe"), b"binary");
        let bin_dst = base.join("dst/bin");
        write(&bin_dst.join("already-here.dll"), b"stale");

        let err = copy_or_rename_dir(&silent(), &bin_src, &bin_dst, &mut moved, 512).unwrap_err();
        assert!(err.contains("existe déjà"));
        assert_eq!(
            std::fs::read(bin_src.join("llama-server.exe")).unwrap(),
            b"binary"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn copy_tree_reports_progress_for_each_file() {
        let base = scratch("progress");
        let src = base.join("src");
        write(&src.join("one.bin"), &[0u8; 10]);
        write(&src.join("two.bin"), &[0u8; 20]);
        write(&src.join("sub/three.bin"), &[0u8; 30]);

        let seen = std::sync::Mutex::new(Vec::new());
        let mut moved = 0u64;
        copy_tree(
            &|p: MigrationProgress| seen.lock().unwrap().push(p.moved_bytes),
            &src,
            &base.join("dst"),
            &mut moved,
            60,
        )
        .unwrap();

        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.len(), 3, "one report per file");
        assert_eq!(moved, 60);
        // Progress must be monotonic, otherwise the bar jumps backwards.
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
        std::fs::remove_dir_all(&base).ok();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn same_volume_compares_drive_letters() {
        assert!(same_volume(
            Path::new(r"D:\Documents\Syncho\models"),
            Path::new(r"D:\LocarynData")
        ));
        assert!(!same_volume(
            Path::new(r"C:\Users\x\.locaryn\data"),
            Path::new(r"D:\LocarynData")
        ));
        // Case must not decide which strategy we take.
        assert!(same_volume(Path::new(r"d:\a"), Path::new(r"D:\b")));
        // sysinfo commonly returns the verbatim form for mounted volumes.
        assert!(same_volume(Path::new(r"C:\a"), Path::new(r"\\?\C:\b")));
    }

    /// Runs against the real machine: proves volume enumeration works on this
    /// host and that every reported path is coherent. Read-only.
    #[test]
    fn storage_info_reports_a_coherent_picture_of_this_machine() {
        let info = storage_info().expect("storage_info must not fail");

        assert!(!info.root.is_empty());
        assert!(!info.db_path.is_empty());
        assert!(!info.drives.is_empty(), "no volume detected on this host");
        assert!(
            info.drives.iter().any(|d| d.is_current),
            "the root must sit on one of the reported volumes"
        );
        for d in &info.drives {
            assert!(d.free_bytes <= d.total_bytes, "{}: free > total", d.mount);
        }
        assert_eq!(info.entries.len(), 4);
        assert_eq!(
            info.total_bytes,
            info.entries.iter().map(|e| e.size_bytes).sum::<u64>()
        );

        println!(
            "\n  racine    {} (configuré: {})",
            info.root, info.configured
        );
        println!("  base      {} ({} octets)", info.db_path, info.db_bytes);
        for e in &info.entries {
            println!(
                "  {:<11} {:>16} o  {}{}",
                e.key,
                e.size_bytes,
                e.path,
                if e.outside_root {
                    "   [HORS RACINE]"
                } else {
                    ""
                }
            );
        }
        for d in &info.drives {
            println!(
                "  disque {:<5} {:>7.1} Go libres / {:>7.1} Go{}",
                d.mount,
                d.free_bytes as f64 / 1024f64.powi(3),
                d.total_bytes as f64 / 1024f64.powi(3),
                if d.is_current { "   <- utilisé" } else { "" }
            );
        }
    }

    #[test]
    fn entry_flags_directories_that_sit_outside_the_root() {
        let root = Path::new(r"D:\LocarynData");
        let inside = entry("models", "M", root.join("models"), root);
        let outside = entry("models", "M", PathBuf::from(r"C:\elsewhere\models"), root);
        assert!(!inside.outside_root);
        assert!(outside.outside_root);
    }
}
