use core::fmt::Write;

use defmt::{error, info};
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Ticker, Timer};
use esp_radio::ble::controller::BleConnector;
use heapless::String;
use trouble_host::prelude::*;

use crate::{
    motion::motion_state::{
        get_motion_state, set_motion_depth_pct, set_motion_enabled, set_motion_length_pct,
        set_motion_pattern, set_motion_sensation_pct, set_motion_velocity_pct,
    },
    pattern::PatternExecutor,
};

pub const MAX_COMMAND_LENGTH: usize = 64;
pub const MAX_STATE_LENGTH: usize = 128;
pub const MAX_PATTERN_LENGTH: usize = 256;

#[gatt_server]
struct Server {
    ossm_service: OssmService,
}

#[gatt_service(uuid = "522b443a-4f53-534d-0001-420badbabe69")]
struct OssmService {
    #[characteristic(uuid = "522b443a-4f53-534d-0002-420badbabe69", read, write)]
    primary_command: String<MAX_COMMAND_LENGTH>,
    #[characteristic(uuid = "522b443a-4f53-534d-0010-420badbabe69", read, write)]
    speed_knob_characteristic: String<16>,
    #[characteristic(uuid = "522b443a-4f53-534d-1000-420badbabe69", read, notify)]
    current_state: String<MAX_STATE_LENGTH>,
    #[characteristic(uuid = "522b443a-4f53-534d-2000-420badbabe69", read)]
    pattern_list: String<MAX_PATTERN_LENGTH>,
}

#[embassy_executor::task]
pub async fn ble_events(
    stack: &'static Stack<
        'static,
        ExternalController<BleConnector<'static>, 20>,
        DefaultPacketPool,
    >,
    mut peripheral: Peripheral<
        'static,
        ExternalController<BleConnector<'static>, 20>,
        DefaultPacketPool,
    >,
) {
    info!("Starting advertising and GATT service");
    let server = Server::new_with_config(GapConfig::Peripheral(PeripheralConfig {
        name: "OSSM",
        appearance: &appearance::motorized_device::GENERIC_MOTORIZED_DEVICE,
    }))
    .unwrap();

    loop {
        match advertise("OSSM", &mut peripheral).await {
            Ok(connection) => {
                Timer::after_millis(100).await;

                connection
                    .set_phy(stack, PhyKind::Le2M)
                    .await
                    .expect("Could not set 2M PHY");

                let connect_params = ConnectParams {
                    min_connection_interval: Duration::from_micros(7500),
                    max_connection_interval: Duration::from_micros(7500),
                    ..Default::default()
                };
                connection
                    .update_connection_params(stack, &connect_params)
                    .await
                    .expect("Failed to update connection params");

                Timer::after_millis(100).await;

                let phy = connection.read_phy(stack).await.unwrap();
                let mtu = connection.att_mtu();
                info!("PHY {} MTU {}", phy, mtu);

                let gatt_connection = connection
                    .with_attribute_server(&server)
                    .expect("Could not transform connection into GATT connection");

                let events = gatt_events_task(&server, &gatt_connection);
                let notify = state_notifications(&server, &gatt_connection);

                match select(events, notify).await {
                    Either::First(res) => {
                        if let Err(err) = res {
                            panic!("[gatt] error in events task: {:?}", err);
                        }
                    }
                    Either::Second(res) => {
                        if let Err(err) = res {
                            panic!("[gatt] error in notify task: {:?}", err);
                        }
                    }
                }
            }
            Err(err) => {
                panic!("[adv] error: {:?}", err);
            }
        }
    }
}

#[embassy_executor::task]
pub async fn ble_task(
    mut runner: Runner<'static, ExternalController<BleConnector<'static>, 20>, DefaultPacketPool>,
) {
    loop {
        if let Err(err) = runner.run().await {
            panic!("[ble_task] error: {:?}", err);
        }
    }
}

async fn gatt_events_task<P: PacketPool>(
    server: &Server<'_>,
    connection: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let reason = loop {
        match connection.next().await {
            GattConnectionEvent::Disconnected { reason } => break reason,
            GattConnectionEvent::Gatt { event } => {
                let mut write = false;
                let mut event_handle = 0;
                match &event {
                    GattEvent::Read(event) => {
                        if event.handle() == server.ossm_service.pattern_list.handle {
                            let patterns = PatternExecutor::new().get_all_patterns_json();
                            server.set(&server.ossm_service.pattern_list, &patterns)?;
                        }
                        if event.handle() == server.ossm_service.current_state.handle {
                            let state: String<MAX_STATE_LENGTH> = get_motion_state().as_json();
                            server.set(&server.ossm_service.current_state, &state)?;
                        }
                    }
                    GattEvent::Write(event) => {
                        write = true;
                        event_handle = event.handle();
                    }
                    _ => {}
                };
                // This step is also performed at drop(), but writing it explicitly is necessary
                // in order to ensure reply is sent.
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(e) => {
                        error!("[gatt] error sending response: {:?}", e);
                    }
                };

                // This is here because the event needs to be accepted before the data can be accessed
                if write {
                    if event_handle == server.ossm_service.primary_command.handle {
                        let command: String<64> =
                            server.get(&server.ossm_service.primary_command)?;

                        process_command(&command, server);
                    }
                }
            }
            _ => {} // ignore other Gatt Connection Events
        }
    };
    info!("[gatt] disconnected: {:?}", reason);
    Ok(())
}

/// Create an advertiser to use to connect to a BLE Central, and wait for it to connect.
async fn advertise<'values, 'server, C: Controller>(
    name: &'values str,
    peripheral: &mut Peripheral<'values, C, DefaultPacketPool>,
) -> Result<Connection<'values, DefaultPacketPool>, BleHostError<C::Error>> {
    let mut advertiser_data = [0; 31];
    let len = AdStructure::encode_slice(
        &[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            AdStructure::ServiceUuids16(&[[0x0f, 0x18]]),
            AdStructure::CompleteLocalName(name.as_bytes()),
        ],
        &mut advertiser_data[..],
    )?;
    let advertiser = peripheral
        .advertise(
            &Default::default(),
            Advertisement::ConnectableScannableUndirected {
                adv_data: &advertiser_data[..len],
                scan_data: &[],
            },
        )
        .await?;
    info!("[adv] advertising");
    let conn = advertiser.accept().await?;
    info!("[adv] connection established");
    Ok(conn)
}

async fn state_notifications<P: PacketPool>(
    server: &Server<'_>,
    connection: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let mut ticker = Ticker::every(Duration::from_millis(500));
    loop {
        let state: String<MAX_STATE_LENGTH> = get_motion_state().as_json();
        server
            .ossm_service
            .current_state
            .notify(connection, &state)
            .await?;
        ticker.next().await;
    }
}

fn process_command(command: &String<MAX_COMMAND_LENGTH>, server: &Server<'_>) {
    info!("BLE Command {}", command);

    let mut split_command = command.split(":");

    let mut fail = false;

    if let Some(cmd) = split_command.next() {
        if let Some(action) = split_command.next() {
            match cmd {
                "set" => {
                    if let Some(value) = split_command.next() {
                        if let Ok(value) = value.parse::<u32>() {
                            match action {
                                "speed" => {
                                    set_motion_velocity_pct(value);
                                }
                                "stroke" => {
                                    set_motion_length_pct(value);
                                }
                                "depth" => {
                                    set_motion_depth_pct(value);
                                }
                                "sensation" => {
                                    set_motion_sensation_pct(value);
                                }
                                "pattern" => {
                                    set_motion_pattern(value);
                                }
                                _ => {
                                    error!("Invalid set command {}", action);
                                    fail = true;
                                }
                            }
                        } else {
                            error!("Could not parse set value");
                            fail = true;
                        };
                    } else {
                        error!("No value after set");
                        fail = true;
                    }
                }
                "go" => match action {
                    "simplePenetration" => {
                        set_motion_enabled(true);
                    }
                    "strokeEngine" => {
                        set_motion_enabled(true);
                    }
                    "menu" => {
                        set_motion_enabled(false);
                    }
                    _ => {
                        error!("Invalid go command {}", action);
                        fail = true;
                    }
                },
                _ => {
                    error!("Command neither set nor go");
                    fail = true;
                }
            }
        } else {
            error!("No action in command");
            fail = true;
        }
    } else {
        error!("Invalid command");
        fail = true;
    }

    let mut response_str: String<MAX_COMMAND_LENGTH> = String::new();
    if fail {
        response_str.write_str("fail:").expect("Should always fit");
        if response_str.write_str(command.as_str()).is_err() {
            response_str
                .write_str("overflow")
                .expect("Should always fit");
        }
    } else {
        response_str.write_str("ok:").expect("Should always fit");
        if response_str.write_str(command.as_str()).is_err() {
            response_str
                .write_str("overflow")
                .expect("Should always fit");
        }
    }
    if let Err(err) = server.set(&server.ossm_service.primary_command, &response_str) {
        error!("Failed to write the response to a set command {:?}", err);
    }
}
