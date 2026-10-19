//! Driver bot tests (SPEC §9.7): pure-pursuit steering pins its sign (positive steer is
//! left, SPEC §4) and steers back to the centre line from either side; speed control
//! pushes towards the target; Hop is pressed once, at the right moment; and the hop
//! prediction the bot times itself with matches the real sim.

// Test helpers may panic on bad setup; that is a test failure, not a library bug.
#![allow(clippy::unwrap_used)]

mod common;

use common::{realistic, scenarios, vehicle};
use nl_sim::conventions::PHYSICS_HZ;
use nl_sim::course::{CourseSpec, GAUNTLET_JSON, PieceSpec};
use nl_sim::driver_bot::{
    BotView, Centerline, DriverBot, DriverBotConfig, HopTrigger, row_hop_triggers,
};
use nl_sim::hop_profile::HopProfile;
use nl_sim::input::{BUTTON_HOP, InputFrame};
use nl_sim::world::{DEFAULT_GROUND_FRICTION, World};

/// The repo's bot settings with the random wobble and jerks turned off.
fn quiet_config() -> DriverBotConfig {
    DriverBotConfig {
        steer_noise_sigma: 0.0,
        jerk_chance_per_s: 0.0,
        hop_timing_noise_s: 0.0,
        ..scenarios().driver_bot
    }
}

fn bot(target_speed_m_s: f32, triggers: &[HopTrigger]) -> DriverBot {
    DriverBot::new(
        quiet_config(),
        &vehicle(),
        Centerline::along_x(),
        target_speed_m_s,
        triggers,
        0,
    )
}

/// A car at `z_m` across the track, heading along +X at `speed_m_s`.
fn view_at(z_m: f32, speed_m_s: f32) -> BotView {
    BotView {
        position_m: [0.0, 0.6, z_m],
        forward: [1.0, 0.0, 0.0],
        right: [0.0, 0.0, 1.0],
        speed_m_s,
        hop_active: false,
        on_wheels: true,
    }
}

#[test]
fn a_car_right_of_the_centre_line_steers_left() {
    // Right is +Z (SPEC §4); left steering is positive.
    let steer = bot(15.0, &[]).pursuit_steer(&view_at(3.0, 15.0));

    assert!(steer > 0.05, "steer {steer}");
}

#[test]
fn a_car_left_of_the_centre_line_steers_right() {
    let steer = bot(15.0, &[]).pursuit_steer(&view_at(-3.0, 15.0));

    assert!(steer < -0.05, "steer {steer}");
}

#[test]
fn a_car_on_the_line_and_heading_along_it_steers_straight() {
    let steer = bot(15.0, &[]).pursuit_steer(&view_at(0.0, 15.0));

    assert!(steer.abs() < 1e-4, "steer {steer}");
}

#[test]
fn a_car_turned_left_of_the_course_steers_right() {
    // Nose turned 30° to the left (towards −Z).
    let yaw_rad = 30f32.to_radians();
    let view = BotView {
        forward: [yaw_rad.cos(), 0.0, -yaw_rad.sin()],
        right: [yaw_rad.sin(), 0.0, yaw_rad.cos()],
        ..view_at(0.0, 15.0)
    };

    let steer = bot(15.0, &[]).pursuit_steer(&view);

    assert!(steer < -0.1, "steer {steer}");
}

#[test]
fn in_the_air_it_centres_the_steering() {
    let flying = BotView {
        on_wheels: false,
        ..view_at(3.0, 15.0)
    };

    let input = bot(15.0, &[]).input(&flying);

    assert_eq!(input.steer, 0);
}

#[test]
fn below_the_target_speed_it_accelerates_and_above_it_brakes() {
    let slow = bot(20.0, &[]).input(&view_at(0.0, 10.0));
    let fast = bot(20.0, &[]).input(&view_at(0.0, 30.0));

    assert!(slow.throttle > 0);
    assert!(fast.throttle < 0);
}

#[test]
fn a_timed_hop_taps_the_button_until_a_hop_starts_then_stops() {
    let mut bot = bot(10.0, &[HopTrigger::AtTime { time_s: 0.5 }]);
    let half_second = PHYSICS_HZ as usize / 2;
    let mut presses_before = 0;
    for _ in 0..half_second {
        presses_before += bot.input(&view_at(0.0, 10.0)).is_pressed(BUTTON_HOP) as u32;
    }

    let first = bot.input(&view_at(0.0, 10.0));
    let second = bot.input(&view_at(0.0, 10.0));
    let third = bot.input(&view_at(0.0, 10.0));
    let started = BotView {
        hop_active: true,
        ..view_at(0.0, 10.0)
    };
    let after_start: Vec<InputFrame> = (0..60).map(|_| bot.input(&started)).collect();

    assert_eq!(presses_before, 0);
    assert!(first.is_pressed(BUTTON_HOP));
    assert!(
        !second.is_pressed(BUTTON_HOP),
        "released so the next press is new"
    );
    assert!(
        third.is_pressed(BUTTON_HOP),
        "pressed again: no Hop started yet"
    );
    assert!(
        after_start
            .iter()
            .all(|input| !input.is_pressed(BUTTON_HOP))
    );
}

#[test]
fn a_row_hop_fires_when_the_row_is_its_lead_time_ahead() {
    let trigger = HopTrigger::BeforeRow {
        middle_m: 30.0,
        end_m: 35.0,
        lead_s: 1.0,
    };
    // At 10 m/s the press comes 10 m before the middle, at x = 20 m.
    let early = |x_m: f32| BotView {
        position_m: [x_m, 0.6, 0.0],
        ..view_at(0.0, 10.0)
    };

    let at_15 = bot(10.0, &[trigger]).input(&early(15.0));
    let at_21 = bot(10.0, &[trigger]).input(&early(21.0));
    let past_the_row = bot(10.0, &[trigger]).input(&early(36.0));

    assert!(!at_15.is_pressed(BUTTON_HOP));
    assert!(at_21.is_pressed(BUTTON_HOP));
    assert!(!past_the_row.is_pressed(BUTTON_HOP), "too late: gave up");
}

#[test]
fn row_hops_come_in_track_order_whatever_the_file_order() {
    let row = |x_m: f32| PieceSpec::CarRow {
        pos: [x_m, 0.0, 0.0],
        yaw: 0.0,
        count: 1,
        height: 1.2,
        hop_hint: true,
    };
    let mut course = CourseSpec::from_json_str(GAUNTLET_JSON).unwrap();
    course.pieces = vec![row(300.0), row(100.0), row(200.0)];
    let profile = HopProfile::new(&vehicle(), &realistic());

    let triggers = row_hop_triggers(&course, &Centerline::of_course(&course), &profile, 0.1);

    let middles: Vec<f32> = triggers
        .iter()
        .map(|trigger| match trigger {
            HopTrigger::BeforeRow { middle_m, .. } => *middle_m,
            HopTrigger::AtTime { .. } => f32::NAN,
        })
        .collect();
    assert_eq!(middles, vec![102.25, 202.25, 302.25]);
}

#[test]
fn the_same_seed_drives_the_same_wobble() {
    let config = scenarios().driver_bot;
    let make = || DriverBot::new(config, &vehicle(), Centerline::along_x(), 15.0, &[], 42);
    let (mut first, mut second) = (make(), make());

    for _ in 0..600 {
        assert_eq!(
            first.input(&view_at(0.0, 15.0)),
            second.input(&view_at(0.0, 15.0))
        );
    }
}

#[test]
fn the_predicted_hop_apex_is_a_little_below_the_sims() {
    let mut world = World::new_on_flat_ground(&vehicle(), realistic(), DEFAULT_GROUND_FRICTION, 0);
    for _ in 0..120 {
        world.step(InputFrame::default());
    }
    let rest_m = world.chassis_pose().position_m[1];
    let hop = InputFrame {
        buttons: BUTTON_HOP,
        ..InputFrame::default()
    };
    world.step(hop);
    let mut apex_m: f32 = 0.0;
    for _ in 0..240 {
        world.step(InputFrame::default());
        apex_m = apex_m.max(world.chassis_pose().position_m[1] - rest_m);
    }

    let predicted_m = HopProfile::new(&vehicle(), &realistic()).apex_m();

    // The springs push the car off the ground, which the prediction leaves out: it
    // errs low, so Stage C never places a row the hop can't clear.
    assert!(
        predicted_m <= apex_m && apex_m - predicted_m < 0.4,
        "predicted {predicted_m} m, sim {apex_m} m"
    );
}

#[test]
fn a_hop_has_no_window_above_its_apex() {
    let profile = HopProfile::new(&vehicle(), &realistic());
    let apex_m = profile.apex_m();

    let below = profile.clear_window(apex_m * 0.5).unwrap();

    assert!(profile.clear_window(apex_m + 0.1).is_none());
    assert!(below.from_s < below.until_s);
}
