# Checking background memory use

Compare release builds with the same keyboard, transport, settings and preview
files. Debug builds and different GPU drivers can have different footprints.

On Windows, record both working set (resident pages) and private bytes (committed
private memory). Task Manager's displayed number alone does not establish a leak.
Run this in PowerShell after each scenario has settled for 30 seconds:

```powershell
Get-Process entropy | Select-Object Id,
    @{Name='WorkingSetMiB'; Expression={[math]::Round($_.WorkingSet64 / 1MB, 1)}},
    @{Name='PrivateMiB'; Expression={[math]::Round($_.PrivateMemorySize64 / 1MB, 1)}},
    HandleCount
```

Record the PID so another Entropy instance is not mistaken for the tested build.
Repeat these scenarios on both builds:

1. Start with `entropy.exe --minimized`, with the layout indicator disabled.
   Check keyboard connection and switching layouts between applications before
   opening the main window. The graphics window, renderer and custom GUI fonts
   should not initialize until restore.
2. Restore from the tray, open the display page, play an animation, then hide to
   the tray again. Repeat ten times. Browse several emoji groups between cycles.
   Check that memory settles instead of growing each cycle.
3. Repeat with the layout indicator enabled. Its graphics window requires the
   renderer at startup; the main window should stay hidden.
4. Quit directly after cold tray startup, and quit while a keyboard write is
   pending. Check that Entropy exits after completing its existing write drain.
5. Restore using a second non-minimized invocation. Check that the existing
   instance opens and keeps the keyboard connection.

The preview retains indexed firmware bytes and decodes the current frame on
request. For 56 v4 frames, packed pixels use 2,838,784 bytes versus 15,052,800 bytes
for 56 RGBA frames (plus the old extra first-frame copy). These are storage sizes,
not a measured reduction in process working set.

UI images share a cache capped at 128 textures and 8 MiB, and are released when
the main window is hidden to the tray or reported minimized. Wayland compositors
that do not report minimization keep the bounded cache while the UI can still paint. This excludes the font atlas, native driver allocations
and images outside this cache. Once the UI has opened, hiding it releases cached
images but keeps its graphics context; it does not return to windowless startup.
