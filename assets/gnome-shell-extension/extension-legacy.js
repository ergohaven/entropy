const {Gio, GLib, Meta, Shell} = imports.gi;

const BUS_NAME = 'org.ergohaven.Entropy.Foreground1';
const OBJECT_PATH = '/org/ergohaven/Entropy/Foreground1';
const INTERFACE_XML = `
<node>
  <interface name="org.ergohaven.Entropy.Foreground1">
    <method name="GetProtocolVersion">
      <arg type="u" name="version" direction="out"/>
    </method>
    <method name="GetActiveWindow">
      <arg type="b" name="focused" direction="out"/>
      <arg type="s" name="app_id" direction="out"/>
      <arg type="s" name="wm_class" direction="out"/>
      <arg type="s" name="title" direction="out"/>
      <arg type="u" name="pid" direction="out"/>
    </method>
    <method name="GetOpenWindows">
      <arg type="a(sssu)" name="windows" direction="out"/>
    </method>
    <signal name="ActiveWindowChanged">
      <arg type="b" name="focused"/>
      <arg type="s" name="app_id"/>
      <arg type="s" name="wm_class"/>
      <arg type="s" name="title"/>
      <arg type="u" name="pid"/>
    </signal>
  </interface>
</node>`;

class EntropyForegroundExtension {
    enable() {
        this._tracker = Shell.WindowTracker.get_default();
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE_XML, this);
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._focusSignalId = global.display.connect('notify::focus-window', () => {
            this._trackFocusedWindow();
            this._queueFocusUpdate();
        });
        this._focusAppSignalId = this._tracker.connect(
            'notify::focus-app',
            () => this._queueFocusUpdate()
        );
        this._trackFocusedWindow();
        this._nameId = Gio.DBus.session.own_name(
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            null,
            null
        );
    }

    disable() {
        if (this._idleId) {
            GLib.source_remove(this._idleId);
            this._idleId = 0;
        }
        this._disconnectFocusedWindow();
        if (this._focusAppSignalId) {
            this._tracker.disconnect(this._focusAppSignalId);
            this._focusAppSignalId = 0;
        }
        if (this._focusSignalId) {
            global.display.disconnect(this._focusSignalId);
            this._focusSignalId = 0;
        }
        if (this._nameId) {
            Gio.DBus.session.unown_name(this._nameId);
            this._nameId = 0;
        }
        if (this._dbus)
            this._dbus.unexport();
        this._dbus = null;
        this._tracker = null;
    }

    _disconnectFocusedWindow() {
        if (this._focusTitleSignalId && this._focusedWindow)
            this._focusedWindow.disconnect(this._focusTitleSignalId);
        this._focusTitleSignalId = 0;
        this._focusedWindow = null;
    }

    _trackFocusedWindow() {
        const window = global.display.focus_window;
        if (window === this._focusedWindow)
            return;
        this._disconnectFocusedWindow();
        this._focusedWindow = window;
        if (window) {
            this._focusTitleSignalId = window.connect(
                'notify::title',
                () => this._queueFocusUpdate()
            );
        }
    }

    _queueFocusUpdate() {
        if (this._idleId)
            return;
        this._idleId = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._idleId = 0;
            this._trackFocusedWindow();
            const payload = this.GetActiveWindow();
            const key = JSON.stringify(payload);
            if (key !== this._lastPayloadKey) {
                this._lastPayloadKey = key;
                if (this._dbus) {
                    this._dbus.emit_signal(
                        'ActiveWindowChanged',
                        new GLib.Variant('(bsssu)', payload)
                    );
                }
            }
            return GLib.SOURCE_REMOVE;
        });
    }

    GetActiveWindow() {
        const window = global.display.focus_window;
        if (!window)
            return [false, '', '', '', 0];

        const app = this._tracker ? this._tracker.get_window_app(window) : null;
        return [
            true,
            app ? app.get_id() : '',
            window.get_wm_class() || window.get_wm_class_instance() || '',
            window.get_title() || '',
            Math.max(0, window.get_pid())
        ];
    }

    GetOpenWindows() {
        const allowedTypes = new Set([
            Meta.WindowType.NORMAL,
            Meta.WindowType.DIALOG,
            Meta.WindowType.MODAL_DIALOG,
            Meta.WindowType.UTILITY,
        ]);
        const seen = new Set();
        const windows = [];
        for (const actor of global.get_window_actors()) {
            const window = actor.meta_window;
            if (!window || window.is_skip_taskbar() || !allowedTypes.has(window.get_window_type()))
                continue;
            const app = this._tracker ? this._tracker.get_window_app(window) : null;
            const appId = app ? app.get_id() : '';
            const wmClass = window.get_wm_class() || window.get_wm_class_instance() || '';
            const title = window.get_title() || '';
            const pid = Math.max(0, window.get_pid());
            const key = `${appId}\u0000${wmClass}\u0000${pid}`;
            if ((!appId && !wmClass && !pid) || seen.has(key))
                continue;
            seen.add(key);
            windows.push([appId, wmClass, title, pid]);
        }
        return windows;
    }

    GetProtocolVersion() {
        return 4;
    }
}

function init() {
    return new EntropyForegroundExtension();
}
