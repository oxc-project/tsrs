use tsrs_compiler::checkerpool::default_checkers_for_parallelism;

/// The default is every core, never below Go's 4 or above 32.
#[test]
fn every_core_up_to_thirty_two() {
    let table = [(1, 4), (4, 4), (6, 6), (16, 16), (32, 32), (33, 32), (128, 32)];
    for (cores, checkers) in table {
        assert_eq!(default_checkers_for_parallelism(cores), checkers, "{cores} cores");
    }
}
