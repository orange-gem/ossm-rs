use core::fmt::Write;

use defmt::{error, info};
use esp_radio::ble::controller::BleConnector;
use heapless::String;
use trouble_host::prelude::*;

use crate::{
    config::{MAX_MOVE_MM, MOTION_CONTROL_MAX_VELOCITY, MOTION_CONTROL_MIN_VELOCITY},
    motion::{
        set_motion_depth, set_motion_length, set_motion_pattern, set_motion_sensation,
        set_motion_velocity,
    },
    pattern::{PatternExecutor, MAX_SENSATION, MIN_SENSATION},
    utils::scale,
};

const MAX_COMMAND_LENGTH: usize = 64;

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
    current_state: String<32>,
    #[characteristic(uuid = "522b443a-4f53-534d-2000-420badbabe69", read)]
    pattern_list: String<256>,
}

#[embassy_executor::task]
pub async fn ble_events(
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
        match advertise("OSSM", &mut peripheral, &server).await {
            Ok(conn) => {
                if let Err(err) = gatt_events_task(&server, &conn).await {
                    panic!("[gatt] error: {:?}", err);
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
    conn: &GattConnection<'_, '_, P>,
) -> Result<(), Error> {
    let reason = loop {
        match conn.next().await {
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
    server: &'server Server<'values>,
) -> Result<GattConnection<'values, 'server, DefaultPacketPool>, BleHostError<C::Error>> {
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
    let conn = advertiser.accept().await?.with_attribute_server(server)?;
    info!("[adv] connection established");
    Ok(conn)
}

fn process_command(command: &String<MAX_COMMAND_LENGTH>, server: &Server<'_>) {
    let mut split_command = command.split(":");

    let mut fail = false;

    if let Some(cmd) = split_command.next() {
        if let Some(action) = split_command.next() {
            match cmd {
                "set" => {
                    if let Some(value) = split_command.next() {
                        if let Ok(value) = value.parse::<i32>() {
                            let value = value as f64;
                            match action {
                                "speed" => {
                                    let velocity = scale(
                                        value,
                                        0.0,
                                        100.0,
                                        MOTION_CONTROL_MIN_VELOCITY,
                                        MOTION_CONTROL_MAX_VELOCITY,
                                    );
                                    set_motion_velocity(velocity as u32);
                                }
                                "stroke" => {
                                    let stroke = scale(value, 0.0, 100.0, 0.0, MAX_MOVE_MM as f64);
                                    set_motion_length(stroke as u32);
                                }
                                "depth" => {
                                    let depth = scale(value, 0.0, 100.0, 0.0, MAX_MOVE_MM as f64);
                                    set_motion_depth(depth as u32);
                                }
                                "sensation" => {
                                    let sensation =
                                        scale(value, 0.0, 100.0, MIN_SENSATION, MAX_SENSATION);
                                    set_motion_sensation(sensation as i32);
                                }
                                "pattern" => {
                                    set_motion_pattern(value as u32);
                                }
                                _ => {
                                    error!("Invalid set command");
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
                "go" => {}
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
        if let Err(_) = response_str.write_str(command.as_str()) {
            response_str
                .write_str("overflow")
                .expect("Should always fit");
        }
    } else {
        response_str.write_str("ok:").expect("Should always fit");
        if let Err(_) = response_str.write_str(command.as_str()) {
            response_str
                .write_str("overflow")
                .expect("Should always fit");
        }
    }
    if let Err(err) = server.set(&server.ossm_service.primary_command, &response_str) {
        error!("Failed to write the response to a set command {:?}", err);
    }
}
