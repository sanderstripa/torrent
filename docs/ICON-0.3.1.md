# Torrent 0.3.1 icon

Master: assets/icon-master.png, redrawn using the built-in image_gen tool from the user-supplied concept. The external white rounded square is removed; the champagne ribbon, bronze downward arrow and filled light center are retained. The master is preserved unchanged. tools/render_icon.py fits its optical bounds and packages ten Windows sizes, without upscaling the master. assets/icon-preview.png and assets/app.ico are generated from that same master. The old blue vector source was removed.

Prompt: Recreate/redraw the exact supplied icon from scratch as a crisp high-resolution Windows application icon. Preserve the intertwined folded champagne/beige satin ribbon silhouette, crossing order, lighting, gradient colors, central warm white rounded diamond and centered bronze downward download arrow. Remove the large outer white rounded square tile and its shadow completely. Only the enclosed central diamond remains light and filled; the exterior is transparent. Make the organic symbol nearly edge to edge without clipping, with sharp contours and a legible arrow at tiny sizes. Do not invent shapes or change the mark.

Coverage: application and dialog window classes, taskbar and Alt+Tab, desktop and Start menu shortcuts, installer PE resources and Wails runtime PNG, NSIS payload and generated uninstaller, installed-programs DisplayIcon, Torrent.File and Torrent.Magnet DefaultIcon, README preview. Windows is notified of association changes after install. No Windows icon-cache deletion is performed.

The shared installer source remains pinned. Product ICO/PNG inputs are staged only for the Torrent build and restored in finally. Build metadata records both icon hashes. No shared source changes are published to CONNECT.
