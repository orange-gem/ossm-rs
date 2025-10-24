use crate::motor::MotorBaudRate;

// ---- User Parameters ----
const PULLEY_TOOTH_COUNT: f64 = 20.0;
const BELT_PITCH: f64 = 2.0;
// The minimum allowed move forward from the homing position
pub const MIN_MOVE_MM: f64 = 10.0;
// The maximum allowed move forward from the homing position
pub const MAX_MOVE_MM: f64 = 190.0;
// The max total travel distance of the machine
pub const MAX_TRAVEL_MM: f64 = MAX_MOVE_MM - MIN_MOVE_MM;
// The velocity at which the machine retracts when it is turned off
// or switching to a different a pattern in mm/s
pub const RETRACT_VELOCITY: f64 = MOTION_CONTROL_MAX_VELOCITY / 4.0;
// Change this if your machine is going the wrong way
pub const REVERSE_DIRECTION: bool = false;
// The baud rate that your motor comes with. Will be automatically changed at startup
pub const STOCK_MOTOR_BAUD_RATE: MotorBaudRate = MotorBaudRate::Baud19200;

// ---- Critical parameters. No touchy unless you know what you are doing ----
// Using the full encoder resolution
const MOTOR_STEPS_PER_REVOLUTION: f64 = 32768.0;
// How often the motion control loop runs
pub const MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS: u64 = 10;
// In mm/s
// Has to be larger than 0
pub const MOTION_CONTROL_MIN_VELOCITY: f64 = 1.0;
// In mm/s
pub const MOTION_CONTROL_MAX_VELOCITY: f64 = 600.0;
// In mm/s²
pub const MOTION_CONTROL_MAX_ACCELERATION: f64 = 10000.0;
// In mm/s³
pub const MOTION_CONTROL_MAX_JERK: f64 = 30000.0;
// Turn the machine off after no heartbeat was received for this long
pub const MAX_NO_REMOTE_HEARTBEAT_MS: u64 = 8000;
// Motor baud rate to be used by the firmware
pub const MOTOR_BAUD_RATE: MotorBaudRate = MotorBaudRate::Baud115200;

// ---- BLE parameters ----
pub const CONNECTIONS_MAX: usize = 1;
pub const L2CAP_CHANNELS_MAX: usize = 2;

// ---- Calculated parameters ----
pub const STEPS_PER_MM: f64 = MOTOR_STEPS_PER_REVOLUTION / (PULLEY_TOOTH_COUNT * BELT_PITCH);
pub const MM_PER_ROTATION: f64 = MOTOR_STEPS_PER_REVOLUTION / STEPS_PER_MM;
pub const MAX_RPM: u16 = ((MOTION_CONTROL_MAX_VELOCITY / STEPS_PER_MM) * 60.0) as u16;
