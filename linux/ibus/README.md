# Entropy IBus backend

Entropy uses this IBus backend for Text Expander on Linux. While selected as an input method, it watches typing and commits matching expansions through IBus; this is also the safe input path required by Wayland.

## Install for current user

```sh
./linux/ibus/install-user.sh
ibus restart
```

Then add/select Entropy input sources in the system input-source/input-method settings:

- **Entropy Text Expander EN** for the `us` layout
- **Entropy Text Expander EN UK** for the `gb` layout
- **Entropy Text Expander DE** for the `de` layout
- **Entropy Text Expander FR** for the `fr` layout
- **Entropy Text Expander ES** for the `es` layout
- **Entropy Text Expander IT** for the `it` layout
- **Entropy Text Expander PT** for the `pt` layout
- **Entropy Text Expander BR** for the `br` layout
- **Entropy Text Expander RU** for the `ru` layout

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

- Loads Text Expander settings from `~/.config/entropy/app_settings.json`
- Loads primary and selected extra rules from `~/.config/entropy/text_expander_rules/`
- Passes normal typing through unless a trigger matches
- On match, swallows the last trigger key, removes the already typed trigger text through surrounding-text APIs in GUI clients, uses terminal erase bytes in terminal clients, and commits the replacement
- Does not log keyboard input

## Scope

This is the Linux backend for Text Expander. Universal Symbols are implemented by supported RMK firmware and do not use IBus.
