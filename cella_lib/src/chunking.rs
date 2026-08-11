//! Splitting a grid's output buffers into disjoint per-worker chunks.

use crate::types::CellType;

/// One worker's disjoint slice of every output buffer.
///
/// `start` is the index of the chunk's first cell in the whole grid; all slices
/// are indexed *locally* (0 = `start`) except reads of the shared `cells` buffer.
pub(crate) struct OutChunk<'a> {
    pub start: usize,
    pub next_cells: &'a mut [CellType],
    pub ages: &'a mut [u32],
    pub history_data: &'a mut [CellType],
    pub history_heads: &'a mut [u8],
    pub history_counts: &'a mut [u8],
}

/// Split the output buffers into chunks of at most `chunk` cells each.
///
/// Handles `history_limit == 0`, where the history buffers are empty and every
/// chunk gets empty history slices. The previous `chunks_mut(chunk * hl)` form
/// panicked on a zero chunk length, which is why `hl == 0` used to be forced
/// down the serial path — the cheapest configuration got the least parallelism.
pub(crate) fn split_chunks<'a>(
    next_cells: &'a mut [CellType],
    ages: &'a mut [u32],
    history_data: &'a mut [CellType],
    history_heads: &'a mut [u8],
    history_counts: &'a mut [u8],
    history_limit: usize,
    chunk: usize,
) -> Vec<OutChunk<'a>> {
    assert!(chunk > 0, "chunk size must be non-zero");
    let mut out = Vec::with_capacity(next_cells.len().div_ceil(chunk));
    let (mut next_cells, mut ages) = (next_cells, ages);
    let mut history_data = history_data;
    let mut history_heads = history_heads;
    let mut history_counts = history_counts;
    let mut start = 0usize;
    while !next_cells.is_empty() {
        let n = chunk.min(next_cells.len());
        let hn = if history_limit > 0 { n } else { 0 };
        let (nc, nc_rest) = next_cells.split_at_mut(n);
        let (ag, ag_rest) = ages.split_at_mut(n);
        let (hd, hd_rest) = history_data.split_at_mut(n * history_limit);
        let (hh, hh_rest) = history_heads.split_at_mut(hn);
        let (hc, hc_rest) = history_counts.split_at_mut(hn);
        out.push(OutChunk {
            start,
            next_cells: nc,
            ages: ag,
            history_data: hd,
            history_heads: hh,
            history_counts: hc,
        });
        next_cells = nc_rest;
        ages = ag_rest;
        history_data = hd_rest;
        history_heads = hh_rest;
        history_counts = hc_rest;
        start += n;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CellType;

    #[test]
    fn split_chunks_handles_zero_history_limit() {
        let mut next_cells = vec![CellType::from("A"); 5];
        let mut ages = vec![0u32; 5];
        let mut history_data: Vec<CellType> = Vec::new();
        let mut history_heads: Vec<u8> = Vec::new();
        let mut history_counts: Vec<u8> = Vec::new();

        let chunks = split_chunks(
            &mut next_cells,
            &mut ages,
            &mut history_data,
            &mut history_heads,
            &mut history_counts,
            0,
            2,
        );

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks[0].next_cells.len(), 2);
        assert!(chunks[0].history_data.is_empty());
        assert!(chunks[0].history_heads.is_empty());
        assert!(chunks[0].history_counts.is_empty());
    }

    #[test]
    fn split_chunks_assigns_history_slices_when_enabled() {
        let mut next_cells = vec![CellType::from("A"); 4];
        let mut ages = vec![0u32; 4];
        let mut history_data = vec![CellType::from("A"); 8];
        let mut history_heads = vec![0u8; 4];
        let mut history_counts = vec![0u8; 4];

        let chunks = split_chunks(
            &mut next_cells,
            &mut ages,
            &mut history_data,
            &mut history_heads,
            &mut history_counts,
            2,
            3,
        );

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].history_data.len(), 6);
        assert_eq!(chunks[0].history_heads.len(), 3);
        assert_eq!(chunks[0].history_counts.len(), 3);
        assert_eq!(chunks[1].start, 3);
    }

    // TODO add tests with heterogeneous cells and test chunks get correct history split
}

