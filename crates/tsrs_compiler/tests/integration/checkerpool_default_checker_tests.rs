use tsrs_compiler::checkerpool::default_checkers_for_parallelism;

/// The default is every core, never below Go's 4 or above 32.
#[test]
fn every_core_up_to_thirty_two() {
    let table = [
        (1, 4),
        (2, 4),
        (4, 4),
        (6, 6),
        (8, 8),
        (12, 12),
        (16, 16),
        (18, 18),
        (24, 24),
        (32, 32),
        (33, 32),
        (48, 32),
        (64, 32),
        (128, 32),
    ];
    for (cores, checkers) in table {
        assert_eq!(default_checkers_for_parallelism(cores), checkers, "{cores} cores");
    }
}
