//! The aging ladder (mayfly's contract, ported inside the loop):
//!
//! | life used | state     | behavior |
//! |-----------|-----------|----------|
//! | 0–50%     | Alive     | normal work |
//! | 50–75%    | Warn      | inject "narrow to done_when only" (one time) |
//! | 75–100%   | Narrow    | inject "final push" (one time) |
//! | 100%      | Expired   | hard stop; report `ttl_exceeded` — a task-design
//! |           |           | failure per mayfly doctrine, never "ran out of time" |
//!
//! A run that hits its deadline without done_when is a task-design failure,
//! logged as such. Pure module: `state_at` is testable without a clock.

use anyhow::{bail, Result};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Alive,
    Warn,
    Narrow,
    Expired,
}

#[derive(Debug, Clone, Copy)]
pub struct Ladder {
    pub ttl: Option<Duration>,
    pub warn_at: f64,
    pub narrow_at: f64,
}

pub const WARN_DEFAULT: f64 = 0.5;
pub const NARROW_DEFAULT: f64 = 0.75;

pub const WARN_INJECT: &str =
    "AGING: half your lifespan is used. Narrow to done_when only — finish what the \
     mechanical gate needs, stop starting new work, prepare to hand off.";
pub const NARROW_INJECT: &str =
    "NARROWING: three quarters used. This is the final push — verify done_when or \
     hand off now with an honest report of what did and did not land.";

impl Ladder {
    pub fn new(ttl: Option<Duration>) -> Self {
        Ladder {
            ttl,
            warn_at: WARN_DEFAULT,
            narrow_at: NARROW_DEFAULT,
        }
    }

    /// State at a given elapsed time — pure, clock-free for tests.
    pub fn state_at(&self, elapsed: Duration) -> State {
        let Some(ttl) = self.ttl else {
            return State::Alive;
        };
        if ttl.is_zero() {
            return State::Expired;
        }
        let used = elapsed.as_secs_f64() / ttl.as_secs_f64();
        if used >= 1.0 {
            State::Expired
        } else if used >= self.narrow_at {
            State::Narrow
        } else if used >= self.warn_at {
            State::Warn
        } else {
            State::Alive
        }
    }

    pub fn started() -> Ladder {
        Ladder::new(None)
    }

    /// The injectable message for a state transition, or None once delivered.
    pub fn inject(state: State) -> Option<&'static str> {
        match state {
            State::Alive => None,
            State::Warn => Some(WARN_INJECT),
            State::Narrow => Some(NARROW_INJECT),
            State::Expired => None,
        }
    }
}

/// Parse mayfly-style durations: "45m", "2h", "90s", "1d", "0s".
pub fn parse_ttl(s: &str) -> Result<Duration> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = num
        .parse()
        .map_err(|_| anyhow::anyhow!("bad ttl '{s}': not a number"))?;
    let secs = match unit.trim() {
        "s" | "" => n,
        "m" => n * 60,
        "h" => n * 3600,
        "d" => n * 86400,
        other => bail!("bad ttl '{s}': unknown unit '{other}' (s|m|h|d)"),
    };
    Ok(Duration::from_secs(secs))
}

/// Wall-clock check helper for the loop.
pub fn now() -> Instant {
    Instant::now()
}
