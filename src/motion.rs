use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use defmt::info;
use embassy_time::{Duration, Ticker, Timer};

use num_traits::float::Float;

use crate::{
    config::{MAX_MOVE_MM, RETRACT_VELOCITY, REVERSE_DIRECTION},
    motion_control::MotionControl,
    motor::{Motor, MAX_MOTOR_SPEED_RPM},
};

static MOTION_LENGTH: AtomicU32 = AtomicU32::new(0);
static DEPTH: AtomicU32 = AtomicU32::new(0);
static VELOCITY: AtomicU32 = AtomicU32::new(0);
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
    let mut ticker = Ticker::every(Duration::from_millis(10));
    let mut out_stroke = true;
    let mut prev_motion_enabled = false;

    info!("Motion started");

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
            out_stroke = true;
        }

        if !MotionControl::is_move_in_progress() && motion_enabled {
            let out_stroke_depth = DEPTH.load(Ordering::Acquire) as f64;

            let length = MOTION_LENGTH.load(Ordering::Acquire) as f64;
            let mut in_stroke_depth = out_stroke_depth - length;

            if in_stroke_depth < 0.0 {
                in_stroke_depth = 0.0;
            }

            if out_stroke {
                MotionControl::set_target_position(out_stroke_depth);
                // info!("OUT");
            } else {
                MotionControl::set_target_position(in_stroke_depth);
                // info!("IN");
            }
            out_stroke = !out_stroke;
        }
        ticker.next().await;
        // Timer::after(Duration::from_millis(10)).await;

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

pub fn set_motion_enabled(enabled: bool) {
    MOTION_ENABLED.store(enabled, Ordering::Release);
}
