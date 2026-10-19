use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Atomically writes `content` to `target_path` by writing to a uniquely named temporary file
/// in the same parent directory, syncing it to disk, and renaming it over `target_path`.
pub fn atomic_write_str(target_path: &Path, content: &str) -> Result<(), String> {
    let parent = target_path
        .parent()
        .ok_or_else(|| format!("Target path has no parent directory: {}", target_path.display()))?;

    if !parent.exists() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create parent directory {}: {e}", parent.display()))?;
    }

    let file_name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("note.md");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let temp_name = format!(".{file_name}.{pid}.{nanos}.tmp");
    let temp_path = parent.join(temp_name);

    let write_res = (|| -> std::io::Result<()> {
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_res {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "Failed to write temporary file {}: {e}",
            temp_path.display()
        ));
    }

    if let Err(e) = fs::rename(&temp_path, target_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "Failed to atomically rename {} to {}: {e}",
            temp_path.display(),
            target_path.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_and_overwrites_file_without_leftover_temp() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("daily").join("2026-10-18.md");

        atomic_write_str(&target, "# First Version\nLine 2\n").expect("first write");
        assert_eq!(
            fs::read_to_string(&target).expect("read first"),
            "# First Version\nLine 2\n"
        );

        atomic_write_str(&target, "# Updated Version\nLine 2\nLine 3\n").expect("second write");
        assert_eq!(
            fs::read_to_string(&target).expect("read second"),
            "# Updated Version\nLine 2\nLine 3\n"
        );

        // Verify no temporary files remain in the directory
        let entries: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "2026-10-18.md");
    }
}
