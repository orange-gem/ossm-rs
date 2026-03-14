use ch224::{VoltageRequest, CH224};
use log::{info, error};
use esp_hal::{i2c::master::I2c, Blocking};
use uom::si::{electric_current::milliampere, electric_potential::millivolt};
use usbpd::protocol_layer::message::{Data::SourceCapabilities, Message, pdo::PowerDataObject::*, units::{ElectricCurrent, ElectricPotential}};

pub fn init(i2c: I2c<'static, Blocking>) {
    let mut ch224 = CH224::new(i2c);

    let status = ch224
        .read_status()
        .expect("Could not read the PD chip status");
    info!("Status {:?}", status);

    let caps = ch224.read_source_capabilities().unwrap();

    // let message = Message::from_bytes(&caps).expect("Could not parse PD capabilities");

    // if let Some(SourceCapabilities(x)) = message.data {
    //     info!("PD Capabilities:");
    //     let pdos = x.pdos();
    //     for pdo in pdos {
    //         match *pdo {
    //             FixedSupply(x) => {
    //                 let voltage = x.voltage();
    //                 let current = x.max_current();
    //                 let epr = x.epr_mode_capable();

    //                 let mv = voltage.get::<millivolt>();
    //                 let ma = current.get::<milliampere>();
    //                 info!("  * [{} mV {} mA] EPR: {}", mv, ma, epr);
    //             }
    //             _ => {
    //                 info!("{:?}", pdo);
    //             }
    //         }
    //     }
    // } else {
    //     panic!("Message not SourceCapabilities? {:?}", message);
    // }

    // ch224.request_voltage(VoltageRequest::Request28V).unwrap();

    // let status = ch224.read_status().expect("Could not read the PD chip status");
    // info!("Status {:?}", status);
}
