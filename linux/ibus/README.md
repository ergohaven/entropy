# Entropy IBus backend

Wayland does not allow a normal application to globally intercept keys and inject text. This IBus backend is the safe Wayland-native path for Entropy Universal Symbols and Text Expander: it consumes the reserved `F13..F20` transport chords, watches ordinary typing while selected as an input method, and commits matching text through IBus.

## Install for current user

```sh
./linux/ibus/install-user.sh
ibus restart
```

Then add/select Entropy input sources in the system input-source/input-method settings:

- **Entropy EN** for the `us` layout
- **Entropy EN UK** for the `gb` layout
- **Entropy DE** for the `de` layout
- **Entropy FR** for the `fr` layout
- **Entropy ES** for the `es` layout
- **Entropy IT** for the `it` layout
- **Entropy PT** for the `pt` layout
- **Entropy BR** for the `br` layout
- **Entropy RU** for the `ru` layout

Switch between those system input sources to change language while keeping Entropy Text Expander active.

To remove the Entropy IBus sources:

```sh
./linux/ibus/uninstall-user.sh
```

Required distro packages are usually:

- `ibus`
- `python3-gi`
- `gir1.2-ibus-1.0`

## Behavior

- Handles only Entropy transport keys: `F13..F20` with `Shift`, `Ctrl`, `Alt`, and `Super`
- Commits the same Unicode symbols as Entropy Smart Input
- Loads Text Expander settings from `~/.config/entropy/app_settings.json`
- Loads primary and selected extra rules from `~/.config/entropy/text_expander_rules/`
- Passes normal typing through unless a trigger matches
- On match, swallows the last trigger key, removes the already typed trigger text through surrounding-text APIs in GUI clients, uses terminal erase bytes in terminal clients, and commits the replacement
- Does not log keyboard input

## Debug

Universal Symbols debug logging is opt-in. Enable it with:

```sh
mkdir -p ~/.config/entropy
touch ~/.config/entropy/universal_symbols_debug
ibus restart
```

Then reproduce the issue and inspect:

```sh
tail -n 80 ~/.cache/entropy/universal-symbols-debug.log
```

Remove `~/.config/entropy/universal_symbols_debug` and restart IBus to disable it.

## Clipboard fallback

Some Wayland clients can deliver Entropy transport chords to IBus but ignore the
resulting `commit_text` call in app-controlled fields. The Firefox address bar is
one observed example. For targeted testing, Universal Symbols can use a clipboard
paste fallback instead of `commit_text`:

```sh
mkdir -p ~/.config/entropy
touch ~/.config/entropy/universal_symbols_clipboard_fallback
ibus restart
```

This mode temporarily puts only the selected symbol on the text clipboard, sends
`Ctrl+V`, then restores the previous text clipboard value when one was available.
It is intentionally opt-in because non-text clipboard contents cannot be fully
restored through the GTK text clipboard API.

Remove `~/.config/entropy/universal_symbols_clipboard_fallback` and restart IBus
to return to the normal `commit_text` path.

## Scope

This is the IBus backend. Fcitx5 currently handles Universal Symbols only.
