# CONNECT UI System v1.0 in Torrent

Reference: [approved specification](https://github.com/sanderstripa/connect/blob/connect-ui-system-v1-reference/docs/CONNECT-UI-SYSTEM-v1.md).

Torrent retains its own layout, 840 × 610 outer window dimensions, 136px torrent row pitch, file selection workflow and four settings. The torrent engine and approved product icon are unchanged. Settings remain immediate and persistent.

## Preserved design tokens

| Role | Light | Dark |
|---|---|---|
| Background | `#F5F5F4` | `#0C0C0C` |
| Surface | `#FAFAFA` | `#101010` |
| Text | `#17191C` | `#F2F2F1` |
| Secondary text | `#777C81` | `#94989C` |
| Border | `#DADBD9` | `#303234` |
| Selected | `#E8E9E7` | `#222324` |
| Progress track | `#E0E1DE` | `#303234` |

The Montserrat TTF is byte-for-byte unchanged. The invariant check also protects Torrent's palette functions and initial outer dimensions. CONNECT's `#F7F7F5 / #0C0C0D`, Screenshot's `#F5F5F5 / #0A0A0A`, and Screencast's `#F7F7F5 / #0B0B0C` were reviewed in their reference sources, not applied to Torrent. No files in those repositories are changed.

## Implemented

- A single custom 62px header with TORRENT. at the left, existing add/settings actions, minimize and close at the right. No native caption, redundant title icon, maximize button or maximize command. Drag, edge resizing, taskbar icon, DPI resize path and Alt+F4 remain supported.
- Shared 8px control radius, 36px inputs/selects and modal action buttons, 13px content text, 12px helper text, 18px headings and 2px keyboard focus outlines. File row density and dialog dimensions are retained.
- Contrasting monochrome primary actions, neutral secondary actions, subtle press/hover with 140ms transitions respecting Windows reduced-motion settings.
- Four-pixel monochrome progress; existing checkbox semantics for file selection. Shared switch/slider primitives are available but no unused controls are shown in Torrent.
- Compact custom runtime error notices with a real error indicator and the same modal action styling. Startup failures before the application window exists retain a native emergency fallback; standard OS file/folder pickers remain OS dialogs.
- System/Light/Dark and RU/EN inside settings. System mode reads Windows AppsUseLightTheme and responds to theme notifications. Old saved Light=0 and Dark=1 settings keep their meaning; System=2 is added without changing the file structure.

## Missing approved asset

The published reference and CONNECT source trees did not contain the original approved gear #8 or an approved exact vector of it. The pre-existing Torrent settings symbol is retained without its background tile. It is explicitly **not claimed to be gear #8**. No substitute gear is drawn. Applying the exact approved gear remains dependent on receiving that asset (and its vector approval if reconstruction is required).

## Verification

Windows CI builds the client and installer, checks the preserved design tokens, runs engine and UI regressions, and captures both themes. UI checks cover caption/maximize removal, outer dimensions, header hit testing, minimize/settings actions, settings persistence, System theme updates without modifying Windows preferences, RU/EN, custom notices, torrent selection and keyboard actions.

Physical monitor/DPI transitions and installation over the real user profile require separate manual verification. Screenshot PR #1 is untouched; this implementation is isolated in Torrent's own PR.
