# Torrent 1.4.2

- Torrent now appears in the foreground on first launch and when an existing instance is opened again, without using an always-on-top window.
- Added an isolated VPN for Torrent traffic with VLESS/Xray and Sudoku keys.
- VPN routing is applied only to libtorrent. It does not change Windows routes, the system VPN, or other applications.
- Added fail-closed leak protection. If the VPN core stops or the local SOCKS tunnel becomes unavailable, Torrent traffic remains blocked instead of falling back to the direct connection.
- Added the approved Sona VPN button to the title bar and the approved VPN controls to both Torrent themes.
- Connection keys are encrypted for the current Windows user with DPAPI.
- Closing the window now hides Torrent in the notification area while downloads and seeding continue.
- Added “Open Torrent” and “Exit” to the notification-area menu. “Exit” performs the full application shutdown.
