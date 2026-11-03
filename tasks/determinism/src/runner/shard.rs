use std::path::Path;

use super::{Result, regressions, selection};

// Approximate seconds per item on the 8-vCPU CI runner (measured 2026-11-03), used only to balance shards. Every
// project and fixture runs in exactly one shard whatever its weight; unlisted items get the default.
const PROJECT_SECONDS: &[(&str, u32)] = &[
    ("vscode", 74),
    ("xstate-main", 7),
    ("webpack", 10),
    ("mui-docs", 48),
    ("Compiler", 3),
    ("Compiler-Unions", 4),
    ("cal-diy", 40),
    ("formbricks-web", 44),
    ("supabase-studio", 55),
    ("t3code-server", 80),
    ("mikro-orm", 90),
    ("next-packages-next", 41),
    ("next-root", 28),
    ("storybook", 23),
    ("nuxt", 23),
    ("playwright", 19),
    ("drizzle-orm", 18),
    ("typeorm", 16),
    ("date-fns", 8),
];
const DEFAULT_PROJECT_SECONDS: u32 = 40;
const FIXTURE_SECONDS: &[(&str, u32)] = &[
    ("conditional-instantiation-limit-at", 35),
    ("conditional-instantiation-limit-under", 34),
    ("instantiation-count-call-argument", 30),
    ("instantiation-count-nested-declaration", 16),
];
const DEFAULT_FIXTURE_SECONDS: u32 = 2;

#[derive(Clone, Copy)]
pub(super) struct Shard {
    index: usize,
    count: usize,
}

impl Shard {
    pub(super) fn parse(spec: &str) -> Result<Self> {
        let parsed = spec
            .split_once('/')
            .and_then(|(index, count)| Some((index.parse().ok()?, count.parse().ok()?)));
        match parsed {
            Some((index, count)) if (1..=count).contains(&index) => {
                Ok(Self { index: index - 1, count })
            }
            _ => {
                Err(format!("--shard must be INDEX/COUNT with 1 <= INDEX <= COUNT, not {spec}")
                    .into())
            }
        }
    }

    /// The benchmark projects, or the regression fixtures, that this shard runs, in their usual order.
    ///
    /// Projects and fixtures are split together, longest first onto the least-loaded shard, so a shard that
    /// checks a heavy project runs fewer regression fixtures.
    pub(super) fn names(self, root: &Path, fixtures: bool) -> Result<Vec<String>> {
        let projects = selection(root, "all")?;
        let regressions = regressions::selection(root, "all")?;
        let weight = |table: &[(&str, u32)], default: u32, name: &str| {
            table.iter().find(|(n, _)| *n == name).map_or(default, |&(_, seconds)| seconds)
        };
        let mut items: Vec<(u32, bool, usize)> =
            projects
                .iter()
                .enumerate()
                .map(|(i, name)| (weight(PROJECT_SECONDS, DEFAULT_PROJECT_SECONDS, name), false, i))
                .chain(regressions.iter().enumerate().map(|(i, name)| {
                    (weight(FIXTURE_SECONDS, DEFAULT_FIXTURE_SECONDS, name), true, i)
                }))
                .collect();
        items.sort_by_key(|&(seconds, ..)| std::cmp::Reverse(seconds));
        let names = if fixtures { regressions } else { projects };
        let mut mine = vec![false; names.len()];
        let mut load = vec![0_u32; self.count];
        for (seconds, fixture, i) in items {
            let target = (0..self.count).min_by_key(|&s| (load[s], s)).unwrap_or_default();
            load[target] += seconds;
            if fixture == fixtures {
                mine[i] = target == self.index;
            }
        }
        Ok(names.into_iter().zip(mine).filter_map(|(name, mine)| mine.then_some(name)).collect())
    }
}
