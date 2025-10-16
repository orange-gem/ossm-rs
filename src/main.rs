#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

mod board;
mod config;
mod motion;
mod motion_control;
mod motor;
mod pattern;
mod remote;
mod utils;

use crate::board::Pins;
use crate::config::{MOTOR_BAUD_RATE, STOCK_MOTOR_BAUD_RATE};
use crate::remote::{
    ble::{ble_events, ble_task},
    esp_now::{m5_heartbeat, m5_heartbeat_check, m5_listener},
};

use crate::motion::{run_motion, set_motor_settings, wait_for_home};
use crate::motion_control::MotionControl;
use crate::motor::{Motor, ReadOnlyMotorRegisters, ReadWriteMotorRegisters};
use config::{CONNECTIONS_MAX, L2CAP_CHANNELS_MAX};
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use esp_hal::gpio::{Level, Output};
use esp_hal::{
    clock::CpuClock,
    gpio::Pin,
    interrupt::software::SoftwareInterruptControl,
    interrupt::Priority,
    peripherals::Peripherals,
    system::Stack,
    timer::{timg::TimerGroup, PeriodicTimer},
    uart::{self, Instance, Uart},
};
use esp_radio::{
    ble::controller::BleConnector,
    esp_now::{EspNowManager, EspNowSender},
    Controller,
};
use esp_rtos::embassy::InterruptExecutor;
use static_cell::StaticCell;
use trouble_host::{
    prelude::{DefaultPacketPool, ExternalController},
    Host, HostResources,
};

use {esp_backtrace as _, esp_println as _};

use enum_iterator::all;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 128 * 1024);

    #[cfg(feature = "board_waveshare")]
    let pins = {
        info!("Board: WaveShare");
        Pins {
            rs485_rx: peripherals.GPIO18.degrade(),
            rs485_tx: peripherals.GPIO17.degrade(),
            rs485_dtr: Some(peripherals.GPIO21.degrade()),
        }
    };

    #[cfg(feature = "board_custom")]
    let pins = {
        info!("Board: custom");
        Pins {
            rs485_rx: peripherals.GPIO35.degrade(),
            rs485_tx: peripherals.GPIO37.degrade(),
            rs485_dtr: Some(peripherals.GPIO36.degrade()),
        }
    };

    #[cfg(feature = "board_atom_s3")]
    let pins = {
        info!("Board: Atom S3");
        Pins {
            rs485_rx: peripherals.GPIO5.degrade(),
            rs485_tx: peripherals.GPIO6.degrade(),
            // rs485_dtr: None,
            rs485_dtr: Some(peripherals.GPIO36.degrade()),
        }
    };

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    static APP_CORE_STACK: StaticCell<Stack<16384>> = StaticCell::new();
    let app_core_stack = APP_CORE_STACK.init(Stack::new());

    // The regular executor seems to freeze
    // Use an interrupt executor instead
    static EXECUTOR_CORE_1: StaticCell<InterruptExecutor<2>> = StaticCell::new();

    static MOTION_INIT_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();

    // All the peripherals are initialised on the core that they will be used on
    let second_core_function = move || {
        let rs485_rx_confg = uart::RxConfig::default();
        let rs485_config = uart::Config::default()
            .with_rx(rs485_rx_confg)
            .with_baudrate(MOTOR_BAUD_RATE.as_int());

        let mut rs485 = Uart::new(peripherals.UART1, rs485_config)
            .expect("Failed to initialise RS485")
            .with_rx(pins.rs485_rx)
            .with_tx(pins.rs485_tx);

        if let Some(dtr) = pins.rs485_dtr {
            rs485 = rs485.with_dtr(dtr);
        }

        unsafe {
            let rs = Peripherals::steal().UART1;
            let regs = rs.info().regs();
            regs.rs485_conf()
                .modify(|_, w| w.rs485_en().set_bit().dl1_en().set_bit());
        }

        let timg1 = TimerGroup::new(peripherals.TIMG1);

        // Wait for the motor to boot up

        let mut motor = Motor::new(rs485, timg1.timer0.into());
        motor.delay(esp_hal::time::Duration::from_millis(500));

        // Try to read a register to see if the motor is online
        if let Err(err) = motor.get_abolute_position() {
            error!(
                "Failed to communicate with the motor ({}). Trying to change baud rate",
                err
            );

            // Give the motor time to cool down from the high baud rate
            // Timer::after_millis(100).await;

            let (mut rs485, motor_timer) = motor.release();

            let slow_rs485_config = rs485_config.with_baudrate(STOCK_MOTOR_BAUD_RATE.as_int());
            rs485
                .apply_config(&slow_rs485_config)
                .expect("Failed to change RS485 config");

            let mut motor = Motor::new(rs485, motor_timer);

            motor
                .set_baud_rate(MOTOR_BAUD_RATE)
                .expect("Failed to set the new motor baud rate");

            error!("Motor baudrate updated. Please power cycle the machine!");

            loop {}
        }

        for x in all::<ReadOnlyMotorRegisters>() {
            let val = motor.read_register(&x).expect("Could not read register");
            info!("Reg {} val {}", x, val);
        }

        for x in all::<ReadWriteMotorRegisters>() {
            let val = motor.read_register(&x).expect("Could not read register");
            info!("Reg {} val {}", x, val);
        }

        wait_for_home(&mut motor);

        set_motor_settings(&mut motor);

        let update_timer = PeriodicTimer::new(timg1.timer1);
        MotionControl::init(update_timer, motor);

        let executor_core1 = InterruptExecutor::new(sw_int.software_interrupt2);
        let executor_core1 = EXECUTOR_CORE_1.init(executor_core1);
        let spawner = executor_core1.start(Priority::Priority1);

        spawner.spawn(run_motion()).ok();

        MOTION_INIT_SIGNAL.signal(true);

        loop {}
    };

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        #[cfg(target_arch = "xtensa")]
        sw_int.software_interrupt0,
        sw_int.software_interrupt1,
        app_core_stack,
        second_core_function,
    );

    MOTION_INIT_SIGNAL.wait().await;

    Timer::after(Duration::from_millis(1000)).await;

    let radio = &*mk_static!(
        Controller<'static>,
        esp_radio::init().expect("Failed to initialize WIFI/BLE controller")
    );

    let wifi = peripherals.WIFI;
    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(radio, wifi, Default::default()).unwrap();
    wifi_controller
        .set_mode(esp_radio::wifi::WifiMode::Sta)
        .unwrap();
    wifi_controller.start().unwrap();

    let esp_now = interfaces.esp_now;
    info!("esp-now version {}", esp_now.version().unwrap());

    let (manager, sender, receiver) = esp_now.split();
    let manager = mk_static!(EspNowManager<'static>, manager);
    let sender = mk_static!(
        Mutex::<NoopRawMutex, EspNowSender<'static>>,
        Mutex::<NoopRawMutex, _>::new(sender)
    );

    let bluetooth = peripherals.BT;
    let connector = BleConnector::new(radio, bluetooth, Default::default());
    let bt_controller: ExternalController<_, 20> = ExternalController::new(connector);

    let resources = mk_static!(HostResources<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX>, HostResources::new());
    let stack = mk_static!(
        trouble_host::Stack<
            'static,
            ExternalController<BleConnector<'static>, 20>,
            DefaultPacketPool,
        >,
        trouble_host::new(bt_controller, resources)
    );

    let Host {
        peripheral, runner, ..
    } = stack.build();

    spawner.spawn(m5_listener(manager, sender, receiver)).ok();
    spawner.spawn(m5_heartbeat(manager, sender)).ok();
    spawner.spawn(m5_heartbeat_check()).ok();

    spawner.spawn(ble_task(runner)).ok();
    spawner.spawn(ble_events(stack, peripheral)).ok();

    loop {
        // ESP-NOW does not work without this
        Timer::after(Duration::from_millis(5000)).await;
    }
}
