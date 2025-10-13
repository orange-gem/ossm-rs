use esp_hal::gpio::AnyPin;

pub struct Pins {
    pub rs485_rx: AnyPin<'static>,
    pub rs485_tx: AnyPin<'static>,
    pub rs485_transmit_enable: Option<AnyPin<'static>>,
    pub rs485_receive_enable_inv: Option<AnyPin<'static>>,
}
