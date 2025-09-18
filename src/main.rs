#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

mod config;
mod m5_remote;
mod motion;
mod motion_control;
mod motor;

use core::ptr::addr_of_mut;

use crate::m5_remote::{m5_heartbeat, m5_heartbeat_check, m5_listener};
use crate::motion::{run_motion, set_motor_settings, wait_for_home};
use crate::motion_control::MotionControl;
use crate::motor::{Motor, ReadOnlyMotorRegisters, ReadWriteMotorRegisters};
use defmt::info;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use esp_hal::system::{CpuControl, Stack};
use esp_hal::{
    clock::CpuClock,
    peripherals::Peripherals,
    timer::{systimer::SystemTimer, timg::TimerGroup, OneShotTimer, PeriodicTimer},
    uart::{self, Instance, Uart},
};
use esp_hal_embassy::Executor;
use esp_radio::esp_now::{EspNowManager, EspNowSender};
use esp_radio::Controller;
use static_cell::StaticCell;

use {esp_backtrace as _, esp_println as _};

use enum_iterator::all;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static mut APP_CORE_STACK: Stack<16384> = Stack::new();

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    // generator version: 0.5.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_preempt::init(timg0.timer0);

    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);

    let rs485_rx_confg = uart::RxConfig::default().with_timeout(10);
    let rs485_config = uart::Config::default()
        .with_rx(rs485_rx_confg)
        .with_baudrate(19200);

    let rs485 = Uart::new(peripherals.UART1, rs485_config)
        .expect("Failed to initialise RS485")
        .with_rx(peripherals.GPIO35)
        .with_tx(peripherals.GPIO37)
        .with_dtr(peripherals.GPIO36);

    unsafe {
        let rs = Peripherals::steal().UART1;
        let regs = rs.info().regs();
        regs.rs485_conf()
            .modify(|_, w| w.rs485_en().set_bit().dl1_en().set_bit());
    }

    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let motor_timer = OneShotTimer::new(timg1.timer0);
    let mut motor = Motor::new(rs485, motor_timer);

    let esp_radio_ctrl = &*mk_static!(
        Controller<'static>,
        esp_radio::init().expect("Failed to initialize WIFI/BLE controller")
    );

    let wifi = peripherals.WIFI;
    let (mut controller, interfaces) = esp_radio::wifi::new(&esp_radio_ctrl, wifi).unwrap();
    controller.set_mode(esp_radio::wifi::WifiMode::Sta).unwrap();
    controller.start().unwrap();

    let esp_now = interfaces.esp_now;

    info!("esp-now version {}", esp_now.version().unwrap());

    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init([systimer.alarm0, systimer.alarm1]);

    info!("Embassy initialized!");

    let (manager, sender, receiver) = esp_now.split();
    let manager = mk_static!(EspNowManager<'static>, manager);
    let sender = mk_static!(
        Mutex::<NoopRawMutex, EspNowSender<'static>>,
        Mutex::<NoopRawMutex, _>::new(sender)
    );

    // Wait for the motor to boot up
    Timer::after(Duration::from_millis(500)).await;

    for x in all::<ReadOnlyMotorRegisters>() {
        let reg = motor.read_register(&x);
        info!("Reg {} val {}", x, reg);
    }

    for x in all::<ReadWriteMotorRegisters>() {
        let reg = motor.read_register(&x);
        info!("Reg {} val {}", x, reg);
    }

    wait_for_home(&mut motor).await;

    set_motor_settings(&mut motor);

    Timer::after(Duration::from_millis(1000)).await;

    let _guard = cpu_control
        .start_app_core(unsafe { &mut *addr_of_mut!(APP_CORE_STACK) }, move || {
            let update_timer = PeriodicTimer::new(timg1.timer1);
            MotionControl::init(update_timer, motor);

            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|spawner| {
                spawner.spawn(run_motion()).ok();
            });
        })
        .unwrap();

    Timer::after(Duration::from_millis(1000)).await;

    spawner.spawn(m5_listener(manager, sender, receiver)).ok();
    spawner.spawn(m5_heartbeat(manager, sender)).ok();
    spawner.spawn(m5_heartbeat_check()).ok();

    loop {
        // ESP-NOW does not work without this
        Timer::after(Duration::from_millis(5000)).await;
    }
}
