//! Muted-talk LED blink driver.
//!
//! Rides the main event loop's ~50 ms wake cadence (both platform adapters
//! tick at 50 ms): while the mic is muted and `blink_on_talk` is enabled,
//! the loop reads the input meters every [`METER_INTERVAL`] and blinks the
//! mute indicator while the level exceeds `talk_threshold` — the "you're
//! talking but muted" warning.
//!
//! The state machine here is pure (no device I/O); the caller performs the
//! [`LedAction`]s it returns, so transitions are unit-testable.

use std::time::{Duration, Instant};

/// How often to read the meters. The loop wakes every ~50 ms; Focusrite
/// Control 2 itself polls at ~22.5 Hz, so ~11 Hz is comfortably gentle.
pub const METER_INTERVAL: Duration = Duration::from_millis(90);
/// Blink phase length (on ↔ off cadence).
const BLINK_PERIOD: Duration = Duration::from_millis(250);
/// How long the input must stay below the threshold before the indicator
/// returns to solid. Bridges natural speech pauses.
const QUIET_HOLD: Duration = Duration::from_millis(700);

/// LED action the caller must perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedAction {
    /// Repaint the solid mute indicator.
    Solid,
    /// Turn the indicator LEDs off (blink off-phase).
    Off,
}

pub struct BlinkState {
    /// Talk detected — currently animating.
    blinking: bool,
    /// The current phase has the LEDs off.
    phase_off: bool,
    next_meter_at: Option<Instant>,
    next_toggle_at: Instant,
    /// When the quiet period ends the blink (None while talking).
    quiet_deadline: Option<Instant>,
}

impl BlinkState {
    pub fn new() -> Self {
        Self {
            blinking: false,
            phase_off: false,
            next_meter_at: None,
            next_toggle_at: Instant::now(),
            quiet_deadline: None,
        }
    }

    /// True when a fresh meter read is due.
    pub fn meter_due(&self, now: Instant) -> bool {
        self.next_meter_at.is_none_or(|at| now >= at)
    }

    /// Record that a meter read happened.
    pub fn note_meter_read(&mut self, now: Instant) {
        self.next_meter_at = Some(now + METER_INTERVAL);
    }

    /// Advance the state machine. `talking` is the thresholded verdict of a
    /// fresh meter read, or `None` when no reading happened this tick.
    pub fn advance(&mut self, now: Instant, talking: Option<bool>) -> Option<LedAction> {
        match talking {
            Some(true) => {
                self.quiet_deadline = None;
                if !self.blinking {
                    // Start blinking with an immediate off-phase for a
                    // visible reaction the moment talk is detected.
                    self.blinking = true;
                    self.phase_off = true;
                    self.next_toggle_at = now + BLINK_PERIOD;
                    return Some(LedAction::Off);
                }
            }
            Some(false) if self.blinking && self.quiet_deadline.is_none() => {
                self.quiet_deadline = Some(now + QUIET_HOLD);
            }
            Some(false) | None => {}
        }

        if !self.blinking {
            return None;
        }

        // Quiet long enough — stop blinking, restore the solid indicator.
        if self.quiet_deadline.is_some_and(|d| now >= d) {
            self.blinking = false;
            self.phase_off = false;
            self.quiet_deadline = None;
            return Some(LedAction::Solid);
        }

        // Toggle the blink phase.
        if now >= self.next_toggle_at {
            self.phase_off = !self.phase_off;
            self.next_toggle_at = now + BLINK_PERIOD;
            return Some(if self.phase_off {
                LedAction::Off
            } else {
                LedAction::Solid
            });
        }

        None
    }

    /// Reset when the blink preconditions vanish (feature disabled, device
    /// lost, or unmuted). Returns `Solid` when the LEDs may be stuck in the
    /// off-phase AND the mute indicator should still be showing; on unmute
    /// the regular clear path repaints, so no action is returned.
    pub fn reset(&mut self, still_muted: bool) -> Option<LedAction> {
        let was_off = self.blinking && self.phase_off;
        self.blinking = false;
        self.phase_off = false;
        self.quiet_deadline = None;
        self.next_meter_at = None;
        if was_off && still_muted {
            Some(LedAction::Solid)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn idle_until_talk_detected() {
        let mut b = BlinkState::new();
        let now = t0();
        assert_eq!(b.advance(now, Some(false)), None);
        assert_eq!(b.advance(now, None), None);
    }

    #[test]
    fn talk_starts_blink_with_off_phase() {
        let mut b = BlinkState::new();
        assert_eq!(b.advance(t0(), Some(true)), Some(LedAction::Off));
    }

    #[test]
    fn phases_alternate_on_period() {
        let mut b = BlinkState::new();
        let now = t0();
        assert_eq!(b.advance(now, Some(true)), Some(LedAction::Off));
        // Before the period elapses: no action.
        assert_eq!(
            b.advance(now + Duration::from_millis(100), Some(true)),
            None
        );
        // After: back to solid, then off again.
        assert_eq!(
            b.advance(now + Duration::from_millis(260), Some(true)),
            Some(LedAction::Solid)
        );
        assert_eq!(
            b.advance(now + Duration::from_millis(520), Some(true)),
            Some(LedAction::Off)
        );
    }

    #[test]
    fn short_pause_keeps_blinking() {
        let mut b = BlinkState::new();
        let now = t0();
        b.advance(now, Some(true));
        // Quiet, but within QUIET_HOLD — the blink keeps going.
        assert_eq!(
            b.advance(now + Duration::from_millis(260), Some(false)),
            Some(LedAction::Solid)
        );
        assert_eq!(
            b.advance(now + Duration::from_millis(520), Some(false)),
            Some(LedAction::Off)
        );
        // Talk resumes — deadline cleared, blinking continues.
        assert_eq!(
            b.advance(now + Duration::from_millis(600), Some(true)),
            None
        );
        assert_eq!(
            b.advance(now + Duration::from_millis(1400), Some(false)),
            Some(LedAction::Solid)
        );
    }

    #[test]
    fn sustained_quiet_restores_solid() {
        let mut b = BlinkState::new();
        let now = t0();
        b.advance(now, Some(true));
        b.advance(now + Duration::from_millis(100), Some(false));
        // Past QUIET_HOLD → Solid and no further actions.
        assert_eq!(
            b.advance(now + Duration::from_millis(900), Some(false)),
            Some(LedAction::Solid)
        );
        assert_eq!(
            b.advance(now + Duration::from_millis(2000), Some(false)),
            None
        );
    }

    #[test]
    fn no_reading_ticks_still_animate() {
        let mut b = BlinkState::new();
        let now = t0();
        b.advance(now, Some(true));
        // Meter not due this tick (talking=None) — phase still toggles.
        assert_eq!(
            b.advance(now + Duration::from_millis(260), None),
            Some(LedAction::Solid)
        );
    }

    #[test]
    fn reset_mid_off_phase_restores_solid_when_still_muted() {
        let mut b = BlinkState::new();
        b.advance(t0(), Some(true)); // off-phase
        assert_eq!(b.reset(true), Some(LedAction::Solid));
        // Idempotent afterwards.
        assert_eq!(b.reset(true), None);
    }

    #[test]
    fn reset_after_unmute_leaves_leds_to_clear_path() {
        let mut b = BlinkState::new();
        b.advance(t0(), Some(true)); // off-phase
        assert_eq!(b.reset(false), None);
    }

    #[test]
    fn meter_read_rate_limited() {
        let mut b = BlinkState::new();
        let now = t0();
        assert!(b.meter_due(now));
        b.note_meter_read(now);
        assert!(!b.meter_due(now + Duration::from_millis(50)));
        assert!(b.meter_due(now + METER_INTERVAL));
    }
}
