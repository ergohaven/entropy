// Loaded once per Entropy process. KWin calls back on every focus or title
// change instead of Entropy loading a fresh script to ask each time.
const SERVICE = "@ENTROPY_BUS_NAME@";
const OBJECT_PATH = "/org/ergohaven/Entropy/KWinForeground1";
const INTERFACE = "org.ergohaven.Entropy.KWinForeground1";

let current = null;

function report() {
    const window = workspace.activeWindow;
    const payload = window ? {
        caption: String(window.caption || ""),
        resourceClass: String(window.resourceClass || ""),
        pid: Number(window.pid) || 0
    } : null;
    callDBus(SERVICE, OBJECT_PATH, INTERFACE, "ActiveWindow", JSON.stringify(payload));
}

function activated(window) {
    if (current) {
        try {
            current.captionChanged.disconnect(report);
        } catch (error) {
            // The previous window may already be gone.
        }
    }
    current = window;
    if (current) {
        current.captionChanged.connect(report);
    }
    report();
}

workspace.windowActivated.connect(activated);
activated(workspace.activeWindow);
