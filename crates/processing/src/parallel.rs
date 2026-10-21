//! Small helper to run a per-row job across threads.

/// Calls `job(first_row, rows)` on bands of `data` (rows of `row_len`
/// elements each), one band per thread.
pub(crate) fn for_row_bands<T: Send>(
    data: &mut [T],
    row_len: usize,
    job: impl Fn(usize, &mut [T]) + Sync,
) {
    if row_len == 0 || data.is_empty() {
        return;
    }
    let rows = data.len() / row_len;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(rows.max(1));
    let rows_per_band = rows.div_ceil(threads).max(1);
    let job = &job;
    std::thread::scope(|scope| {
        for (band, chunk) in data.chunks_mut(rows_per_band * row_len).enumerate() {
            scope.spawn(move || job(band * rows_per_band, chunk));
        }
    });
}
