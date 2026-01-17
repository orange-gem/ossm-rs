mod motion_control;

use crate::motion_control::run_motion_control;

use ossm_motion::motion::{motion_state, run_motion};
use tokio::{task};

use liveplot::{LivePlotConfig, channel_plot, run_liveplot};


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let (sink, rx) = channel_plot();

    let _motion_control = task::spawn(run_motion_control(sink));
    let _motion = task::spawn(run_motion());

    motion_state::set_motion_enabled(true);
    motion_state::set_motion_depth_pct(100);
    motion_state::set_motion_length_pct(100);
    motion_state::set_motion_velocity_pct(50);
    motion_state::set_motion_pattern(3);

    run_liveplot(rx, LivePlotConfig::default()).unwrap();
    Ok(())
}
