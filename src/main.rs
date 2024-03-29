use embedded_graphics::{
    mono_font::MonoTextStyle,
    pixelcolor::Rgb565,
    prelude::*,
    text::{Baseline, Text, TextStyleBuilder},
};
use profont::PROFONT_10_POINT;

use embedded_hal::digital::v2::OutputPin;
use linux_embedded_hal::Serial;

use embedded_hal::prelude::*;
use gpio_cdev::{Chip, LineRequestFlags};
use linux_embedded_hal::spidev::{SpiModeFlags, SpidevOptions};
use linux_embedded_hal::{CdevPin, Delay, I2cdev, Spidev};

use bme280::BME280;
use pms5003::PMS5003;
use st7735_lcd::{Orientation, ST7735};

use systemstat::{Platform, System};

// From Pimoroni:
// const TEMP_FACTOR: f32 = 2.25;
// From measurements @ 24⁰C:
const TEMP_FACTOR: f32 = 1.4;

// In milliseconds
const REFRESH_DELAY: u16 = 10000;

fn main() {
    let i2c_bus = I2cdev::new("/dev/i2c-1").expect("i2c bus");
    let mut bme280 = BME280::new_primary(i2c_bus, Delay);
    bme280.init().expect("bme280 init");

    let sys = System::new();

    let mut spi = Spidev::open("/dev/spidev0.1").expect("SPI device");
    let options = SpidevOptions::new()
        .bits_per_word(8)
        .max_speed_hz(10_000_000)
        .mode(SpiModeFlags::SPI_MODE_0)
        .build();
    spi.configure(&options).expect("SPI configuration");

    let mut chip = Chip::new("/dev/gpiochip0").expect("chip");
    let display_dc = CdevPin::new(
        chip.get_line(9)
            .expect("dc line")
            .request(LineRequestFlags::OUTPUT, 1, "dc export")
            .expect("dc request"),
    )
    .expect("dc pin");
    let display_reset = CdevPin::new(
        chip.get_line(16) // Unused! Empty pin according to the pinout
            .expect("reset line")
            .request(LineRequestFlags::OUTPUT, 1, "reset export")
            .expect("reset request"),
    )
    .expect("reset pin");
    let _backlight = CdevPin::new(
        chip.get_line(12)
            .expect("backlight line")
            .request(LineRequestFlags::OUTPUT, 1, "backlight export")
            .expect("backlight request"),
    )
    .expect("backlight pin");
    let mut pms_dc = CdevPin::new(
        chip.get_line(22)
            .expect("pms dc line")
            .request(LineRequestFlags::OUTPUT, 1, "pms dc export")
            .expect("pms dc request"),
    )
    .expect("pms dc pin");
    let mut pms_reset = CdevPin::new(
        chip.get_line(27)
            .expect("pms reset line")
            .request(LineRequestFlags::OUTPUT, 1, "pms reset export")
            .expect("pms reset request"),
    )
    .expect("pms reset pin");
    let mut delay = Delay {};

    let mut display = ST7735::new(spi, display_dc, display_reset, false, true, 160, 80);
    display.init(&mut delay).unwrap();
    display
        .set_orientation(&Orientation::LandscapeSwapped)
        .unwrap();
    display
        .clear(Rgb565::BLACK)
        .expect("Failed to clear display");
    display.set_offset(0, 25);

    let character_style = MonoTextStyle::new(&PROFONT_10_POINT, Rgb565::WHITE);
    let text_style = TextStyleBuilder::new()
        .baseline(Baseline::Top)
        .build();

    pms_dc.set_high().unwrap();
    pms_reset.set_high().unwrap();

    let pms_tty = Serial::open("/dev/ttyAMA0").expect("PMS serial port");
    let mut pms5003 = PMS5003::new(pms_tty, pms_dc, pms_reset, Delay);
    pms5003.init().unwrap();

    let mut humi = String::from("N/A");
    let mut temp = String::from("N/A");
    let mut pm25 = String::from("N/A");
    let mut pm10 = String::from("N/A");

    loop {
        if let Ok(measurements) = bme280.measure() {
            let mut temperature = measurements.temperature;
            if let Ok(cpu_temp) = sys.cpu_temp() {
                println!("Raw T:  {:.1}°\nCPU T:  {:.1}°", temperature, cpu_temp);
                temperature -= (cpu_temp - temperature) / TEMP_FACTOR;
            }
            temp = format!("T:      {:.1}°", temperature);
            humi = format!("RH:     {:.1}%", measurements.humidity);
        } else {
            println!("Failed to read BME280, recycling old temperature value!");
        }
        if let Ok(measurements) = pms5003.measure() {
            pm25 = format!("PM 2.5: {:2} µg/m³", measurements.ug_per_m3.pm2p5);
            pm10 = format!("PM 10:  {:2} µg/m³", measurements.ug_per_m3.pm10p0);
        } else {
            println!("Failed to read PMS5003, recycling old particle values!");
        }
        let text = format!("{}\n{}\n{}\n{}", temp, humi, pm25, pm10);
        println!("{}", text);
        display
            .clear(Rgb565::BLACK)
            .expect("Failed to clear display");
        Text::with_text_style(
            &text,
            Point::new(2, 2),
            character_style,
            text_style,
        )
        .draw(&mut display)
        .unwrap();
        delay.delay_ms(REFRESH_DELAY);
    }
}
