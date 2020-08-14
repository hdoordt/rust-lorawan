use hal::crc::Crc;
use hal::gpio::*;
use hal::prelude::*;
use hal::rtc::Rtc;
use hal::stm32 as pac;
use stm32f1xx_hal as hal;

type Adc = hal::adc::Adc<pac::ADC1>;
type AdcPin = gpioa::PA1<Analog>;

/// Simple, non-production ready random number generator based on the RTC and the ADC,
/// Uses the CRC for amplification.
pub struct Rng {
    rtc: Rtc,
    adc: Adc,
    adc_pin: AdcPin,
    crc: Crc,
}

impl Rng {
    pub fn new(rtc: Rtc, adc: Adc, adc_pin: AdcPin, crc: Crc) -> Self {
        Self {
            rtc,
            adc,
            adc_pin,
            crc,
        }
    }

    pub fn rand_u32(&mut self) -> u32 {
        self.crc.write(self.rtc.current_time());

        for _ in 0..8 {
            let val = self.read_adc() | (self.read_adc() << 16);
            self.crc.write(val);
        }

        self.crc.read()
    }

    fn read_adc(&mut self) -> u32 {
        self.adc.read(&mut self.adc_pin).unwrap_or(0u32)
    }
}
