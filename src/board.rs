use esp_hal::{
    gpio::{AnyPin, Pin},
};

pub struct Pins {
    pub rs485_rx: AnyPin<'static>,
    pub rs485_tx: AnyPin<'static>,
    pub rs485_dtr: Option<AnyPin<'static>>,
}
