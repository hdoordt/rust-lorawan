use embedded_hal::digital::v2::OutputPin;
use hal::gpio::*;
use hal::pac;
use hal::prelude::*;
use hal::rcc::APB2;
use hal::spi::{Mode as SpiMode, Phase, Polarity, Spi, Spi1NoRemap};

use nb::block;
use stm32f1xx_hal as hal;
use sx12xx::{AntPinsMode, BoardBindings};

type Uninitialized = Input<Floating>;

pub type RadioIRQ = gpiob::PB4<Input<PullUp>>;

pub fn initialize_irq() {
    todo!("Configure interrupt on dio0_pin's rising edge (EXTI15_10)");
}

pub type TcxoEn = gpioa::PA8<Output<PushPull>>;

pub fn new(
    spi_peripheral: pac::SPI1,
    rcc_apb2: &mut APB2,
    spi_sck: gpioa::PA5<Uninitialized>,
    spi_miso: gpioa::PA6<Uninitialized>,
    spi_mosi: gpioa::PA7<Uninitialized>,
    spi_nss_pin: gpiob::PB0<Uninitialized>,
    reset: gpiob::PB1<Uninitialized>,
    gpioa_crl: &mut gpioa::CRL,
    gpiob_crl: &mut gpiob::CRL,
    mapr: &mut hal::afio::MAPR,
    clocks: hal::rcc::Clocks,
) -> BoardBindings {
    let spi_pins = (
        spi_sck.into_alternate_push_pull(gpioa_crl),  // D13
        spi_miso.into_floating_input(gpioa_crl),      // D12
        spi_mosi.into_alternate_push_pull(gpioa_crl), // D11
    );

    let spi_mode = SpiMode {
        polarity: Polarity::IdleLow,
        phase: Phase::CaptureOnFirstTransition,
    };

    // store all of the necessary pins and peripherals into statics
    // this is necessary as the extern C functions need access
    // this is safe, thanks to ownership and because these statics are private
    unsafe {
        let spi1 = Spi::spi1(
            spi_peripheral,
            spi_pins,
            mapr,
            spi_mode,
            100.khz(),
            clocks,rcc_apb2,
        );
        SPI = Some(spi1);
        SPI_NSS = Some(spi_nss_pin.into_push_pull_output(gpiob_crl));
        RESET = Some(reset.into_push_pull_output(gpiob_crl));
    };

    BoardBindings {
        reset: Some(radio_reset),
        spi_in_out: Some(spi_in_out),
        spi_nss: Some(spi_nss),
        delay_ms: Some(delay_ms),
        set_antenna_pins: Some(set_antenna_pins),
        set_board_tcxo: None,
        busy_pin_status: None,
        reduce_power: None,
    }
}

static mut EN_TCXO: Option<TcxoEn> = None;

#[no_mangle]
pub extern "C" fn set_tcxo(value: bool) -> u8 {
    unsafe {
        if let Some(pin) = &mut EN_TCXO {
            if value {
                pin.set_high().unwrap();
            } else {
                pin.set_low().unwrap();
            }
        }
    }
    6
}

type SpiPort = hal::spi::Spi<
    pac::SPI1,
    Spi1NoRemap,
    (
        gpioa::PA5<Alternate<PushPull>>,
        gpioa::PA6<Input<Floating>>,
        gpioa::PA7<Alternate<PushPull>>,
    ),
>;
static mut SPI: Option<SpiPort> = None;
#[no_mangle]
extern "C" fn spi_in_out(out_data: u8) -> u8 {
    unsafe {
        if let Some(spi) = &mut SPI {
            spi.send(out_data).unwrap();
            block!(spi.read()).unwrap()
        } else {
            0
        }
    }
}

static mut SPI_NSS: Option<gpiob::PB0<Output<PushPull>>> = None;
#[no_mangle]
extern "C" fn spi_nss(value: bool) {
    unsafe {
        if let Some(pin) = &mut SPI_NSS {
            if value {
                pin.set_high().unwrap();
            } else {
                pin.set_low().unwrap();
            }
        }
    }
}

static mut RESET: Option<gpiob::PB1<Output<PushPull>>> = None;
#[no_mangle]
extern "C" fn radio_reset(value: bool) {
    unsafe {
        if let Some(pin) = &mut RESET {
            if value {
                pin.set_low().unwrap();
            } else {
                pin.set_high().unwrap();
            }
        }
    }
}

#[no_mangle]
extern "C" fn delay_ms(ms: u32) {
    cortex_m::asm::delay(ms);
}

pub extern "C" fn set_antenna_pins(mode: AntPinsMode, _power: u8) {
    // TODO: Do we need this declaration if there are no antenna pins?
}
