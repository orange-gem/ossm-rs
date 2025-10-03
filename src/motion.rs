use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use defmt::info;
use embassy_time::{Duration, Ticker, Timer};

use num_traits::float::Float;

use crate::{
    config::{MAX_MOVE_MM, RETRACT_VELOCITY, REVERSE_DIRECTION},
    motion_control::MotionControl,
    motor::{Motor, MAX_MOTOR_SPEED_RPM},
    pattern::{Pattern, PatternExecutor, PatternInput, PatternMove, MAX_SENSATION, MIN_SENSATION},
};

static DEPTH: AtomicU32 = AtomicU32::new(0);
static MOTION_LENGTH: AtomicU32 = AtomicU32::new(0);
static VELOCITY: AtomicU32 = AtomicU32::new(0);
static SENSATION: AtomicI32 = AtomicI32::new(0);
static PATTERN: AtomicU32 = AtomicU32::new(0);
static MOTION_ENABLED: AtomicBool = AtomicBool::new(false);

/// Set the default motor settings
pub fn set_motor_settings(motor: &mut Motor) {
    // Set high speed and acceleration since those are controlled by motion control
    motor.set_target_speed(MAX_MOTOR_SPEED_RPM);
    motor.set_target_acceleration(50000);

    // Defaults from OSSM
    motor.set_speed_proportional_coefficient(3000);
    motor.set_position_proportional_coefficient(3000);
    motor.set_max_allowed_output(600);
}

/// Home and wait until done
pub async fn wait_for_home(motor: &mut Motor) {
    // Remember the original values
    let target_speed = motor.get_target_speed();
    let max_allowed_output = motor.get_max_allowed_output();

    // Set slower speed and output for homing
    motor.set_target_speed(80);
    motor.set_max_allowed_output(89);
    motor.set_dir_polarity(REVERSE_DIRECTION);

    motor.home();

    info!("Homing...");
    loop {
        info!("Target {}", motor.get_target_position());
        if motor.get_target_position().abs() < 15 {
            info!("Homing Done");
            break;
        }
        Timer::after(Duration::from_millis(100)).await;
    }

    // Restore the original values
    motor.set_target_speed(target_speed);
    motor.set_max_allowed_output(max_allowed_output);
}

#[embassy_executor::task]
pub async fn run_motion() {
    let mut ticker = Ticker::every(Duration::from_millis(30));
    let mut prev_motion_enabled = false;

    let mut pattern_executor = PatternExecutor::new();
    let mut prev_pattern: u32 = 0;
    let mut pattern_move = PatternMove::default();

    info!("Task Motion Started");

    loop {
        let motion_enabled = MOTION_ENABLED.load(Ordering::Acquire);

        // Retract the machine if motion was disabled
        if !motion_enabled && prev_motion_enabled {
            // Retract the machine
            MotionControl::set_max_velocity(RETRACT_VELOCITY);
            MotionControl::set_target_position(0.0);
            while MotionControl::is_move_in_progress() {
                Timer::after(Duration::from_millis(10)).await;
            }
            // Restore the previous velocity
            let velocity = VELOCITY.load(Ordering::Acquire);
            MotionControl::set_max_velocity(velocity as f64);
        }

        if !MotionControl::is_move_in_progress() && motion_enabled {
            // Apply the delay from the previous move before executing the next one
            Timer::after_millis(pattern_move.delay_ms).await;

            let depth = DEPTH.load(Ordering::Acquire) as f64;
            let motion_length = MOTION_LENGTH.load(Ordering::Acquire) as f64;
            let velocity = VELOCITY.load(Ordering::Acquire) as f64;
            let sensation = SENSATION.load(Ordering::Acquire) as f64;
            let pattern = PATTERN.load(Ordering::Acquire);

            if pattern != prev_pattern {
                pattern_executor.set_pattern(pattern);
                prev_pattern = pattern;
            }

            let input = PatternInput {
                velocity,
                depth,
                motion_length,
                sensation,
            };

            pattern_move = pattern_executor.next_move(&input);

            if pattern_move.position < 0.0 {
                pattern_move.position = 0.0;
            }
            MotionControl::set_max_velocity(pattern_move.velocity);
            MotionControl::set_target_position(pattern_move.position);
        }
        ticker.next().await;

        prev_motion_enabled = motion_enabled;
    }
}

/// Set the motion start in mm
pub fn set_motion_length(length: u32) {
    MOTION_LENGTH.store(length, Ordering::Release);
}

/// Set the motion depth in mm
pub fn set_motion_depth(mut depth: u32) {
    if depth as f32 > MAX_MOVE_MM {
        depth = MAX_MOVE_MM.floor() as u32;
    }
    DEPTH.store(depth, Ordering::Release);
}

/// Set the motion velocity in mm/s
pub fn set_motion_velocity(velocity: u32) {
    VELOCITY.store(velocity, Ordering::Release);
    MotionControl::set_max_velocity(velocity as f64);
}

/// Set the motion velocity in mm/s
pub fn set_motion_sensation(mut sensation: i32) {
    if (sensation as f64) > MAX_SENSATION {
        sensation = MAX_SENSATION.floor() as i32;
    }
    if (sensation as f64) < MIN_SENSATION {
        sensation = MIN_SENSATION.ceil() as i32;
    }

    SENSATION.store(sensation, Ordering::Release);
}

pub fn set_motion_pattern(index: u32) {
    PATTERN.store(index, Ordering::Release);
}

pub fn set_motion_enabled(enabled: bool) {
    MOTION_ENABLED.store(enabled, Ordering::Release);
}
