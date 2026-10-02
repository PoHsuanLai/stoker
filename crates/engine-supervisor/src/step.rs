//! The lifecycle machine.

use std::time::Duration;

use model_catalog::MiB;
use serde::{Deserialize, Serialize};

use crate::{
    Attempt, BudgetVerdict, EngineFailure, EngineId, EngineSpec, EngineState, ExitCode, GpuMemory,
    MonoMs, Probe, UnitSpec, budget,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum SupervisorIn {
    /// A caller wants this engine ready.
    Want(EngineId),
    /// A turn used this engine just now.
    Used(EngineId),
    Exited {
        id: EngineId,
        code: ExitCode,
    },
    Probed {
        id: EngineId,
        probe: Probe,
    },
    Gpu(GpuMemory),
    Tick(MonoMs),
}

/// What the daemon must do next. `step` never does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorOut {
    Spawn(EngineId, UnitSpec),
    Stop(EngineId),
    Probe(EngineId),
    Changed(EngineId, EngineState),
    /// Deliver a `Tick` at this time.
    WakeAt(MonoMs),
}

/// The values of the six `ai.engine.*` settings (design/22 rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisorConfig {
    pub idle_unload: Duration,
    pub start_timeout: Duration,
    pub probe_every: Duration,
    pub max_attempts: Attempt,
    pub backoff: (Duration, Duration),
    pub headroom: MiB,
}

impl Default for SupervisorConfig {
    /// 600 s idle unload, 180 s start timeout (vLLM cold start), probe every 500 ms, 3
    /// attempts, backoff 2 s to 30 s, 1024 MiB headroom.
    fn default() -> Self {
        SupervisorConfig {
            idle_unload: Duration::from_secs(600),
            start_timeout: Duration::from_secs(180),
            probe_every: Duration::from_millis(500),
            max_attempts: Attempt(3),
            backoff: (Duration::from_secs(2), Duration::from_secs(30)),
            headroom: MiB(1024),
        }
    }
}

/// Every engine the daemon may run, its state, and the GPU as last observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supervisor {
    engines: Vec<(EngineSpec, EngineState)>,
    gpu: GpuMemory,
    /// The last `Tick`: the clock every other input reads.
    now: MonoMs,
    /// Engines a caller still wants, in the order they asked. An engine leaves this list when it
    /// fails for good, is unloaded for idleness, or is evicted for another.
    wanted: Vec<EngineId>,
}

impl Supervisor {
    /// All engines `Stopped`.
    pub fn new(specs: Vec<EngineSpec>, gpu: GpuMemory) -> Supervisor {
        let engines = specs
            .into_iter()
            .map(|spec| (spec, EngineState::Stopped))
            .collect();
        Supervisor {
            engines,
            gpu,
            now: MonoMs(0),
            wanted: Vec::new(),
        }
    }

    pub fn state(&self, id: &EngineId) -> Option<&EngineState> {
        self.engines
            .iter()
            .find(|(spec, _)| &spec.id == id)
            .map(|(_, state)| state)
    }

    pub fn gpu(&self) -> GpuMemory {
        self.gpu
    }
}

/// The only transition: `(state, input) -> (state, effects)`. Time arrives as `Tick`; every other
/// input reads the last tick seen (the daemon ticks before it delivers anything late). The
/// table is design: models §4.1. Beyond the table, entering `Ready` and `Backoff` and every
/// `Tick` in `Ready` schedule a `WakeAt` (idle unload and the retry need a tick), a victim of
/// eviction reports `Changed`, and `Spawn` from a backoff re-checks the budget.
pub fn step(
    mut s: Supervisor,
    input: SupervisorIn,
    cfg: &SupervisorConfig,
) -> (Supervisor, Vec<SupervisorOut>) {
    let mut out = Vec::new();
    match input {
        SupervisorIn::Want(id) => s.want(&id, cfg, &mut out),
        SupervisorIn::Used(id) => s.used(&id),
        SupervisorIn::Exited { id, code } => s.exited(&id, code, cfg, &mut out),
        SupervisorIn::Probed { id, probe } => s.probed(&id, probe, cfg, &mut out),
        SupervisorIn::Gpu(mem) => s.gpu = mem,
        SupervisorIn::Tick(now) => s.tick(now, cfg, &mut out),
    }
    (s, out)
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

fn after(at: MonoMs, d: Duration) -> MonoMs {
    MonoMs(at.0.saturating_add(millis(d)))
}

fn elapsed(since: MonoMs, now: MonoMs) -> u64 {
    now.0.saturating_sub(since.0)
}

/// `b(a)`: the first backoff doubled per attempt, capped.
fn backoff(attempt: Attempt, cfg: &SupervisorConfig) -> Duration {
    let (first, cap) = cfg.backoff;
    let doublings = u32::from(attempt.0.saturating_sub(1));
    let factor = 1u32.checked_shl(doublings).unwrap_or(u32::MAX);
    first.saturating_mul(factor).min(cap)
}

impl Supervisor {
    fn spec(&self, id: &EngineId) -> Option<&EngineSpec> {
        self.engines
            .iter()
            .find(|(spec, _)| &spec.id == id)
            .map(|(spec, _)| spec)
    }

    fn slot(&mut self, id: &EngineId) -> Option<&mut EngineState> {
        self.engines
            .iter_mut()
            .find(|(spec, _)| &spec.id == id)
            .map(|(_, state)| state)
    }

    fn set(&mut self, id: &EngineId, state: EngineState, out: &mut Vec<SupervisorOut>) {
        if let Some(slot) = self.slot(id) {
            *slot = state;
            out.push(SupervisorOut::Changed(id.clone(), state));
        }
    }

    fn unwant(&mut self, id: &EngineId) {
        self.wanted.retain(|w| w != id);
    }

    fn is_wanted(&self, id: &EngineId) -> bool {
        self.wanted.contains(id)
    }

    fn want(&mut self, id: &EngineId, cfg: &SupervisorConfig, out: &mut Vec<SupervisorOut>) {
        let Some(state) = self.state(id).copied() else {
            return;
        };
        if !self.is_wanted(id) {
            self.wanted.push(id.clone());
        }
        match state {
            EngineState::Stopped | EngineState::Failed(_) => {
                self.evaluate(id, Attempt(1), cfg, out)
            }
            _ => {}
        }
    }

    fn used(&mut self, id: &EngineId) {
        let now = self.now;
        if let Some(EngineState::Ready { last_used, .. }) = self.slot(id) {
            *last_used = now;
        }
    }

    /// Engines out of reach of eviction go to the fixed total (an engine in a turn, or starting);
    /// the rest are candidates, a stopping one first since its memory is already coming back.
    fn candidates(&self, cfg: &SupervisorConfig) -> (MiB, Vec<(EngineId, MiB, MonoMs)>) {
        let mut fixed = 0u32;
        let mut running = Vec::new();
        for (spec, state) in &self.engines {
            match *state {
                EngineState::Starting { .. } => fixed = fixed.saturating_add(spec.need.0),
                EngineState::Ready { last_used, .. }
                    if elapsed(last_used, self.now) < millis(cfg.probe_every) =>
                {
                    fixed = fixed.saturating_add(spec.need.0);
                }
                EngineState::Ready { last_used, .. } => {
                    running.push((spec.id.clone(), spec.need, last_used));
                }
                EngineState::Stopping { .. } => {
                    running.push((spec.id.clone(), spec.need, MonoMs(0)));
                }
                _ => {}
            }
        }
        (MiB(fixed), running)
    }

    /// Start `id` if the budget allows, stop idle engines to make room, or fail with numbers.
    fn evaluate(
        &mut self,
        id: &EngineId,
        attempt: Attempt,
        cfg: &SupervisorConfig,
        out: &mut Vec<SupervisorOut>,
    ) {
        let Some(want) = self.spec(id).cloned() else {
            return;
        };
        let (fixed, running) = self.candidates(cfg);
        let gpu = GpuMemory {
            used_by_others: MiB(self.gpu.used_by_others.0.saturating_add(fixed.0)),
            ..self.gpu
        };
        match budget(&want, &running, gpu, cfg.headroom) {
            BudgetVerdict::Fits => {
                let now = self.now;
                out.push(SupervisorOut::Spawn(id.clone(), want.unit));
                out.push(SupervisorOut::Probe(id.clone()));
                out.push(SupervisorOut::WakeAt(after(now, cfg.probe_every)));
                self.set(
                    id,
                    EngineState::Starting {
                        since: now,
                        attempt,
                    },
                    out,
                );
            }
            BudgetVerdict::EvictFirst(victims) => victims.iter().for_each(|v| self.evict(v, out)),
            BudgetVerdict::NoRoom { need, free } => {
                self.unwant(id);
                self.set(
                    id,
                    EngineState::Failed(EngineFailure::NoRoom { need, free }),
                    out,
                );
            }
        }
    }

    fn evict(&mut self, id: &EngineId, out: &mut Vec<SupervisorOut>) {
        if matches!(self.state(id), Some(EngineState::Ready { .. })) {
            self.unwant(id);
            out.push(SupervisorOut::Stop(id.clone()));
            self.set(id, EngineState::Stopping { since: self.now }, out);
        }
    }

    /// The process ended or never came up: back off, or give up at the last attempt.
    fn crash(
        &mut self,
        id: &EngineId,
        attempt: Attempt,
        failure: EngineFailure,
        cfg: &SupervisorConfig,
        out: &mut Vec<SupervisorOut>,
    ) {
        if attempt >= cfg.max_attempts {
            self.unwant(id);
            self.set(id, EngineState::Failed(failure), out);
        } else {
            let until = after(self.now, backoff(attempt, cfg));
            out.push(SupervisorOut::WakeAt(until));
            let next = Attempt(attempt.0.saturating_add(1));
            self.set(
                id,
                EngineState::Backoff {
                    until,
                    attempt: next,
                },
                out,
            );
        }
    }

    fn exited(
        &mut self,
        id: &EngineId,
        code: ExitCode,
        cfg: &SupervisorConfig,
        out: &mut Vec<SupervisorOut>,
    ) {
        let exit = EngineFailure::Exited { code };
        match self.state(id).copied() {
            Some(EngineState::Starting { attempt, .. }) => self.crash(id, attempt, exit, cfg, out),
            Some(EngineState::Ready { .. }) => self.crash(id, Attempt(1), exit, cfg, out),
            Some(EngineState::Stopping { .. }) => {
                self.set(id, EngineState::Stopped, out);
                self.settle_pending(cfg, out);
            }
            _ => {}
        }
    }

    /// Memory came back: every wanted engine still `Stopped` looks at the budget again.
    fn settle_pending(&mut self, cfg: &SupervisorConfig, out: &mut Vec<SupervisorOut>) {
        let pending: Vec<EngineId> = self
            .wanted
            .iter()
            .filter(|id| matches!(self.state(id), Some(EngineState::Stopped)))
            .cloned()
            .collect();
        pending
            .iter()
            .for_each(|id| self.evaluate(id, Attempt(1), cfg, out));
    }

    fn probed(
        &mut self,
        id: &EngineId,
        probe: Probe,
        cfg: &SupervisorConfig,
        out: &mut Vec<SupervisorOut>,
    ) {
        let Some(EngineState::Starting { since, attempt }) = self.state(id).copied() else {
            return;
        };
        let now = self.now;
        match probe {
            Probe::Ready => {
                out.push(SupervisorOut::WakeAt(after(now, cfg.idle_unload)));
                self.set(
                    id,
                    EngineState::Ready {
                        since: now,
                        last_used: now,
                    },
                    out,
                );
            }
            Probe::Loading | Probe::Down if elapsed(since, now) >= millis(cfg.start_timeout) => {
                self.never_ready(id, attempt, cfg, out);
            }
            Probe::Loading | Probe::Down => {
                out.push(SupervisorOut::WakeAt(after(now, cfg.probe_every)));
                out.push(SupervisorOut::Probe(id.clone()));
            }
        }
    }

    fn never_ready(
        &mut self,
        id: &EngineId,
        attempt: Attempt,
        cfg: &SupervisorConfig,
        out: &mut Vec<SupervisorOut>,
    ) {
        out.push(SupervisorOut::Stop(id.clone()));
        self.crash(id, attempt, EngineFailure::NeverReady, cfg, out);
    }

    fn tick(&mut self, now: MonoMs, cfg: &SupervisorConfig, out: &mut Vec<SupervisorOut>) {
        self.now = now;
        let ids: Vec<EngineId> = self
            .engines
            .iter()
            .map(|(spec, _)| spec.id.clone())
            .collect();
        for id in ids {
            match self.state(&id).copied() {
                Some(EngineState::Starting { since, attempt })
                    if elapsed(since, now) >= millis(cfg.start_timeout) =>
                {
                    self.never_ready(&id, attempt, cfg, out);
                }
                Some(EngineState::Ready { last_used, .. }) => {
                    if elapsed(last_used, now) >= millis(cfg.idle_unload) {
                        self.unwant(&id);
                        out.push(SupervisorOut::Stop(id.clone()));
                        self.set(&id, EngineState::Stopping { since: now }, out);
                    } else {
                        out.push(SupervisorOut::WakeAt(after(last_used, cfg.idle_unload)));
                    }
                }
                Some(EngineState::Backoff { until, attempt }) if now >= until => {
                    if self.is_wanted(&id) {
                        self.evaluate(&id, attempt, cfg, out);
                    } else {
                        self.set(&id, EngineState::Stopped, out);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
