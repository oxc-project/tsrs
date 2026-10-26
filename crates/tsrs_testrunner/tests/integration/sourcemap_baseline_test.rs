use tsrs_testrunner::sourcemap_baseline::*;

#[test]
fn query_round_trip_is_identity() {
    for s in ["a b+c%d", "héllo\r\n\t~", ""] {
        assert_eq!(query_unescape(&query_escape(s)).unwrap(), s.as_bytes());
    }
}
