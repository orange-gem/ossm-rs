use std::{
    thread,
    time::{Duration, Instant, SystemTime},
};

use liveplot::{PlotPoint, PlotSink, Trace};
use ossm_motion::{
    config::MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS,
    motion_control::{
        MotionControl,
        debug::DebugOut,
        motor::Motor,
        timer::{Timer, TimerDuration, TimerInstant},
    },
};

pub async fn run_motion_control(plot_sink: PlotSink) {
    let motor = DummyMotor::new();
    let timer = StdTimer::new();
    let debug = PlotDebug::new(plot_sink);
    let mut motion_control = MotionControl::new_with_debug(motor, timer, debug);

    let mut interval = tokio::time::interval(Duration::from_millis(
        MOTION_CONTROL_LOOP_UPDATE_INTERVAL_MS,
    ));
    loop {
        interval.tick().await;
        motion_control.update_handler();
    }
}

struct DummyMotor {}

impl DummyMotor {
    pub fn new() -> Self {
        Self {}
    }
}

impl Motor for DummyMotor {
    type MotorError = ();

    fn min_consecutive_write_delay() -> TimerDuration {
        TimerDuration::millis(1)
    }

    fn set_absolute_position(&mut self, _steps: i32) -> Result<(), Self::MotorError> {
        Ok(())
    }

    fn set_max_allowed_output(&mut self, _output: u16) -> Result<(), Self::MotorError> {
        Ok(())
    }

    fn delay(&mut self, duration: TimerDuration) {
        thread::sleep(Duration::from_micros(duration.to_micros()));
    }
}

struct StdTimer {
    start_time: SystemTime,
}

impl StdTimer {
    pub fn new() -> Self {
        Self {
            start_time: SystemTime::now(),
        }
    }
}

impl Timer for StdTimer {
    fn now(&self) -> TimerInstant {
        let time = SystemTime::now().duration_since(self.start_time).unwrap();
        TimerInstant::from_ticks(time.as_micros() as u64)
    }
}

struct PlotDebug {
    start_time: Instant,
    position_trace: Trace,
    velocity_trace: Trace,
    acceleration_trace: Trace,
    jerk_trace: Trace,
    plot_sink: PlotSink,
}

impl PlotDebug {
    pub fn new(plot_sink: PlotSink) -> Self {
        Self {
            start_time: Instant::now(),
            position_trace: plot_sink.create_trace("position", None),
            velocity_trace: plot_sink.create_trace("velocity", None),
            acceleration_trace: plot_sink.create_trace("acceleration", None),
            jerk_trace: plot_sink.create_trace("jerk", None),
            plot_sink,
        }
    }
}

impl DebugOut for PlotDebug {
    fn new_position(&mut self, position: f64) {
        let time = self.start_time.elapsed().as_secs_f64();
        let point = PlotPoint {
            x: time,
            y: position,
        };
        self.plot_sink
            .send_point(&self.position_trace, point)
            .expect("Could not send position");
    }

    fn new_velocity(&mut self, velocity: f64) {
        let time = self.start_time.elapsed().as_secs_f64();
        let point = PlotPoint {
            x: time,
            y: velocity,
        };
        self.plot_sink
            .send_point(&self.velocity_trace, point)
            .expect("Could not send position");
    }

    fn new_acceleration(&mut self, acceleration: f64) {
        let time = self.start_time.elapsed().as_secs_f64();
        let point = PlotPoint {
            x: time,
            y: acceleration,
        };
        self.plot_sink
            .send_point(&self.acceleration_trace, point)
            .expect("Could not send position");
    }

    fn new_jerk(&mut self, jerk: f64) {
        let time = self.start_time.elapsed().as_secs_f64();
        let point = PlotPoint { x: time, y: jerk };
        self.plot_sink
            .send_point(&self.jerk_trace, point)
            .expect("Could not send position");
    }
}
