use core::{
    cell::RefCell,
    panic,
    sync::atomic::{AtomicBool, Ordering},
};

use critical_section::Mutex;
use defmt::{debug, error, info};
use esp_hal::{handler, interrupt::Priority, time::Instant, timer::PeriodicTimer, Blocking};
use rsruckig::prelude::*;

use crate::{config::*, motor::Motor, utils::{saturate_range, scale}};

static UPDATE_TIMER: Mutex<RefCell<Option<PeriodicTimer<'static, Blocking>>>> =
    Mutex::new(RefCell::new(None));
static MOTION_CONTROL: Mutex<RefCell<Option<MotionControl>>> = Mutex::new(RefCell::new(None));

static MOVE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

// Whether to panic on the thresholds being exceeded by motion control
// If false the values will be capped to the allowed limits, but the execution will continue
const PANIC_ON_EXCEEEDED: bool = false;

// Timer interrupt
#[handler(priority = Priority::Priority2)]
pub fn motion_control_interrupt() {
    critical_section::with(|cs| {
        UPDATE_TIMER
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .clear_interrupt();
        MOTION_CONTROL
            .borrow_ref_mut(cs)
            .as_mut()
            .unwrap()
            .update_handler();
    });
}

pub struct MotionControl {
    motor: Motor,
    ruckig: Ruckig<1, ThrowErrorHandler>,
    input: InputParameter<1>,
    output: OutputParameter<1>,
    last_update: Instant,
}

impl MotionControl {
    /// Initialises the MotionControl and allows the use of attached functions
    pub fn init(mut update_timer: PeriodicTimer<'static, Blocking>, mut motor: Motor) {
        info!("Motion Control Init");

        // Motion control over modbus
        motor.enable_modbus(true).expect("Failed to enable modbus");

        update_timer.set_interrupt_handler(motion_control_interrupt);
        update_timer.listen();

        let mut input = InputParameter::new(None);

        input.current_position[0] = MIN_MOVE_MM;
        input.max_velocity[0] = MOTION_CONTROL_MAX_VELOCITY;
        input.max_acceleration[0] = MOTION_CONTROL_MAX_ACCELERATION;
        input.max_jerk[0] = MOTION_CONTROL_MAX_JERK;
        input.synchronization = Synchronization::None;
        input.duration_discretization = DurationDiscretization::Discrete;

        let motion_control = Self {
            motor,
            ruckig: Ruckig::<1, ThrowErrorHandler>::new(
                None,
                MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS as f64 / 1000.0,
            ),
            input,
            output: OutputParameter::new(None),
            last_update: Instant::now(),
        };

        critical_section::with(|cs| {
            MOTION_CONTROL.borrow_ref_mut(cs).replace(motion_control);
            UPDATE_TIMER.borrow_ref_mut(cs).replace(update_timer);
        });
    }

    /// The handler that must be called every MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS
    /// This is handled by the UPDATE_TIMER interrupt
    pub fn update_handler(&mut self) {
        if MOVE_IN_PROGRESS.load(Ordering::Acquire) {
            let before = Instant::now();
            let res = self.ruckig.update(&self.input, &mut self.output);

            let since_last = self.last_update.elapsed().as_micros();
            self.last_update = Instant::now();

            match res {
                Ok(ok) => {
                    match ok {
                        RuckigResult::Working => {
                            let mut new_position = self.output.new_position[0];

                            // Saturate the position if out of bounds
                            let mut exceeded = false;
                            if new_position < MIN_MOVE_MM {
                                error!(
                                    "Motion control exceeded the min allowed move ({} < {})",
                                    new_position, MIN_MOVE_MM
                                );
                                new_position = MIN_MOVE_MM;
                                exceeded = true;
                            }

                            if new_position > MAX_MOVE_MM {
                                error!(
                                    "Motion control exceeded the max allowed move ({} > {})",
                                    new_position, MAX_MOVE_MM
                                );
                                new_position = MAX_MOVE_MM;
                                exceeded = true;
                            }

                            if exceeded && PANIC_ON_EXCEEEDED {
                                panic!("Motion control thresholds were exceeded. See above ^");
                            }

                            let mut new_steps = new_position * STEPS_PER_MM;
                            if !REVERSE_DIRECTION {
                                new_steps = -new_steps;
                            }
                            if let Err(err) = self.motor.set_absolute_position(new_steps as i32) {
                                error!("Failed to set motor position {}", err);
                            }

                            debug!("Set motor position {}", new_position);

                            // info!("PROG");
                            self.output.pass_to_input(&mut self.input);
                        }
                        RuckigResult::Finished => {
                            MOVE_IN_PROGRESS.store(false, Ordering::Release);
                            // Stop the timer until next move
                            critical_section::with(|cs| {
                                UPDATE_TIMER
                                    .borrow_ref_mut(cs)
                                    .as_mut()
                                    .unwrap()
                                    .cancel()
                                    .ok();
                            });
                            // info!("DONE");
                        }
                        _ => {
                            error!("Error!");
                        }
                    }
                }
                Err(err) => {
                    error!("Ruckig Error {}", defmt::Debug2Format(&err));
                }
            }
            let duration_ms = before.elapsed().as_millis();
            debug!(
                "Update took: {} ms Since last call: {} us",
                duration_ms, since_last
            );

            if duration_ms > MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS {
                error!(
                    "Update took longer than the update interval {} > {}",
                    duration_ms, MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS
                );
            }
        }
    }

    /// MotionControl::init() must be called once before calling this
    /// Otherwise this will panic!
    pub fn set_target_position(position: f64) {
        critical_section::with(|cs| {
            let mut motion_control = MOTION_CONTROL.borrow_ref_mut(cs);
            let motion_control = motion_control.as_mut().unwrap();

            // info!("Going to a new target position {}", position as f32);
            motion_control.input.target_position[0] = position;
            motion_control.output.time = 0.0;

            MOVE_IN_PROGRESS.store(true, Ordering::Release);

            // Start the timer to run the control loop until the move is done
            UPDATE_TIMER
                .borrow_ref_mut(cs)
                .as_mut()
                .unwrap()
                .start(esp_hal::time::Duration::from_millis(
                    MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS,
                ))
                .expect("Could not start motor update timer");
        });
    }

    /// Set the maximum velocity for the move
    pub fn set_max_velocity(mut max_velocity: f64) {
        critical_section::with(|cs| {
            let mut motion_control = MOTION_CONTROL.borrow_ref_mut(cs);
            let motion_control = motion_control.as_mut().unwrap();

            // A velocity of 0 breaks motion control
            // Set some small minimum velocity
            if max_velocity < MOTION_CONTROL_MIN_VELOCITY {
                max_velocity = MOTION_CONTROL_MIN_VELOCITY;
            }

            if max_velocity <= MOTION_CONTROL_MAX_VELOCITY {
                motion_control.input.max_velocity[0] = max_velocity;
            } else {
                error!(
                    "Velocity {} is larger than allowed {}",
                    max_velocity, MOTION_CONTROL_MAX_VELOCITY
                );
                motion_control.input.max_velocity[0] = MOTION_CONTROL_MAX_VELOCITY;
            }
            motion_control.output.time = 0.0;
        });
    }

    /// Set the maximum torque for the move in %
    pub fn set_torque(max_torque: f64) {
        let mut torque = saturate_range(max_torque, 0.0, 100.0);
        torque = scale(
            torque,
            0.0,
            100.0,
            MOTOR_MIN_OUTPUT,
            MOTOR_MAX_OUTPUT,
        );
        // The last digit is 0 for no alarm
        torque = torque * 10.0;

        info!("Torque set to {}", torque as u16);

        critical_section::with(|cs| {
            let mut motion_control = MOTION_CONTROL.borrow_ref_mut(cs);
            let motion_control = motion_control.as_mut().unwrap();

            motion_control.motor.set_max_allowed_output(torque as u16).expect("Failed to set max allowed output (torque)");
        });
    }

    pub fn is_move_in_progress() -> bool {
        MOVE_IN_PROGRESS.load(Ordering::Acquire)
    }
}
