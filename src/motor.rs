use defmt::error;
use embedded_io::{Read, Write};
use enum_iterator::Sequence;
use esp_hal::{timer::OneShotTimer, uart::Uart, Blocking};
use heapless::Vec;
use rmodbus::{client::ModbusRequest, guess_response_frame_len, ModbusProto};

const PROTO: ModbusProto = ModbusProto::Rtu;
const MIN_REG_READ_REQUIRED: usize = 3;

const MAX_REG_READ_AT_ONCE: usize = 8;

pub const MAX_MOTOR_SPEED_RPM: u16 = 3000;

#[derive(Clone, Copy, defmt::Format, PartialEq, Sequence)]
#[repr(u16)]
pub enum ReadWriteMotorRegisters {
    ModbusEnable = 0x00,
    DriverOutputEnable = 0x01,
    MotorTargetSpeed = 0x02,
    MotorAcceleration = 0x03,
    WeakMagneticAngle = 0x04,
    SpeedRingProportionalCoefficient = 0x05,
    SpeedLoopIntegrationTime = 0x06,
    PositionRingProportionalCoefficient = 0x07,
    SpeedFeedForward = 0x08,
    DirPolarity = 0x09,
    ElectronicGearNumerator = 0x0A,
    ElectronicGearDenominator = 0x0B,
    ParameterSaveFlag = 0x14,
    AbsolutePositionLowU16 = 0x16,
    AbsolutePositionHighU16 = 0x17,
    StandstillMaxOutput = 0x18,
    SpecificFunction = 0x19,
}

#[derive(Clone, Copy, defmt::Format, PartialEq, Sequence)]
#[repr(u16)]
pub enum ReadOnlyMotorRegisters {
    TargetPositionLowU16 = 0x0C,
    TargetPositionHighU16 = 0x0D,
    AlarmCode = 0x0E,
    SystemCurrent = 0x0F,
    MotorCurrentSpeed = 0x10,
    SystemVoltage = 0x11,
    SystemTemperature = 0x12,
    SystemOutputPwm = 0x13,
    DeviceAddress = 0x15,
}

pub trait ReadableMotorRegister {
    fn addr(&self) -> u16;
}

impl ReadableMotorRegister for ReadWriteMotorRegisters {
    fn addr(&self) -> u16 {
        *self as u16
    }
}

impl ReadableMotorRegister for ReadOnlyMotorRegisters {
    fn addr(&self) -> u16 {
        *self as u16
    }
}

// Taken from the rmodbus crate
fn calc_crc16(frame: &[u8], data_length: u8) -> u16 {
    let mut crc: u16 = 0xffff;
    for i in frame.iter().take(data_length as usize) {
        crc ^= u16::from(*i);
        for _ in (0..8).rev() {
            if (crc & 0x0001) == 0 {
                crc >>= 1;
            } else {
                crc >>= 1;
                crc ^= 0xA001;
            }
        }
    }
    crc
}

pub struct Motor {
    rs485: Uart<'static, Blocking>,
    timer: OneShotTimer<'static, Blocking>,
}

impl Motor {
    pub fn new(rs485: Uart<'static, Blocking>, timer: OneShotTimer<'static, Blocking>) -> Self {
        Self { rs485, timer }
    }

    /// Write one motor register
    pub fn write_register(&mut self, reg: &ReadWriteMotorRegisters, val: u16) {
        let mut modbus_req = ModbusRequest::new(1, PROTO);
        let mut request: Vec<u8, 32> = Vec::new();

        modbus_req
            .generate_set_holding(reg.addr(), val, &mut request)
            .expect("Failed to generate reg write request");

        self.rs485
            .write_all(&request)
            .expect("Failed to write the request bytes to RS485");

        let mut response = [0u8; 32];
        self.rs485
            .read_exact(&mut response[0..MIN_REG_READ_REQUIRED])
            .expect("Failed to read the first response bytes");

        let len = guess_response_frame_len(&response[0..MIN_REG_READ_REQUIRED], PROTO)
            .expect("Failed to guess frame len") as usize;
        if len > MIN_REG_READ_REQUIRED {
            self.rs485
                .read_exact(&mut response[MIN_REG_READ_REQUIRED..len])
                .expect("Failed to read the remaining response bytes");
        }
        let response = &response[0..len];

        modbus_req.parse_ok(response).expect("Modbus error");

        // Make sure that multiple operations in a row can succeed
        self.timer.delay_millis(1);
    }

    /// Read one or more motor registers
    pub fn read_registers<T: ReadableMotorRegister>(
        &mut self,
        reg: &T,
        count: u16,
    ) -> Vec<u16, MAX_REG_READ_AT_ONCE> {
        let mut modbus_req = ModbusRequest::new(1, PROTO);
        let mut request: Vec<u8, 32> = Vec::new();

        modbus_req
            .generate_get_holdings(reg.addr(), count, &mut request)
            .expect("Failed to generate reg read request");

        // info!("Req {:x}", request);
        self.rs485
            .write_all(&request)
            .expect("Failed to write the request bytes to RS485");

        let mut response = [0u8; 32];
        self.rs485
            .read_exact(&mut response[0..MIN_REG_READ_REQUIRED])
            .expect("Failed to read the first response bytes");

        let len = guess_response_frame_len(&response[0..MIN_REG_READ_REQUIRED], PROTO)
            .expect("Failed to guess frame len") as usize;
        if len > MIN_REG_READ_REQUIRED {
            self.rs485
                .read_exact(&mut response[MIN_REG_READ_REQUIRED..len])
                .expect("Failed to read the remaining response bytes");
        }
        let response = &response[0..len];

        // modbus_req.parse_ok(response).expect("Modbus error");

        let mut res: Vec<u16, MAX_REG_READ_AT_ONCE> = Vec::new();
        modbus_req
            .parse_u16(response, &mut res)
            .expect("Failed to parse response reg");

        // Make sure that multiple operations in a row can succeed
        self.timer.delay_millis(1);

        res
    }

    /// Read one motor register
    pub fn read_register<T: ReadableMotorRegister>(&mut self, reg: &T) -> u16 {
        self.read_registers(reg, 1)[0]
    }

    /// Set the absolute position using the custom 0x7b command
    pub fn set_absolute_position(&mut self, position: i32) {
        let mut request = [0u8; 8];
        let bytes = position.to_be_bytes();

        request[0] = 0x1;
        request[1] = 0x7b;
        request[2..6].copy_from_slice(&bytes);
        let crc = calc_crc16(&request[0..6], 6).to_le_bytes();
        request[6..8].copy_from_slice(&crc);

        // info!("Request {:x}", request);

        self.rs485
            .write_all(&request)
            .expect("Failed to write the request bytes to RS485");

        // TODO: Add read timeout
        let mut response = [0u8; 32];
        self.rs485
            .read_exact(&mut response[0..8])
            .expect("Failed to read the first response bytes");

        if response[0..2] != [0x1, 0x7b] {
            error!(
                "Incorrect response to a 0x7b command: {:x}",
                &response[0..8]
            );
        }
        // Not necessary because we prioritise the update rate over missed positions
        // self.timer.delay_millis(1);
    }

    /// Enables modbus
    pub fn enable_modbus(&mut self, enable: bool) {
        self.write_register(&ReadWriteMotorRegisters::ModbusEnable, enable as u16);
    }

    // ---- RO regs ----

    /// Get the current in A
    pub fn get_current(&mut self) -> f32 {
        let reg = self.read_register(&ReadOnlyMotorRegisters::SystemCurrent);
        reg as f32 / 2000.0
    }

    /// Get the voltage in V
    pub fn get_voltage(&mut self) -> f32 {
        let reg = self.read_register(&ReadOnlyMotorRegisters::SystemVoltage);
        reg as f32 / 327.0
    }

    /// Get how many steps need to be taken to reach the target
    pub fn get_target_position(&mut self) -> i32 {
        let regs = self.read_registers(&ReadOnlyMotorRegisters::TargetPositionLowU16, 2);
        let bytes = (((regs[1] as u32) << 16) | regs[0] as u32).to_le_bytes();
        i32::from_le_bytes(bytes)
    }

    // ---- RW regs ----

    pub fn get_target_speed(&mut self) -> u16 {
        self.read_register(&ReadWriteMotorRegisters::MotorTargetSpeed)
    }

    /// Set the target speed in RPM 0-3000
    pub fn set_target_speed(&mut self, speed: u16) {
        if speed > 3000 {
            panic!("The speed cannot be more than 3000")
        }

        self.write_register(&ReadWriteMotorRegisters::MotorTargetSpeed, speed);
    }

    /// 0-59999. 60000 means disabled
    pub fn set_target_acceleration(&mut self, acceleration: u16) {
        self.write_register(&ReadWriteMotorRegisters::MotorAcceleration, acceleration);
    }

    pub fn set_speed_proportional_coefficient(&mut self, coefficient: u16) {
        self.write_register(
            &ReadWriteMotorRegisters::SpeedRingProportionalCoefficient,
            coefficient,
        );
    }

    pub fn set_position_proportional_coefficient(&mut self, coefficient: u16) {
        self.write_register(
            &ReadWriteMotorRegisters::PositionRingProportionalCoefficient,
            coefficient,
        );
    }

    /// 0 - Applying external DIR results in anticlockwise rotation
    /// 1 - Applying external DIR results in clockwise rotation
    pub fn set_dir_polarity(&mut self, polarity: bool) {
        self.write_register(&ReadWriteMotorRegisters::DirPolarity, polarity as u16);
    }

    /// Get the absolute position in encoder pulses
    pub fn get_abolute_position(&mut self) -> i32 {
        let regs = self.read_registers(&ReadWriteMotorRegisters::AbsolutePositionLowU16, 2);
        let bytes = (((regs[1] as u32) << 16) | regs[0] as u32).to_le_bytes();
        i32::from_le_bytes(bytes)
    }

    pub fn get_max_allowed_output(&mut self) -> u16 {
        self.read_register(&ReadWriteMotorRegisters::StandstillMaxOutput)
    }

    pub fn set_max_allowed_output(&mut self, output: u16) {
        self.write_register(&ReadWriteMotorRegisters::StandstillMaxOutput, output);
    }

    /// Home automatically
    pub fn home(&mut self) {
        self.write_register(&ReadWriteMotorRegisters::SpecificFunction, 1);
    }
}
