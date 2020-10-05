#![cfg_attr(not(test), no_std)]
#![no_main]

// To use example, press any key in serial terminal
// Packet will send and "Transmit Done!" will print when radio is done sending packet

use panic_semihosting as _;

use core::fmt::Write;
use hal::device::USART1 as DebugUsart;
use hal::{
    adc::{Adc, SampleTime},
    gpio::*,
    prelude::*,
    timer::{CountDownTimer, Timer},
};
use hal::{pac, pac::Interrupt, serial};
use lorawan_device::{Device as LoRaWanDevice, Event as LoRaWanEvent, Response as LoRaWanResponse};
use rtic::app;
use stm32f1xx_hal as hal;
use sx12xx;
use sx12xx::Sx12xx;

use cortex_m_rt::{exception, ExceptionFrame};

mod bindings;
mod rng;
pub use bindings::initialize_irq as initialize_radio_irq;
pub use bindings::RadioIRQ;
pub use bindings::TcxoEn;
use rng::Rng;

static mut RNG: Option<Rng> = None;
fn get_random_u32() -> u32 {
    unsafe { &mut RNG }
        .iter_mut()
        .map(|rng| rng.rand_u32())
        .next()
        .unwrap_or(0)
}

pub struct TimerContext {
    pub target: u16,
    pub count: u16,
    pub enable: bool,
}

#[app(device = stm32f1xx_hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        debug_uart: serial::Tx<DebugUsart>,
        uart_rx: serial::Rx<DebugUsart>,
        timer: CountDownTimer<pac::TIM2>,
        #[init([0;512])]
        buffer: [u8; 512],
        #[init(0)]
        count: u8,
        sx12xx: Sx12xx,
        radio_irq: bindings::RadioIRQ,
        lorawan: LoRaWanDevice<Sx12xx, sx12xx::Event>,
        #[init(TimerContext {
            target: 0,
            count: 0,
            enable: false,
        })]
        timer_context: TimerContext,
        led_pin: gpioc::PC13<Output<PushPull>>,
    }

    #[init(spawn = [send_ping, lorawan_event], resources = [buffer])]
    fn init(ctx: init::Context) -> init::LateResources {
        let mut device = ctx.device;

        let mut rcc = device.RCC.constrain();
        let mut flash = device.FLASH.constrain();
        let mut afio = device.AFIO.constrain(&mut rcc.apb2);
        let mut bkp = rcc
            .bkp
            .constrain(device.BKP, &mut rcc.apb1, &mut device.PWR);
        let crc = device.CRC.new(&mut rcc.ahb);

        let clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            .sysclk(56.mhz())
            .pclk1(28.mhz())
            .adcclk(14.mhz())
            .freeze(&mut flash.acr);

        let mut gpioa = device.GPIOA.split(&mut rcc.apb2);
        let mut gpiob = device.GPIOB.split(&mut rcc.apb2);
        let mut gpioc = device.GPIOC.split(&mut rcc.apb2);

        let led_pin = gpioc
            .pc13
            .into_push_pull_output_with_state(&mut gpioc.crh, State::High);

        let usart1_tx = gpioa.pa9.into_alternate_push_pull(&mut gpioa.crh);
        let usart1_rx = gpioa.pa10.into_floating_input(&mut gpioa.crh);
        let serial_pins = (usart1_tx, usart1_rx);

        let usart1_config = serial::Config::default();

        let mut serial = serial::Serial::usart1(
            device.USART1,
            serial_pins,
            &mut afio.mapr,
            usart1_config,
            clocks,
            &mut rcc.apb2,
        );

        // listen for incoming bytes which will trigger transmits
        serial.listen(serial::Event::Rxne);
        let (mut tx, rx) = serial.split();

        write!(tx, "===== LongFi Device Test =======\r\n").unwrap();

        // constructor initializes 48 MHz clock that RNG requires
        // Initialize 48 MHz clock and RNG
        let adc_in_pin = gpioa.pa1.into_analog(&mut gpioa.crl);
        let mut adc = Adc::adc1(device.ADC1, &mut rcc.apb2, clocks);
        adc.set_sample_time(SampleTime::T_1);
        let rtc = hal::rtc::Rtc::rtc(device.RTC, &mut bkp);
        unsafe { RNG = Some(Rng::new(rtc, adc, adc_in_pin, crc)) };
        let radio_irq = initialize_radio_irq(gpioa.pa3, &mut gpioa.crl, &mut afio, &device.EXTI);

        // Configure the timer.
        let timer = Timer::tim2(device.TIM2, &clocks, &mut rcc.apb1).start_count_down(100.khz());

        let bindings = bindings::new(
            device.SPI1,
            &mut rcc.apb2,
            gpioa.pa5,
            gpioa.pa6,
            gpioa.pa7,
            gpiob.pb0,
            gpiob.pb1,
            &mut gpioa.crl,
            &mut gpiob.crl,
            &mut afio.mapr,
            clocks,
        );
        let radio = sx12xx::Radio::sx1276();

        let mut sx12xx = Sx12xx::new(radio, bindings);
        sx12xx.set_public_network(true);

        let lorawan = LoRaWanDevice::new(
            0x00_u64.to_be_bytes(),
            0x00_u64.to_be_bytes(),
            0x00_u128.to_be_bytes(),
            get_random_u32,
        );

        ctx.spawn.lorawan_event(LoRaWanEvent::StartJoin).unwrap();

        write!(tx, "Going to main loop\r\n").unwrap();

        // Return the initialised resources.
        init::LateResources {
            debug_uart: tx,
            uart_rx: rx,
            sx12xx,
            lorawan,
            timer,
            radio_irq,
            led_pin,
        }
    }

    #[task(capacity = 4, priority = 2, resources = [debug_uart, buffer, sx12xx, lorawan], spawn  = [lorawan_response])]
    fn radio_event(ctx: radio_event::Context, event: sx12xx::Event) {
        let (sx12xx, lorawan) = (ctx.resources.sx12xx, ctx.resources.lorawan);
        let debug = ctx.resources.debug_uart;
        match event {
            sx12xx::Event::Sx12xxEvent_DIO0 => write!(debug, "DIO0 \r\n").unwrap(),
            _ => write!(debug, "Unexpected!\r\n").unwrap(),
        }

        if let Some(response) = lorawan.handle_radio_event(sx12xx, event) {
            ctx.spawn.lorawan_response(response).unwrap();
        }
    }

    #[task(capacity = 4, priority = 2, resources = [debug_uart, buffer, sx12xx, lorawan], spawn  = [lorawan_response])]
    fn lorawan_event(ctx: lorawan_event::Context, event: LoRaWanEvent) {
        let (sx12xx, lorawan) = (ctx.resources.sx12xx, ctx.resources.lorawan);
        let debug = ctx.resources.debug_uart;

        match event {
            LoRaWanEvent::TimerFired => {
                write!(debug, "Providing Timer Event!\r\n").unwrap();
            }
            _ => (),
        }

        if let Some(response) = lorawan.handle_event(sx12xx, event) {
            ctx.spawn.lorawan_response(response).unwrap();
        }
    }

    #[task(capacity = 4, priority = 2, resources = [debug_uart, timer_context])]
    fn lorawan_response(ctx: lorawan_response::Context, response: LoRaWanResponse) {
        match response {
            LoRaWanResponse::TimerRequest(ms) => {
                write!(ctx.resources.debug_uart, "Arming Timer {:} ms \r\n", ms).unwrap();
                // grab a lock on timer and arm a timeout
                ctx.resources.timer_context.target = ms as u16;
                ctx.resources.timer_context.count = 0;
                ctx.resources.timer_context.enable = true;
                // trigger timer so that it can set itself up
                rtic::pend(Interrupt::TIM2);
            }
            LoRaWanResponse::Error => {
                write!(ctx.resources.debug_uart, "LoRaWanResponse::Error!!\r\n").unwrap();
            }
        }
    }

    #[task(capacity = 4, priority = 2, resources = [debug_uart, count, sx12xx, lorawan, led_pin])]
    fn send_ping(ctx: send_ping::Context) {
        use embedded_hal::digital::v2::OutputPin;

        let (sx12xx, lorawan) = (ctx.resources.sx12xx, ctx.resources.lorawan);
        let debug = ctx.resources.debug_uart;
        let led_pin = ctx.resources.led_pin;

        led_pin.set_low();
        write!(debug, "Sending Ping\r\n").unwrap();

        let data: [u8; 5] = [0xDE, 0xAD, 0xBE, 0xEF, *ctx.resources.count];
        *ctx.resources.count += 1;

        lorawan.send(sx12xx, &data, 1, true);
        led_pin.set_high();
    }

    #[task(binds = USART1, priority=1, resources = [uart_rx], spawn = [send_ping])]
    fn USART1(ctx: USART1::Context) {
        let rx = ctx.resources.uart_rx;
        rx.read().unwrap();
        ctx.spawn.send_ping().unwrap();
    }

    /// This task runs on rising edge of DIO0 IRQ line,
    #[task(binds = EXTI3, priority = 1, resources = [radio_irq], spawn = [radio_event])]
    fn EXTI3(ctx: EXTI3::Context) {
        use stm32f1xx_hal::gpio::ExtiPin;
        let radio_irq = ctx.resources.radio_irq;

        if radio_irq.check_interrupt() {
            radio_irq.clear_interrupt_pending_bit();

            ctx.spawn
                .radio_event(sx12xx::Event::Sx12xxEvent_DIO0)
                .unwrap();
        }
    }

    // This is a pretty not scalable timeout implementation
    // but we can switch to RTFM timer queues later maybe
    #[task(binds = TIM2, resources = [timer, timer_context], spawn = [lorawan_event])]
    fn TIM2(mut ctx: TIM2::Context) {
        use hal::timer::Event::Update;

        let timer = ctx.resources.timer;
        let spawn = ctx.spawn;
        timer.clear_update_interrupt_flag();

        ctx.resources.timer_context.lock(|context| {
            // if timer has been disabled,
            // timeout has been dismarmed
            if !context.enable {
                context.target = 0;
                context.count = 0;
                timer.unlisten(Update);
            } else {
                // if count is 0, we are just setting up a timeout
                if context.count == 0 {
                    timer.reset();
                    timer.listen(Update);
                }
                context.count += 1;

                // if we have a match, timer has fired
                if context.count == context.target {
                    timer.unlisten(Update);
                    context.count = 0;
                    context.enable = false;
                    spawn.lorawan_event(LoRaWanEvent::TimerFired).unwrap()
                }
            }
        });
    }

    // Interrupt handlers used to dispatch software tasks
    extern "C" {
        fn USART3(); // TODO: verify that this should indeed be USART3
    }
};
