# Entropy design language

This document defines the shared design rules for Entropy's Settings and Advanced pages. Read it before changing the interface. It belongs in the repository so contributors do not need a local agent skill to find the product's design rules.

## Shared owners

Use the existing owners instead of recreating controls or layout rules inside a page:

- [src/ui_style.rs](src/ui_style.rs): shared controls, typography, colors, responsive metrics and navigation-to-content spacing
- [src/ui/layout_chrome.rs](src/ui/layout_chrome.rs): the main navigation and entry into the settings shell
- [src/ui/settings_shell.rs](src/ui/settings_shell.rs): the common content rectangle and page routing
- [src/ui/settings_viewport.rs](src/ui/settings_viewport.rs): adaptive settings lists, visible rows, scrollbar geometry and fixed action bars
- [src/ui/sm62_scroll.rs](src/ui/sm62_scroll.rs): the SM62 preview/settings group and its floating scrollbar

Keep design rules and their owning implementation consistent. Change a shared rule at its owner; do not compensate for it with offsets, copied painters or alternate state in individual pages. A scoped change is not permission to redesign unrelated pages or migrate every existing control.

## Page origin and navigation

All Config and Advanced pages receive their vertical origin through the common shell. `settings_page_content_top(navigation_top, navigation_height)` in `src/ui_style.rs` places that origin below the visible main navigation, with an 8-point gap.

- Do not reserve space for the Layout tab's layer bar, battery information or application-layout selector on settings pages where those controls are not shown
- Do not make the settings origin depend on application-layout support or whether automatic switching is enabled
- Adjust the common navigation-to-content rule to move all settings pages; do not tune each page separately
- Preserve the Layout tab's keyboard geometry and its actual control reservations when changing settings spacing
- Keep bottom hints and page actions anchored to their existing viewport, rather than moving them with scrolling content

The content origin is not the top of the title's visible glyphs: a page can retain its established internal header layout. The shared origin is the control for moving the entire settings area closer to the navigation.

## Titles and descriptions

Use `settings_section_header(ui, title, description)` for the shared title/description presentation. SM62's Display and Shared display settings sections use this component.

Baseline typography and spacing:

- Title: 18 points, bold
- Description: 13 points, using the shared `muted_text` color
- Gap between title and description: 6 points
- Gap before the controls: 24 points

Titles and descriptions do not end with periods. Prefer one sentence or a semicolon to period-separated sentences in a short description. Keep the source copy clean; the shared component also strips terminal periods as a safeguard.

The parent layout owns horizontal alignment. Center a header within the column it describes, not within a wider neighboring preview-and-settings group. Preserve meaningful title hover explanations when replacing an older header implementation.

## Settings lists and responsive sizing

Use `ResponsiveMetrics` rather than copying numeric sizes into a page. Baseline values are logical egui points, not physical pixels; shared responsive sizing and application zoom determine the rendered size.

The standard settings list has:

- A 470-point outer width and a 452-point row-content width
- 54-point rows
- Labels on the left and controls aligned to a common right edge
- Thin graphite separators
- A reserved scrollbar gutter, so the list does not shift when scrolling becomes necessary

Use `settings_list_row_with_tooltip` for labeled settings. Labels must stay within their own column. Long translations can wrap over two lines, but must not overlap the control or paint beyond the row. Do not stretch a bounded settings list across a wide window just to fill space.

## Controls

### Dropdowns

Use `modern_dropdown_button`, `modern_dropdown_button_sized`, `modern_dropdown_select_sized` and the existing popup-option helpers. Do not introduce default `egui::ComboBox` controls or page-local dropdown painters.

For a standard settings dropdown, use:

- `ResponsiveMetrics::settings_control_width()`: 168 points at baseline
- `ResponsiveMetrics::settings_control_height()`: 32 points at baseline
- `ResponsiveMetrics::settings_control_font_size()`: 12.5 points at baseline

Use the same metrics for the trigger and its option text. Clamp width only when actual available space requires it. An intentionally different existing field width is not a reason to create a second size system or silently redesign another page.

SM62's Half, Default mode, standby delay, Layout, layout duration and display timeout dropdowns all use these shared metrics. Do not restore the former page-local 196-by-34 dimensions or add a separate SM62 size helper. A progress/status allowance beside a field is not part of the dropdown's width; reserve that space without shifting the trigger's right edge.

### Switches, buttons and text fields

- Reuse the shared `settings_switch` family, `modern_button` and `modern_text_field`
- Keep ordinary switch states neutral grayscale; green is reserved for an explicit success state, not a general enabled state
- Use existing enabled, disabled, hover, focus and pending-write treatments rather than assigning new page-specific colors
- Do not use default framed `TextEdit` widgets where a shared modern field applies
- In paired actions, place the backward or negative action before the forward or positive action

### Colors

Use the shared palette functions in `src/ui_style.rs`, including `surface_fill`, `hover_fill`, `border_color`, `muted_text` and `modal_outline_stroke`. Preserve both light and dark themes. Do not replace these owners with copied RGB values in a page.

## Hover explanations and copy

Every left-side settings label needs a meaningful explanation through `settings_list_row_with_tooltip`. A tooltip attached only to the dropdown or switch does not explain the label when the user hovers its text.

- Explain the practical effect in plain language, rather than repeating the label or exposing protocol terminology
- State whether an edit affects the selected half or both halves when that distinction matters
- Describe important exceptions and dependent controls without turning the tooltip into an implementation report
- Do not end tooltips with periods
- Preserve clear explanations on section headings where they are provided
- Use the selected interface language consistently and allow for longer translations
- Use the modifier name appropriate to the target OS; do not show combined strings such as `Win/Cmd/Super`

Displaying an input-language indicator is not the same as changing the computer's input language. Copy must make this distinction clear.

## Scrolling and actions

Use the adaptive settings viewport where it applies, and render only the visible rows. Keep page-level action buttons outside scrolling content, using the existing fixed action-bar geometry.

Scrollbars use the shared floating handle with a reserved gutter. Do not add a permanent framed scrollbar or reclaim its gutter when the handle is hidden. Preserve pointer, wheel and popup interaction when changing layout.

## SM62 display page

Keep the display preview and settings as one bounded responsive group.

- In the wide layout, Shared display settings uses the same settings column as the selected-half controls above it
- Align labels, separators and right-hand controls across those sections; center the shared header inside that settings column
- Derive the alignment from the existing preview width and panel gap in `src/ui/sm62_scroll.rs`, not from independent offsets for individual rows
- In the stacked narrow layout, retain a single centered settings column beneath the preview
- Preserve preview geometry, scrolling and the distinction between selected-half and shared edits

Off is a choice in the selected half's Default mode dropdown, backed by the existing per-half display-power setting. It is not a new stored firmware mode. Keep the saved scene and brightness intact while the display is off, and expose the choice only when the firmware supports it. Do not restore a duplicate shared power switch.

The shared Layout dropdown offers English and English + Russian. It presents the existing Cyrillic-legend setting: English keeps English key labels, while English + Russian adds the secondary Russian letters on the base layer. An explicit selection applies to both displays; a mixed stored pair stays unchanged until the user chooses an option. This setting does not change the computer's input language.

## State and firmware capabilities

Presentation changes must not create new stored settings, shadow values or a second write path.

- Opening a page, changing the inspected half, hovering and repainting are read-only operations
- Preserve mixed values until the user explicitly edits them
- Retain the existing write queue, pending/error feedback and confirmed readback values
- Do not mark an optimistic change as saved before the existing confirmation path succeeds
- Hide unsupported firmware features and choices; use actual capability bits, setting availability and dynamic counts
- Preserve immediate or automatic saving unless a different interaction was explicitly requested

## Verification and scope

For a UI change, inspect the owning code and nearby established pattern, make the scoped edit, build the requested artifact and hand it off for user acceptance. Check relevant layout, localization and control behavior without adding an unrelated full test, review or CI campaign.

Keep verification claims distinct: source/geometry inspection, compilation, executed tests, application startup and connected-device visual acceptance establish different things. A startup screenshot alone does not verify the settings page on a real keyboard.

Documentation-only edits do not require rebuilding the application. Update this document when an accepted shared design rule changes, and keep it available to Git rather than treating it as an ignored local note. Publication still requires the user's explicit authorization.
