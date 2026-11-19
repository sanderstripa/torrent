//! Settings passed to the future libtorrent session at construction.
/// libtorrent's auto-managed downloads and seeds have separate limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuePolicy {
    pub active_downloads: i32,
    pub active_seeds: i32,
    pub active_limit: i32,
    pub connections_limit: i32,
}

pub const PREVIEW_POLICY: QueuePolicy = QueuePolicy {
    active_downloads: 16,
    active_seeds: 1,
    active_limit: 17,
    connections_limit: 80,
};
