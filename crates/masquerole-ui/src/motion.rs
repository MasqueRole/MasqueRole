//! Motion: how the interface moves.
//!
//! Every animation in the design system goes through here, so it feels like one hand made
//! it: a strong ease-out curve, short durations, a press that gives a little, and exits that
//! are quicker than entrances. Anything done many times a day (list rows, tabs, menus)
//! changes color only. Movement is kept for what is seen occasionally.
//!
//! With reduced motion on (see [`set_reduced`]) nothing moves or scales; fades and color
//! changes stay, because they help the eye follow a change.

use egui::{Context, Id};

/// How far a pressed control shrinks.
pub const PRESS_SCALE: f32 = 0.97;

/// Seconds a press takes to shrink the control.
pub const PRESS_SECONDS: f32 = 0.1;

/// Seconds the release takes. Faster to settle than to press, so the control feels alive.
pub const RELEASE_SECONDS: f32 = 0.16;

/// Seconds something takes to enter, and to leave. A leaving thing is quicker: the system is
/// responding, not asking for attention.
pub const ENTER_SECONDS: f32 = 0.22;
/// See [`ENTER_SECONDS`].
pub const EXIT_SECONDS: f32 = 0.15;

/// The curve `cubic-bezier(0.23, 1, 0.32, 1)`: starts fast and settles gently.
const EASE_OUT: (f32, f32, f32, f32) = (0.23, 1.0, 0.32, 1.0);

/// Where the "reduce motion" flag lives in the context.
fn flag_id() -> Id {
    Id::unique("masquerole-ui-reduce-motion")
}

/// Turn reduced motion on or off for the whole interface. Call once per frame.
pub fn set_reduced(ctx: &Context, reduced: bool) {
    ctx.data_mut(|d| d.insert_temp(flag_id(), reduced));
}

/// Whether reduced motion is on.
pub fn reduced(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(flag_id())).unwrap_or(false)
}

/// Evaluate a cubic Bezier easing curve at progress `x` (0 to 1), by bisection.
fn bezier(curve: (f32, f32, f32, f32), x: f32) -> f32 {
    let (x1, y1, x2, y2) = curve;
    let at = |t: f32, a: f32, b: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    let x = x.clamp(0.0, 1.0);
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if at(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at((lo + hi) / 2.0, y1, y2)
}

/// The strong ease-out curve applied to a linear progress from 0 to 1.
pub fn ease_out(progress: f32) -> f32 {
    bezier(EASE_OUT, progress)
}

/// Eased progress (0 to 1) of something that is `on` or off, for movement and scale.
///
/// It retargets from where it is when `on` flips mid-way, so quick toggling stays smooth.
/// Entering takes `enter` seconds and leaving `exit`. With reduced motion it jumps to the end
/// at once, so nothing moves.
pub fn eased(ctx: &Context, id: Id, on: bool, enter: f32, exit: f32) -> f32 {
    if reduced(ctx) {
        return f32::from(u8::from(on));
    }
    let seconds = if on { enter } else { exit };
    ease_out(ctx.animate_bool_with_time(id, on, seconds))
}

/// Linear progress (0 to 1) for fades: opacity and color keep animating with reduced motion.
pub fn fade(ctx: &Context, id: Id, on: bool, seconds: f32) -> f32 {
    ctx.animate_bool_with_time(id, on, seconds)
}

/// The scale of a control that is `pressed`: [`PRESS_SCALE`] while pressed, 1 at rest, and
/// always 1 with reduced motion.
pub fn press_scale(ctx: &Context, id: Id, pressed: bool) -> f32 {
    if reduced(ctx) {
        return 1.0;
    }
    let progress = eased(ctx, id, pressed, PRESS_SECONDS, RELEASE_SECONDS);
    1.0 - (1.0 - PRESS_SCALE) * progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_runs_from_zero_to_one_and_leads_a_straight_line() {
        assert!(ease_out(0.0).abs() < 1e-3);
        assert!((ease_out(1.0) - 1.0).abs() < 1e-3);
        // Ease-out is ahead of linear for most of the way: the movement is quick up front.
        for p in [0.1, 0.25, 0.5, 0.75] {
            assert!(ease_out(p) > p, "{p}");
        }
        // And it never goes backward.
        let mut last = 0.0;
        for i in 0..=100 {
            let v = ease_out(i as f32 / 100.0);
            assert!(v >= last - 1e-4);
            last = v;
        }
    }

    #[test]
    fn reduced_motion_removes_movement_at_once() {
        let ctx = Context::default();
        set_reduced(&ctx, true);
        assert!(reduced(&ctx));
        assert_eq!(eased(&ctx, Id::unique("a"), true, 0.2, 0.1), 1.0);
        assert_eq!(eased(&ctx, Id::unique("a"), false, 0.2, 0.1), 0.0);
        assert_eq!(press_scale(&ctx, Id::unique("b"), true), 1.0);
        set_reduced(&ctx, false);
        assert!(!reduced(&ctx));
    }

    #[test]
    fn a_press_shrinks_a_little_and_never_below_the_limit() {
        let ctx = Context::default();
        assert_eq!(press_scale(&ctx, Id::unique("p"), false), 1.0);
        let pressed = press_scale(&ctx, Id::unique("p"), true);
        assert!((PRESS_SCALE..=1.0).contains(&pressed), "{pressed}");
    }
}
