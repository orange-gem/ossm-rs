use defmt::info;
use embassy_time::{Duration, Ticker, Timer};
pub mod motion_state;

use crate::{
    config::{MIN_MOVE_MM, RETRACT_VELOCITY, REVERSE_DIRECTION, STEPS_PER_MM},
    motion::motion_state::{get_motion_state, MachineMotionState},
    motion_control::MotionControl,
    motor::{Motor, MAX_MOTOR_SPEED_RPM},
    pattern::{Pattern, PatternExecutor, PatternInput, PatternMove},
};

/// Set the default motor settings
pub fn set_motor_settings(motor: &mut Motor) {
    // Set high speed and acceleration since those are controlled by motion control
    motor
        .set_target_speed(MAX_MOTOR_SPEED_RPM)
        .expect("Failed to set target speed");
    motor
        .set_target_acceleration(50000)
        .expect("Failed to set target acceleration");

    // Defaults from OSSM
    motor
        .set_speed_proportional_coefficient(3000)
        .expect("Failed to set speed proportional coefficient");
    motor
        .set_position_proportional_coefficient(3000)
        .expect("Failed to set position proportional coefficient");
    motor
        .set_max_allowed_output(600)
        .expect("Failed to set max allowed output");
}

/// Home and wait until done
pub fn wait_for_home(motor: &mut Motor) {
    // Set slower speed and output for homing
    motor
        .set_target_speed(80)
        .expect("Failed to set target speed");
    motor
        .set_max_allowed_output(89)
        .expect("Failed to set max allowed output");
    motor
        .set_dir_polarity(REVERSE_DIRECTION)
        .expect("Failed to set direction polarity");

    motor.home().expect("Failed to start homing");

    info!("Homing...");
    motor.wait_for_target_reached(15);
    info!("Homing Done");

    motor.delay(esp_hal::time::Duration::from_millis(20));

    // Enabling modbus seems to reset the target speed and the max allowed output to default
    motor.enable_modbus(true).expect("Failed to enable modbus");

    motor.delay(esp_hal::time::Duration::from_millis(800));

    let mut new_steps = MIN_MOVE_MM * STEPS_PER_MM;
    if !REVERSE_DIRECTION {
        new_steps = -new_steps;
    }

    motor
        .set_target_speed(100)
        .expect("Failed to set target speed");
    motor
        .set_absolute_position(new_steps as i32)
        .expect("Failed to move to the minimum position");

    motor.delay(esp_hal::time::Duration::from_millis(20));

    motor.wait_for_target_reached(15);

    info!("Moved to minimum position");
}

async fn retract() {
    let motion_state: MachineMotionState = get_motion_state().into();

    MotionControl::set_max_velocity(RETRACT_VELOCITY);
    MotionControl::set_target_position(MIN_MOVE_MM);
    while MotionControl::is_move_in_progress() {
        Timer::after(Duration::from_millis(10)).await;
    }
    // Restore the previous velocity
    MotionControl::set_max_velocity(motion_state.velocity);
}

#[embassy_executor::task]
pub async fn run_motion() {
    let mut ticker = Ticker::every(Duration::from_millis(30));
    let mut prev_motion_enabled = false;

    let mut pattern_executor = PatternExecutor::new();
    let mut prev_pattern: u32 = 0;
    let mut pattern_move = PatternMove::default();
    let mut prev_pattern_move = PatternMove::default();
    // Values to be overriden on the first move
    prev_pattern_move.velocity = -420.0;
    prev_pattern_move.torque = -420.0;

    info!("Task Motion Started");

    loop {
        let motion_state: MachineMotionState = get_motion_state().into();

        // Retract the machine if motion was disabled
        if !motion_state.motion_enabled && prev_motion_enabled {
            pattern_executor.reset();
            retract().await;
        }

        if motion_state.pattern != prev_pattern {
            pattern_executor.set_pattern(motion_state.pattern);
            pattern_executor.reset();
            info!(
                "Pattern set to: {}",
                pattern_executor.get_current_pattern_name()
            );
            prev_pattern = motion_state.pattern;
            // Always start the pattern from the retracted position
            retract().await;
        }

        if !MotionControl::is_move_in_progress() && motion_state.motion_enabled {
            // Apply the delay from the previous move before executing the next one
            Timer::after_millis(pattern_move.delay_ms).await;

            let input = PatternInput {
                velocity: motion_state.velocity,
                depth: motion_state.depth,
                motion_length: motion_state.motion_length,
                sensation: motion_state.sensation,
            };

            // A move with all the constraints met
            pattern_move = pattern_executor.next_move(&input);

            if pattern_move.velocity != prev_pattern_move.velocity {
                MotionControl::set_max_velocity(pattern_move.velocity);
            }
            if pattern_move.torque != prev_pattern_move.torque {
                MotionControl::set_torque(pattern_move.torque);
            }
            MotionControl::set_target_position(pattern_move.position);

            prev_pattern_move = pattern_move;
        }
        ticker.next().await;

        prev_motion_enabled = motion_state.motion_enabled;
    }
}
