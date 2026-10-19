# CONNECT installer component parity, 0.2.8

Reference: `sanderstripa/connect` commit `37210bf6dbe1ebcd85938fcb6465bb9b5ed5d184`, `cmd/installer/web/style.css` and its unmodified Montserrat asset.

`reference/settings-ru.json` and `settings-en.json` record DOM bounds and computed typography from that installer at 840 x 540. Chrome-specific controls are omitted visually; their layout space is retained. `src/setup_geometry.h` transfers those measured values to native Direct2D. Light/dark reference PNGs are actual reference renders, not generated mockups.

Installer text, including the editable path, now uses DirectWrite with the exact CONNECT font, 400/500/600 weights and source colors. Path layout is updated only on page/language/theme/DPI transitions and only moved when the rectangle changes. Parent painting never moves, shows or invalidates the path. The native EDIT still handles text, IME and clipboard; its visible text and caret use DirectWrite.

There is no extra language/appearance explanation. Successful installation writes both the selected language and theme while preserving the download folder and add action. Native folder selection, Unicode installation paths and shortcut options remain supported.

Windows integration covers RU/EN x light/dark actual first launch, the app settings title in the selected language, stable path pixels/position across repeated parent redraws, custom installation directories, shortcut combinations and uninstall cleanup. The normal Torrent client UI and engine sources are unchanged.


0.2.7: explicitly set the Montserrat variable `wght` axis (the source font default is 100) on both IDWriteTextFormat3 and IDWriteTextLayout4; verify axis API success in native visual capture. The editable path retains its horizontal offset on focus loss instead of changing it during paint. Integration now also verifies focused/selected path stability.

0.2.8: remeasured full CONNECT v0.2.5 (37210bf), keeping extension layout space without adding extension UI to Torrent. Accept top 387; finish title top 131.75, Done top 312.25. Path content height is 15px (35px field minus 2px border and 18px padding), not 17px. Text uses fractional baseline positioning (NO_SNAP) rather than native pixel snapping.

Renderer limitation: CONNECT uses Chromium HTML input/button; Torrent retains native DirectWrite and EDIT behavior. Exact font file, weight, dimensions, palette and layout are verified separately; this does not establish identical glyph rasterization between the two renderers. No scale, zoom, filter, backdrop-filter, translateZ or will-change is introduced.

Browser-measured normal line metrics (size: ascent/descent): 11:11/3, 12:12/3, 13:13/3, 14:14/4, 23:22/6, 42:41/11. Native text formats now use these source-font rounded metrics instead of fractional DirectWrite defaults.

0.2.9: ribbon-only visual change. Welcome object-fit box (377,61)-(817,426), 440×365 CSS px; progress box (185,69)-(655,374), 470×305 CSS px, from CONNECT source CSS at 840×540. Original 1536×1024 RGBA artwork is unchanged and rendered with its aspect ratio preserved. Both screens downsample the source even at 300% Windows DPI; no low-resolution enlargement or regenerated artwork.
