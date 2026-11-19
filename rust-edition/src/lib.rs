/// Sorts an initialized array of file indices, preserving the order of ties.
///
/// # Safety
/// For count > 1, both pointers must reference count aligned, initialized
/// elements, with exclusive access to indices. Each index must be in range.
#[no_mangle]
pub unsafe extern "C" fn torrent_re_sort_by_size(
    sizes: *const i64,
    indices: *mut i32,
    count: usize,
    ascending: u8,
) {
    if count <= 1 {
        return;
    }
    let sizes = unsafe { std::slice::from_raw_parts(sizes, count) };
    let indices = unsafe { std::slice::from_raw_parts_mut(indices, count) };
    indices.sort_by(|&a, &b| {
        let order = sizes[a as usize].cmp(&sizes[b as usize]);
        if ascending != 0 {
            order
        } else {
            order.reverse()
        }
    });
}

pub mod profile;
pub mod queue_policy;

#[cfg(feature = "libtorrent")]
pub mod engine;
