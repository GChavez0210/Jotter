//! Moves vault entries to the OS trash without ever deleting them permanently.
//!
//! On Windows, `trash::delete` asks the shell to recycle with `FOF_NO_UI`. When the shell cannot
//! recycle an item (a low-integrity process, a drive without a Recycle Bin, an item larger than
//! the bin's quota), it deletes the item permanently and still reports success. This module runs
//! the same shell operation with a progress sink that cancels any delete the shell would not
//! recycle, so the entry stays in place and the caller gets an error.

use std::path::Path;

/// Why an entry was not moved to the trash.
#[derive(Debug, PartialEq, Eq)]
pub enum TrashError {
    /// The OS would have deleted the entry permanently, so it was left in place.
    NotRecyclable,
    /// The OS reported another failure.
    Failed(String),
}

/// Moves `path` to the OS trash. Returns `TrashError::NotRecyclable`, with `path` untouched, when
/// Windows would delete it permanently instead.
#[cfg(windows)]
pub fn move_to_trash(path: &Path) -> Result<(), TrashError> {
    windows_impl::recycle(path)
}

/// Moves `path` to the OS trash.
#[cfg(not(windows))]
pub fn move_to_trash(path: &Path) -> Result<(), TrashError> {
    trash::delete(path).map_err(|e| TrashError::Failed(e.to_string()))
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Path, Prefix};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use windows::core::{implement, Ref, Result as WinResult, HRESULT, PCWSTR};
    use windows::Win32::Foundation::E_ABORT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
        COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{
        FileOperation, IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl,
        IShellItem, SHCreateItemFromParsingName, FOF_ALLOWUNDO, FOF_NO_UI,
        TSF_DELETE_RECYCLE_IF_POSSIBLE,
    };

    use super::TrashError;

    /// Cancels each delete the shell announces without the recycle flag.
    #[implement(IFileOperationProgressSink)]
    struct RecycleOnly {
        refused: Arc<AtomicBool>,
    }

    impl IFileOperationProgressSink_Impl for RecycleOnly_Impl {
        fn StartOperations(&self) -> WinResult<()> {
            Ok(())
        }
        fn FinishOperations(&self, _hr: HRESULT) -> WinResult<()> {
            Ok(())
        }
        fn PreRenameItem(&self, _f: u32, _i: Ref<IShellItem>, _n: &PCWSTR) -> WinResult<()> {
            Ok(())
        }
        fn PostRenameItem(
            &self,
            _f: u32,
            _i: Ref<IShellItem>,
            _n: &PCWSTR,
            _hr: HRESULT,
            _c: Ref<IShellItem>,
        ) -> WinResult<()> {
            Ok(())
        }
        fn PreMoveItem(
            &self,
            _f: u32,
            _i: Ref<IShellItem>,
            _d: Ref<IShellItem>,
            _n: &PCWSTR,
        ) -> WinResult<()> {
            Ok(())
        }
        fn PostMoveItem(
            &self,
            _f: u32,
            _i: Ref<IShellItem>,
            _d: Ref<IShellItem>,
            _n: &PCWSTR,
            _hr: HRESULT,
            _c: Ref<IShellItem>,
        ) -> WinResult<()> {
            Ok(())
        }
        fn PreCopyItem(
            &self,
            _f: u32,
            _i: Ref<IShellItem>,
            _d: Ref<IShellItem>,
            _n: &PCWSTR,
        ) -> WinResult<()> {
            Ok(())
        }
        fn PostCopyItem(
            &self,
            _f: u32,
            _i: Ref<IShellItem>,
            _d: Ref<IShellItem>,
            _n: &PCWSTR,
            _hr: HRESULT,
            _c: Ref<IShellItem>,
        ) -> WinResult<()> {
            Ok(())
        }
        fn PreDeleteItem(&self, flags: u32, _i: Ref<IShellItem>) -> WinResult<()> {
            if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
                self.refused.store(true, Ordering::SeqCst);
                return Err(E_ABORT.into());
            }
            Ok(())
        }
        fn PostDeleteItem(
            &self,
            _f: u32,
            _i: Ref<IShellItem>,
            _hr: HRESULT,
            _c: Ref<IShellItem>,
        ) -> WinResult<()> {
            Ok(())
        }
        fn PreNewItem(&self, _f: u32, _d: Ref<IShellItem>, _n: &PCWSTR) -> WinResult<()> {
            Ok(())
        }
        fn PostNewItem(
            &self,
            _f: u32,
            _d: Ref<IShellItem>,
            _n: &PCWSTR,
            _t: &PCWSTR,
            _a: u32,
            _hr: HRESULT,
            _i: Ref<IShellItem>,
        ) -> WinResult<()> {
            Ok(())
        }
        fn UpdateProgress(&self, _total: u32, _done: u32) -> WinResult<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> WinResult<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> WinResult<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> WinResult<()> {
            Ok(())
        }
    }

    /// Balances a successful `CoInitializeEx` on this thread.
    struct ComScope(bool);
    impl Drop for ComScope {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// The shell rejects verbatim (`\\?\`) paths, which `canonicalize` returns on Windows.
    fn shell_parsing_name(path: &Path) -> Vec<u16> {
        let mut out = OsString::new();
        let mut components = path.components().peekable();
        if let Some(Component::Prefix(prefix)) = components.peek() {
            match prefix.kind() {
                Prefix::VerbatimDisk(letter) => out.push(format!("{}:", letter as char)),
                Prefix::VerbatimUNC(host, share) => {
                    out.push(r"\\");
                    out.push(host);
                    out.push(r"\");
                    out.push(share);
                }
                _ => out.push(prefix.as_os_str()),
            }
            components.next();
        }
        let mut needs_separator = false;
        for component in components {
            match component {
                Component::RootDir => {
                    out.push(r"\");
                    needs_separator = false;
                }
                other => {
                    if needs_separator {
                        out.push(r"\");
                    }
                    out.push(other.as_os_str());
                    needs_separator = true;
                }
            }
        }
        out.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub fn recycle(path: &Path) -> Result<(), TrashError> {
        let failed = |e: windows::core::Error| TrashError::Failed(e.message());
        let full = std::fs::canonicalize(path).map_err(|e| TrashError::Failed(e.to_string()))?;
        let name = shell_parsing_name(&full);
        let refused = Arc::new(AtomicBool::new(false));

        unsafe {
            // S_FALSE (already initialized) also needs a matching uninitialize. A thread already
            // in the multithreaded apartment returns an error and keeps working as is.
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let _com = ComScope(init.is_ok());

            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(failed)?;
            operation
                .SetOperationFlags(FOF_NO_UI | FOF_ALLOWUNDO)
                .map_err(failed)?;
            let sink: IFileOperationProgressSink = RecycleOnly {
                refused: refused.clone(),
            }
            .into();
            let item: IShellItem =
                SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None).map_err(failed)?;
            operation.DeleteItem(&item, &sink).map_err(failed)?;
            let performed = operation.PerformOperations();
            let aborted = operation
                .GetAnyOperationsAborted()
                .map(|b| b.as_bool())
                .unwrap_or(false);

            if refused.load(Ordering::SeqCst) {
                return Err(TrashError::NotRecyclable);
            }
            performed.map_err(failed)?;
            if aborted {
                return Err(TrashError::Failed(
                    "The shell aborted the operation.".into(),
                ));
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::shell_parsing_name;
        use std::path::Path;

        fn convert(path: &str) -> String {
            let wide = shell_parsing_name(Path::new(path));
            String::from_utf16(&wide[..wide.len() - 1]).unwrap()
        }

        #[test]
        fn verbatim_prefixes_become_shell_paths() {
            assert_eq!(convert(r"\\?\C:\Notes\a.md"), r"C:\Notes\a.md");
            assert_eq!(convert(r"\\?\UNC\host\share\a.md"), r"\\host\share\a.md");
            assert_eq!(convert(r"C:\Notes\a.md"), r"C:\Notes\a.md");
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{move_to_trash, TrashError};

    /// Uses the real Recycle Bin, so it only runs on request. It holds in either environment:
    /// a normal process recycles the file and can restore it, and a process that cannot recycle
    /// (for example, one at low integrity) gets `NotRecyclable` with the file intact. A file that
    /// is gone and absent from the bin fails the test.
    #[test]
    #[ignore = "uses the real Windows Recycle Bin"]
    fn delete_never_removes_a_file_permanently() {
        let dir = tempfile::tempdir().unwrap();
        let name = format!("JotterRecycleCheck-{}.md", std::process::id());
        let file = dir.path().join(&name);
        std::fs::write(&file, "disposable").unwrap();
        let parent = std::fs::canonicalize(dir.path()).unwrap();

        match move_to_trash(&file) {
            Ok(()) => {
                assert!(!file.exists());
                let item = trash::os_limited::list()
                    .unwrap()
                    .into_iter()
                    .find(|item| {
                        item.name.to_string_lossy() == name
                            && std::fs::canonicalize(&item.original_parent).ok().as_deref()
                                == Some(parent.as_path())
                    })
                    .expect("a reported move must leave the file in the Recycle Bin");
                trash::os_limited::restore_all([item]).unwrap();
                assert!(file.exists());
                println!("outcome: recycled and restored");
            }
            Err(TrashError::NotRecyclable) => {
                assert_eq!(std::fs::read_to_string(&file).unwrap(), "disposable");
                println!("outcome: refused, file kept");
            }
            Err(TrashError::Failed(e)) => panic!("unexpected failure: {e}"),
        }
    }
}
