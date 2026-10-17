# CONNECT VPN theme icons

The original `monitor.js`, `sun.js` and `moon.js` modules are copied unchanged from `lucide-vue-next` **0.468.0**, the version pinned by CONNECT VPN's `frontend/package-lock.json`. CONNECT's `frontend/src/App.vue` uses these exact Monitor, Sun and Moon components at **13px**. Their SVG attributes are `viewBox="0 0 24 24"`, `fill="none"`, `stroke="currentColor"`, stroke width 2, round caps and round joins. Its theme picker centers icon and text with a 5px gap.

Source package: https://registry.npmjs.org/lucide-vue-next/-/lucide-vue-next-0.468.0.tgz

Package integrity from CONNECT: `sha512-quV/6T8YB1XK0VOEnebg3Byd8Rsan5/m95cvjnuHV4vcS3qEnLAybkrSh0hk3ppavx+V7R1PjNW+mGDvcBdz4A==`.

`tools/compile_theme_icons.py` compiles the original rect, line, circle and path nodes to exact Direct2D geometry. The Moon uses both original elliptical arcs, with no masking approximation. Generated source is verified by CI. ISC/Feather notices are in `LICENSE` and installed as `licenses/Lucide-ISC.txt`.
