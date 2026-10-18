# Torrent installer 0.2.4 — visual review, awaiting approval

Only installer presentation changes. The application, engine, icon, shared theme/font code, payload NSIS script, installation routines and release version metadata remain untouched.

Reference: sanderstripa/connect commit 09dab71e7982848d8b44711dc0a9cfc9e39333f2, cmd/installer/web/style.css and index.html. Native Direct2D/DirectWrite implementation uses its 840 × 540 geometry, color tokens, typography and spacing. No WebView is introduced.

Ribbon: supplied beige reference, cut out using the built-in image tool. Prompt: preserve exact geometry, camera, proportions, beige shading and edge highlights; remove only black background/glow including inner holes to transparency; no redesign, recolor, text or added objects. Both original and cutout are included for review.

Four screens in light/dark: welcome, installation settings, progress and finish. No Chrome extension controls or copy. Version label is 0.2.4. Montserrat remains resource embedded.

Settings now includes interactive Desktop and Start menu shortcut switches, matching CONNECT, both enabled by default. Their selection is retained while navigating the visual preview. Connecting these choices to shortcut creation in the installation backend is deferred until visual approval, together with the full build. Existing install execution is untouched in this review.

Visual-only executable has no installation payload and always forces preview mode. It cannot install, change associations or launch an installed application. The normal full build is disabled on this review branch. No stable release/tag, no full installer or app build, and no merge to main until user visual approval.
