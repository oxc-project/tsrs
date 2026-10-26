use tsrs_ls::lsconv::linemap::*;

#[test]
fn line_lookup_keeps_the_first_exact_match_and_previous_gap() {
    let map = LSPLineMap { line_starts: vec![2, 5, 5, 8], ascii_only: true };
    for (position, line) in [(0, 0), (2, 0), (4, 0), (5, 1), (7, 2), (8, 3), (9, 3)] {
        assert_eq!(map.compute_index_of_line_start(position), line);
    }
    assert_eq!(LSPLineMap::default().compute_index_of_line_start(0), 0);
}

// Only \n, \r and \r\n break lines (not U+2028), and non-ASCII text clears ascii_only.
#[test]
fn lsp_line_starts() {
    let lm = compute_lsp_line_starts("a\nb\r\nc\rd\u{2028}e");
    assert_eq!(lm.line_starts, vec![0, 2, 5, 7]);
    assert!(!lm.ascii_only);
    assert!(compute_lsp_line_starts("abc\n").ascii_only);
    assert_eq!(compute_lsp_line_starts("").line_starts, vec![0]);

    let lm = LSPLineMap { line_starts: vec![5, 10, 23, 80], ascii_only: true };
    assert_eq!(lm.compute_index_of_line_start(20), 1); // the Go comment says (3, false); the search returns (2, false)
    assert_eq!(lm.compute_index_of_line_start(23), 2);
    assert_eq!(lm.compute_index_of_line_start(0), 0);
    assert_eq!(lm.compute_index_of_line_start(100), 3);
}
