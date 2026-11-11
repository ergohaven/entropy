//! Strict Windows namespace barrier, separate from the normal file-sync/publication step.
//!
//! Integration: call `run_helper_if_requested` as the FIRST main operation and exit
//! immediately for Some(code), before logging, config, GUI, single-instance or HID.
//! Call `flush_published_backup` only after complete file sync and no-clobber publication;
//! retain its returned lease through the actual import SETs. Keep the application's
//! mutation gate closed on every error. Never remove the backup.
//!
//! Trust boundary: trusted executable/dependency deployment and OS/UAC are prerequisites.
//! The nonce is NOT an authenticator on its own. Completion is read from the exact
//! ShellExecuteExW-created process handle, not a PID, file, event or guessed exit zero.
//! A lower-integrity process cannot set an elevated process's exit status using
//! PROCESS_TERMINATE; elevated/admin tampering and compromise of this process are out
//! of scope. Pins deny image writes/deletion and namespace ancestor deletion throughout
//! launch and completion. No handle inheritance or invented cross-UAC handle semantics.
//!
//! Scope: local fixed NTFS, one actual GUID volume, no reparses/mount points/UNC/ADS.
//! API-level FlushFileBuffers guarantee assumes a storage stack honoring flushes;
//! this is not physical power-cut certification. No raw I/O, lock, dismount or IOCTL.
//!
//! Bounds: caller waits at most 120s, including validation and UAC. One outstanding
//! worker/helper per app process; a timed-out attempt keeps the slot and pins until
//! its exact process exits. The helper self-exits at its deadline with a watchdog.
//! Windows can delay ShellExecuteExW/UAC and kernel process teardown indefinitely:
//! there is NO claimed hard kernel-lifetime bound. Such a stuck attempt disables
//! further attempts in this app process rather than spawning unbounded helpers.
//! Native UAC, NTFS share-mode, watchdog/flush and storage qualification are pending.

#[path = "backup_volume_policy.rs"]
mod policy;

#[cfg(windows)]
pub(crate) use win::{flush_published_backup, run_helper_if_requested, BackupNamespaceLease};

// Never route Unix's qualified sync implementation through this module.
#[cfg(not(windows))]
pub(crate) fn run_helper_if_requested() -> Option<i32> {
    None
}

#[cfg(windows)]
mod win {
    use super::policy::{self, Dispatch, Request};
    use std::{
        io, mem,
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr::{null, null_mut},
        sync::mpsc,
        thread,
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
        Storage::FileSystem::*,
        System::{
            Com::{
                CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            },
            SystemInformation::GetTickCount64,
            Threading::{ExitProcess, GetExitCodeProcess, WaitForSingleObject},
            WindowsProgramming::DRIVE_FIXED,
        },
        UI::{
            Shell::{
                ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
                SHELLEXECUTEINFOW,
            },
            WindowsAndMessaging::SW_HIDE,
        },
    };

    fn refuse(s: &str) -> io::Error {
        io::Error::other(s.to_owned())
    }
    fn now() -> u64 {
        unsafe { GetTickCount64() }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    fn nul(a: &[u16]) -> io::Result<Vec<u16>> {
        if a.contains(&0) {
            return Err(refuse("NUL in Windows path/argument"));
        }
        let mut out = a.to_vec();
        out.push(0);
        Ok(out)
    }
    struct Handle(HANDLE);
    impl Handle {
        fn new(h: HANDLE) -> io::Result<Self> {
            if h.is_null() || h == INVALID_HANDLE_VALUE {
                Err(io::Error::last_os_error())
            } else {
                Ok(Self(h))
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    struct Com;
    impl Com {
        fn init() -> io::Result<Self> {
            let r = unsafe {
                CoInitializeEx(
                    null(),
                    (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
                )
            };
            if r < 0 {
                Err(refuse(&format!("COM initialization: {r:#x}")))
            } else {
                Ok(Self)
            }
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    fn open(path: &[u16], access: u32, share: u32) -> io::Result<Handle> {
        let p = nul(path)?;
        Handle::new(unsafe {
            CreateFileW(
                p.as_ptr(),
                access,
                share,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        })
    }
    fn info(h: &Handle) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
        let mut i: BY_HANDLE_FILE_INFORMATION = unsafe { mem::zeroed() };
        if unsafe { GetFileType(h.0) } != FILE_TYPE_DISK
            || unsafe { GetFileInformationByHandle(h.0, &mut i) } == 0
        {
            return Err(refuse("not an inspectable disk filesystem object"));
        }
        if i.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(refuse("reparse point refused"));
        }
        Ok(i)
    }
    fn final_path(h: &Handle) -> io::Result<Vec<u16>> {
        let mut b = vec![0u16; 32768];
        let n = unsafe {
            GetFinalPathNameByHandleW(h.0, b.as_mut_ptr(), b.len() as u32, VOLUME_NAME_GUID)
        } as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n >= b.len() {
            return Err(refuse("filesystem path exceeds bounded buffer"));
        }
        b.truncate(n);
        Ok(b)
    }
    fn volume_of(h: &Handle) -> io::Result<Vec<u16>> {
        let p = final_path(h)?;
        if p.len() < 49 || !policy::volume_valid(&p[..49]) {
            return Err(refuse("not a local GUID volume path"));
        }
        Ok(p[..49].to_vec())
    }
    fn ntfs(root: &Handle, volume: &[u16]) -> io::Result<()> {
        let mut fs = [0u16; 32];
        if unsafe {
            GetVolumeInformationByHandleW(
                root.0,
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let n = fs
            .iter()
            .position(|c| *c == 0)
            .ok_or_else(|| refuse("filesystem name overflow"))?;
        let v = nul(volume)?;
        if fs[..n] != wide("NTFS") || unsafe { GetDriveTypeW(v.as_ptr()) } != DRIVE_FIXED {
            return Err(refuse("strict barrier requires local fixed NTFS"));
        }
        Ok(())
    }
    struct Pins {
        _handles: Vec<Handle>,
        path: Vec<u16>,
        volume: Vec<u16>,
        serial: u32,
        id: u64,
    }
    impl Pins {
        // Open top-down without following reparses. Every ancestor remains non-deletable.
        // Backup/image is additionally non-writable; pre-existing write/delete handles
        // make this acquisition fail under Windows symmetric share-mode checks.
        fn acquire(path: &[u16]) -> io::Result<Self> {
            let path = policy::drive_path(path)
                .ok_or_else(|| refuse("requires strict absolute drive path"))?;
            let mut prefix = wide("\\\\?\\");
            prefix.extend_from_slice(&path[..3]);
            let root = open(
                &prefix,
                policy::pin_access(),
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            )?;
            let ri = info(&root)?;
            if ri.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
                return Err(refuse("volume root not a directory"));
            }
            let volume = volume_of(&root)?;
            ntfs(&root, &volume)?;
            let mut handles = vec![root];
            let mut last = None;
            let parts: Vec<_> = path[3..].split(|c| *c == 92).collect();
            for (index, part) in parts.iter().enumerate() {
                if index != 0 {
                    prefix.push(92);
                }
                prefix.extend_from_slice(part);
                let leaf = index + 1 == parts.len();
                let h = open(
                    &prefix,
                    policy::pin_access(),
                    if leaf {
                        FILE_SHARE_READ
                    } else {
                        FILE_SHARE_READ | FILE_SHARE_WRITE
                    },
                )?;
                let i = info(&h)?;
                if (i.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == leaf {
                    return Err(refuse("unexpected directory/file kind"));
                }
                if volume_of(&h)? != volume {
                    return Err(refuse("cross-volume namespace refused"));
                }
                last = Some(i);
                handles.push(h);
            }
            let i = last.ok_or_else(|| refuse("backup leaf missing"))?;
            Ok(Self {
                _handles: handles,
                path,
                volume,
                serial: i.dwVolumeSerialNumber,
                id: (u64::from(i.nFileIndexHigh) << 32) | u64::from(i.nFileIndexLow),
            })
        }
        fn matches(&self, r: &Request) -> bool {
            self.volume == r.volume && self.serial == r.serial && self.id == r.file_id
        }
    }

    pub(crate) fn run_helper_if_requested() -> Option<i32> {
        let args: Vec<Vec<u16>> = std::env::args_os()
            .skip(1)
            .map(|a| a.encode_wide().collect())
            .collect();
        let r = match policy::parse(&args, now()) {
            Dispatch::Ordinary => return None,
            Dispatch::Invalid => return Some(1),
            Dispatch::Helper(r) => r,
        };
        // No GUI/diagnostic/config/HID initialization is permitted on this branch.
        let deadline = r.deadline;
        if thread::Builder::new()
            .name("backup-volume-watchdog".into())
            .spawn(move || {
                thread::sleep(Duration::from_millis(deadline.saturating_sub(now())));
                unsafe {
                    ExitProcess(1);
                }
            })
            .is_err()
        {
            return Some(1);
        }
        let barrier = || -> io::Result<()> {
            let pins = Pins::acquire(&r.path)?;
            if !pins.matches(&r) || now() >= r.deadline {
                return Err(refuse("backup identity changed or request expired"));
            }
            // Trim only the validated GUID root's trailing slash to open filesystem
            // VOLUME, never a physical disk/device selected from untrusted arguments.
            let volume_path = nul(&pins.volume[..48])?;
            let volume = Handle::new(unsafe {
                CreateFileW(
                    volume_path.as_ptr(),
                    GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    null(),
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                )
            })?;
            if now() >= r.deadline {
                return Err(refuse("volume request expired"));
            }
            if unsafe { FlushFileBuffers(volume.0) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if now() >= r.deadline {
                return Err(refuse("volume flush missed deadline"));
            }
            Ok(())
        };
        Some(if barrier().is_ok() { r.nonce as i32 } else { 1 })
    }

    static ACTIVE: policy::Gate = policy::Gate::new();
    fn nonce() -> io::Result<u32> {
        for _ in 0..16 {
            let mut b = [0u8; 4];
            if unsafe {
                BCryptGenRandom(
                    null_mut(),
                    b.as_mut_ptr(),
                    4,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            } < 0
            {
                return Err(refuse("OS random generation failed"));
            }
            let n = (u32::from_ne_bytes(b) & 0x1fff_ffff) | 0x2000_0000;
            if policy::valid_nonce(n) {
                return Ok(n);
            }
        }
        Err(refuse("no usable random completion nonce"))
    }
    fn arguments(r: &Request) -> io::Result<Vec<u16>> {
        let args = vec![
            wide(policy::FLAG),
            wide(&format!("{:08x}", r.nonce)),
            wide(&format!("{:016x}", r.deadline)),
            wide(&format!("{:08x}", r.serial)),
            wide(&format!("{:016x}", r.file_id)),
            r.volume.clone(),
            r.path.clone(),
        ];
        let mut out = Vec::new();
        for a in args {
            if !out.is_empty() {
                out.push(32);
            }
            out.extend(policy::quote(&a).map_err(refuse)?);
        }
        if out.len() > 30_000 {
            return Err(refuse("helper command line too long"));
        }
        nul(&out)
    }
    fn attempt(path: Vec<u16>, deadline: u64) -> io::Result<Pins> {
        let pins = Pins::acquire(&path)?;
        let exe = std::env::current_exe()?;
        let image = Pins::acquire(&exe.as_os_str().encode_wide().collect::<Vec<_>>())?;
        let request = Request {
            nonce: nonce()?,
            deadline,
            serial: pins.serial,
            file_id: pins.id,
            volume: pins.volume.clone(),
            path: pins.path.clone(),
        };
        let parameters = arguments(&request)?;
        let mut executable = wide("\\\\?\\");
        executable.extend_from_slice(&image.path);
        let executable = nul(&executable)?;
        let verb = nul(&wide("runas"))?;
        let _com = Com::init()?;
        if now() >= deadline {
            return Err(refuse("barrier expired before UAC"));
        }
        let mut launch: SHELLEXECUTEINFOW = unsafe { mem::zeroed() };
        launch.cbSize = mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        launch.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
        launch.lpVerb = verb.as_ptr();
        launch.lpFile = executable.as_ptr();
        launch.lpParameters = parameters.as_ptr();
        launch.nShow = SW_HIDE;
        if unsafe { ShellExecuteExW(&mut launch) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let process = match Handle::new(launch.hProcess) {
            Ok(h) => h,
            // Shell success without a process handle cannot authorize completion
            // or another launch. Keep this one attempt quarantined until app exit.
            Err(_) => loop {
                thread::sleep(Duration::from_millis(1000));
            },
        };
        // Preserve pins/slot after caller timeout. Do NOT assume PROCESS_TERMINATE
        // access on the shell handle or release namespace while helper may be active.
        loop {
            match unsafe { WaitForSingleObject(process.0, 1000) } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT => continue,
                // A failed wait does not prove process death. Quarantine this
                // single attempt and its pins; never free the slot on ambiguity.
                _ => thread::sleep(Duration::from_millis(1000)),
            }
        }
        let mut exit = 0;
        if unsafe { GetExitCodeProcess(process.0, &mut exit) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // Both pin sets deliberately live through exact process completion.
        let _keep = (&pins, &image);
        if !policy::completed(request.nonce, true, exit, now() >= deadline) {
            return Err(refuse(
                "helper did not certify this attempt before deadline",
            ));
        }
        Ok(pins)
    }
    /// Dropping this lease releases worker-owned backup pins after import.
    #[derive(Debug)]
    pub(crate) struct BackupNamespaceLease {
        _release: policy::CompletionLease,
    }

    pub(crate) fn flush_published_backup(path: &Path) -> io::Result<BackupNamespaceLease> {
        let deadline = now()
            .checked_add(policy::BUDGET_MS)
            .ok_or_else(|| refuse("deadline overflow"))?;
        let slot = ACTIVE
            .acquire()
            .ok_or_else(|| refuse("another volume barrier is still outstanding"))?;
        let path = path.as_os_str().encode_wide().collect();
        let (tx, rx) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("backup-volume-barrier".into())
            .spawn(move || {
                let _slot = slot;
                match attempt(path, deadline) {
                    Ok(pins) => {
                        let (lease, released) = policy::completion_lease();
                        if tx
                            .send(Ok(BackupNamespaceLease { _release: lease }))
                            .is_ok()
                        {
                            // OS handles stay on this thread, through the caller's
                            // actual import, not merely through helper completion.
                            let _ = released.recv();
                        }
                        drop(pins);
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error));
                    }
                }
            })?;
        match rx.recv_timeout(Duration::from_millis(deadline.saturating_sub(now()))) {
            Ok(r) if now() < deadline => r,
            Ok(_) => Err(io::Error::new(io::ErrorKind::TimedOut, "volume barrier result expired before delivery")),
            Err(mpsc::RecvTimeoutError::Timeout)=>Err(io::Error::new(io::ErrorKind::TimedOut,"volume barrier deadline expired; backup preserved, further attempts blocked until helper exits")),
            Err(_)=>Err(refuse("volume barrier worker failed")),
        }
    }
}
