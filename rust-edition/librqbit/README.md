# Патч librqbit для Torrent Rust Edition

База — [закреплённая ревизия PR №748](https://github.com/ikatson/rqbit/tree/d525e433f2ccde9d837e68eddd190be4275a2fa7) с поддержкой BitTorrent v2 и гибридных торрентов. Патч меняет библиотеку движка. Прототип приложения в этой ветке пока не использует его.

Сессия с прокси принимает локальный адрес SOCKS5h, отправляет через него запросы HTTP(S) и соединения с пирами, отключает UDP-трекеры, DHT, локальное обнаружение и входящие подключения. Проверка в комплекте поднимает локальные TCP и UDP сокеты и проверяет, что недоступный SOCKS-прокси не приводит к прямому соединению с пиром.

API приоритетов исключает файлы с нулевым приоритетом и планирует новые части файлов с больших приоритетов. Порядок сохраняется при паузе в той же сессии. Для восстановления после перезапуска клиенту нужно сохранить значения и снова передать их движку.

Для проверки на чистой копии исходников:

```sh
git clone https://github.com/ikatson/rqbit.git
git -C rqbit fetch origin d525e433f2ccde9d837e68eddd190be4275a2fa7
git -C rqbit switch --detach d525e433f2ccde9d837e68eddd190be4275a2fa7
git -C rqbit apply /absolute/path/to/rust-edition/librqbit/torrent.patch
cargo test --manifest-path rqbit/Cargo.toml --locked -p librqbit --no-default-features --features rust-tls --lib
```

Остаётся проверить изоляцию в собранном клиенте на Windows с Xray и Sudoku, включая переключение с прямого режима и остановку VPN во время загрузки.
