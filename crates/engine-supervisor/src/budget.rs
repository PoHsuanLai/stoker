//! The GPU memory budget: does an engine fit, and whom to evict if not.

use model_catalog::MiB;
use serde::{Deserialize, Serialize};

use crate::{EngineId, MonoMs, UnitSpec};

/// An engine the supervisor may run: its id, what it needs, how to start it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpec {
    pub id: EngineId,
    /// `weights + kv(context) + overhead` (`VramEstimate::need`).
    pub need: MiB,
    pub unit: UnitSpec,
}

/// GPU memory as last observed. Others (a ComfyUI run, a game) are observed, never killed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuMemory {
    pub total: MiB,
    pub used_by_others: MiB,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum BudgetVerdict {
    Fits,
    /// Stop these first, least recently used first; then it fits.
    EvictFirst(Vec<EngineId>),
    NoRoom {
        need: MiB,
        free: MiB,
    },
}

/// Pure. `free = total - used_by_others - sum(running)`. If `need + headroom <= free` the verdict
/// is `Fits`; otherwise evict idle engines, least recently used first, until it fits; if that is
/// still not enough, `NoRoom`. `running` carries each engine's memory and its last use.
///
/// Every engine in `running` is a candidate for eviction: the signature carries no clock, so the
/// caller (`step`) moves engines in a turn (used within the probe interval), and engines still
/// starting, out of `running` and into `gpu.used_by_others`, where they count against `free` and
/// can never be named. `NoRoom::free` is what the budget would be with every candidate evicted.
pub fn budget(
    want: &EngineSpec,
    running: &[(EngineId, MiB, MonoMs)],
    gpu: GpuMemory,
    headroom: MiB,
) -> BudgetVerdict {
    let required = u64::from(want.need.0) + u64::from(headroom.0);
    let committed = u64::from(gpu.used_by_others.0)
        + running
            .iter()
            .map(|(_, mem, _)| u64::from(mem.0))
            .sum::<u64>();
    let free_now = u64::from(gpu.total.0).saturating_sub(committed);
    if required <= free_now {
        return BudgetVerdict::Fits;
    }
    let mut order: Vec<&(EngineId, MiB, MonoMs)> = running.iter().collect();
    order.sort_by(|a, b| (a.2, &a.0).cmp(&(b.2, &b.0)));
    let victims = order
        .iter()
        .scan(free_now, |free, (id, mem, _)| {
            let before = *free;
            *free += u64::from(mem.0);
            Some((id, before, *free))
        })
        .take_while(|(_, before, _)| *before < required)
        .collect::<Vec<_>>();
    let reachable = victims.last().map_or(free_now, |(_, _, after)| *after);
    if reachable >= required {
        BudgetVerdict::EvictFirst(victims.iter().map(|(id, _, _)| (*id).clone()).collect())
    } else {
        BudgetVerdict::NoRoom {
            need: want.need,
            free: MiB(u32::try_from(reachable).unwrap_or(u32::MAX)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{id, spec};

    const GPU: GpuMemory = GpuMemory {
        total: MiB(16000),
        used_by_others: MiB(1000),
    };

    fn run(name: &str, mem: u32, last: u64) -> (EngineId, MiB, MonoMs) {
        (id(name), MiB(mem), MonoMs(last))
    }

    type Case = (
        &'static str,
        u32,
        Vec<(EngineId, MiB, MonoMs)>,
        BudgetVerdict,
    );

    fn ids(names: &[&str]) -> BudgetVerdict {
        BudgetVerdict::EvictFirst(names.iter().map(|n| id(n)).collect())
    }

    #[test]
    fn budget_table() {
        let a = run("a", 4000, 50);
        let b = run("b", 4000, 10);
        let c = run("c", 4000, 30);
        let cases: Vec<Case> = vec![
            ("empty gpu fits", 10000, vec![], BudgetVerdict::Fits),
            (
                "exact fit with headroom",
                14000,
                vec![],
                BudgetVerdict::Fits,
            ),
            (
                "one over",
                14001,
                vec![],
                BudgetVerdict::NoRoom {
                    need: MiB(14001),
                    free: MiB(15000),
                },
            ),
            (
                "running fits beside",
                5000,
                vec![a.clone()],
                BudgetVerdict::Fits,
            ),
            (
                "evicts the least recently used first",
                6000,
                vec![a.clone(), b.clone(), c.clone()],
                ids(&["b"]),
            ),
            (
                "evicts as many as needed in LRU order",
                12000,
                vec![a.clone(), b.clone(), c.clone()],
                ids(&["b", "c", "a"]),
            ),
            (
                "two are enough",
                9000,
                vec![a.clone(), b.clone(), c.clone()],
                ids(&["b", "c"]),
            ),
            (
                "ties break by id",
                10000,
                vec![run("y", 4000, 5), run("x", 4000, 5), run("z", 4000, 9)],
                ids(&["x", "y"]),
            ),
            (
                "no room reports the best reachable free",
                15000,
                vec![a.clone(), b.clone()],
                BudgetVerdict::NoRoom {
                    need: MiB(15000),
                    free: MiB(15000),
                },
            ),
        ];
        for (name, need, running, expected) in cases {
            assert_eq!(
                budget(&spec("want", need), &running, GPU, MiB(1000)),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn used_by_others_is_never_evicted() {
        let gpu = GpuMemory {
            total: MiB(16000),
            used_by_others: MiB(12000),
        };
        assert_eq!(
            budget(&spec("want", 4000), &[], gpu, MiB(0)),
            BudgetVerdict::Fits
        );
        assert_eq!(
            budget(&spec("want", 4001), &[], gpu, MiB(0)),
            BudgetVerdict::NoRoom {
                need: MiB(4001),
                free: MiB(4000)
            }
        );
    }

    #[test]
    fn overcommitted_gpu_has_zero_free() {
        let gpu = GpuMemory {
            total: MiB(1000),
            used_by_others: MiB(900),
        };
        let running = [run("a", 500, 1)];
        assert_eq!(
            budget(&spec("want", 100), &running, gpu, MiB(0)),
            ids(&["a"])
        );
    }
}
