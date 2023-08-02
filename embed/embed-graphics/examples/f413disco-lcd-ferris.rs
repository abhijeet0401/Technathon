//! Draws the Ferris crab moving up and down the screen on the LCD display driven by the ST7789 LCD
//! driver connected via a 16bit parallel bus on the STM32F413Discovery board
//!
//! To run this feature you can (after installing `probe-run` use:
//!
//! ```bash
//! cargo run --release --example f413disco_lcd_ferris --features="stm32f413,fsmc_lcd"
//! ```

#![no_main]
#![no_std]

use panic_halt as _;
use rtt_target::{self, rtt_init_print};

use stm32f4xx_hal as hal;

#[allow(unused_imports)]
use num_traits::real::Real;

use core::f32::consts::PI;
use crate::hal::{
    fsmc_lcd::{DataPins16, FsmcLcd, LcdPins, Timing},
    gpio::Speed,
    pac::{CorePeripherals, Peripherals},
    prelude::*,
};

use embedded_graphics::geometry::Size;
use embedded_graphics::image::*;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::*;
use st7789::*;
use embedded_graphics::{
    mono_font::{ascii::FONT_9X15, MonoTextStyle},

    text::Text,
};
pub use display_interface::{DisplayError, WriteOnlyDataCommand};
const MARGIN: u32 = 5;

/// Define the lovely ferris crab sprite

fn polar(circle: &Circle, angle: f32, radius_delta: i32) -> Point {
    let radius = circle.diameter as f32 / 2.0 + radius_delta as f32;

    circle.center()
        + Point::new(
            (angle.sin() * radius) as i32,
            -(angle.cos() * radius) as i32,
        )
}

/// Converts an hour into an angle in radians.
fn hour_to_angle(hour: u32) -> f32 {
    // Convert from 24 to 12 hour time.
    let hour = hour % 12;

    (hour as f32 / 12.0) * 2.0 * PI
}

/// Converts a sexagesimal (base 60) value into an angle in radians.
fn sexagesimal_to_angle(value: u32) -> f32 {
    (value as f32 / 60.0) * 2.0 * PI
}

/// Creates a centered circle for the clock face.
fn create_face(target: &impl DrawTarget) -> Circle {
    // The draw target bounding box can be used to determine the size of the display.
    let bounding_box = target.bounding_box();

    let diameter = bounding_box.size.width.min(bounding_box.size.height) - 2 * MARGIN;

    Circle::with_center(bounding_box.center(), diameter)
}

/// Draws a circle and 12 graduations as a simple clock face.
fn draw_face<D>(target: &mut D, clock_face: &Circle) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Draw the outer face.
    (*clock_face)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 2))
        .draw(target);

    // Draw 12 graduations.
    for angle in (0..12).map(hour_to_angle) {
        // Start point on circumference.
        let start = polar(clock_face, angle, 0);

        // End point offset by 10 pixels from the edge.
        let end = polar(clock_face, angle, -10);

        Line::new(start, end)
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
            .draw(target);
    }

    Ok(())
}

/// Draws a clock hand.
fn draw_hand<D>(
    target: &mut D,
    clock_face: &Circle,
    angle: f32,
    length_delta: i32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let end = polar(clock_face, angle, length_delta);

    Line::new(clock_face.center(), end)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
        .draw(target)
}

/// Draws a decorative circle on the second hand.
fn draw_second_decoration<D>(
    target: &mut D,
    clock_face: &Circle,
    angle: f32,
    length_delta: i32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    let decoration_position = polar(clock_face, angle, length_delta);

    let decoration_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb565::BLACK)
        .stroke_color(Rgb565::WHITE)
        .stroke_width(1)
        .build();

    // Draw a fancy circle near the end of the second hand.
    Circle::with_center(decoration_position, 11)
        .into_styled(decoration_style)
        .draw(target)
}

/// Draw digital clock just above center with black text on a white background
fn draw_digital_clock<D>(
    target: &mut D,
    clock_face: &Circle,
    time_str: &str,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    // Create a styled text object for the time text.
    let mut text = Text::new(
        &time_str,
        Point::zero(),
        MonoTextStyle::new(&FONT_9X15, Rgb565::BLACK),
    );

    // Move text to be centered between the 12 o'clock point and the center of the clock face.
    text.translate_mut(
        clock_face.center()
            - text.bounding_box().center()
            - clock_face.bounding_box().size.y_axis() / 4,
    );

    // Add a background around the time digits.
    // Note that there is no bottom-right padding as this is added by the font renderer itself.
    let text_dimensions = text.bounding_box();
    Rectangle::new(
        text_dimensions.top_left - Point::new(3, 3),
        text_dimensions.size + Size::new(4, 4),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
    .draw(target);

    // Draw the text after the background is drawn.
    text.draw(target);

    Ok(())
}
#[cortex_m_rt::entry]
fn main() -> ! {
    // Initialise RTT so you can see panics and other output in your `probe-run` output
    rtt_init_print!(NoBlockTrim);

    if let (Some(p), Some(cp)) = (Peripherals::take(), CorePeripherals::take()) {
        // Split all the GPIO blocks we need
        let gpiob = p.GPIOB.split();
        let gpiod = p.GPIOD.split();
        let gpioe = p.GPIOE.split();
        let gpiof = p.GPIOF.split();
        let gpiog = p.GPIOG.split();

        // Configure and lock the clocks at maximum warp
        let rcc = p.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(100.MHz()).freeze();

        // Define the pins we need for our 16bit parallel bus
        use stm32f4xx_hal::gpio::alt::fsmc as alt;
        let lcd_pins = LcdPins::new(
            DataPins16::new(
                gpiod.pd14, gpiod.pd15, gpiod.pd0, gpiod.pd1, gpioe.pe7, gpioe.pe8, gpioe.pe9,
                gpioe.pe10, gpioe.pe11, gpioe.pe12, gpioe.pe13, gpioe.pe14, gpioe.pe15, gpiod.pd8,
                gpiod.pd9, gpiod.pd10,
            ),
            alt::Address::from(gpiof.pf0),
            gpiod.pd4,
            gpiod.pd5,
            alt::ChipSelect3::from(gpiog.pg10),
        );

        // Setup the RESET pin
        let rst = gpiob.pb13.into_push_pull_output().speed(Speed::VeryHigh);

        // We're not using the "tearing" signal from the display
        let mut _te = gpiob.pb14.into_floating_input();

        // Get delay provider
        let mut delay = cp.SYST.delay(&clocks);

        // Set up timing
        let write_timing = Timing::default().data(3).address_setup(3).bus_turnaround(0);
        let read_timing = Timing::default().data(8).address_setup(8).bus_turnaround(0);

        // Initialise FSMC memory provider
        let (_fsmc, interface) = FsmcLcd::new(p.FSMC, lcd_pins, &read_timing, &write_timing);

        // Pass display-interface instance ST7789 driver to setup a new display
        let mut display = ST7789::new(
            interface,
            Some(rst),
            Some(gpioe.pe5.into_push_pull_output()),
            240,
            240,
        );

        // Initialise the display and clear the screen
        display.init(&mut delay).unwrap();
        display.clear(Rgb565::WHITE).unwrap();
        let clock_face = create_face(&display);

    'running: loop {


        // Calculate the position of the three clock hands in radians.
        let hours_radians = hour_to_angle(23 as u32);
        let minutes_radians = sexagesimal_to_angle(15 as u32);
        let seconds_radians = sexagesimal_to_angle(30 as u32);

        // NOTE: In no-std environments, consider using
        // [arrayvec](https://stackoverflow.com/a/39491059/383609) and a fixed size buffer
        // = write!(
        //     "{:02}:{:02}:{:02}",
        //     time.hour(),
        //     time.minute(),
        //     time.second()
        // );
        let digital_clock_text ="Hello World";
        // let mut buf = ArrayString::<[u8; 100]>::new();
        // write!(&mut buf, "{}", x).expect("Can't write");
        // assert_eq!(&buf, "123");

        display.clear(Rgb565::BLACK).unwrap();
        draw_face(&mut display, &clock_face);
        draw_hand(&mut display, &clock_face, hours_radians, -60);
        draw_hand(&mut display, &clock_face, minutes_radians, -30);
        draw_hand(&mut display, &clock_face, seconds_radians, 0);
        draw_second_decoration(&mut display, &clock_face, seconds_radians, -20);

        // Draw digital clock just above center.
        draw_digital_clock(&mut display, &clock_face, &digital_clock_text);

        // Draw a small circle over the hands in the center of the clock face.
        // This has to happen after the hands are drawn so they're covered up.
        Circle::with_center(clock_face.center(), 9)
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(&mut display);
        delay.delay_ms(10000_u32);

      
    }        

        // Load our crab sprite
        // const SIZEX: i32 = 86;
        // const SIZEY: i32 = 64;
        // const STEP: i32 = 4;
        // let raw_image_data = ImageRawLE::new(&FERRIS, SIZEX as u32);

        // loop {
        //     disp.clear(Rgb565::BLACK).unwrap();

        //     for x in (0..240 - SIZEX).step_by(STEP as usize) {
        //         let mut ferris = Image::new(&raw_image_data, Point::new(x, 0));
        //         let mut horiz_line =
        //             Rectangle::new(Point::new(x, 0), Size::new(SIZEX as u32, STEP as u32))
        //                 .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK));
        //         let vert_line = Rectangle::new(Point::new(x, 0), Size::new(STEP as u32, 240))
        //             .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK));

        //         for _y in (0..240 - SIZEY).step_by(STEP as usize) {
        //             horiz_line.draw(&mut disp).unwrap();
        //             ferris
        //                 .translate_mut(Point::new(0, STEP))
        //                 .draw(&mut disp)
        //                 .unwrap();
        //             horiz_line.translate_mut(Point::new(0, STEP));
        //         }

        //         let mut horiz_line = Rectangle::new(
        //             Point::new(x, 239 - STEP),
        //             Size::new(SIZEX as u32, STEP as u32),
        //         )
        //         .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK));

        //         for _y in (0..240 - SIZEY).rev().step_by(STEP as usize) {
        //             horiz_line.draw(&mut disp).unwrap();
        //             ferris
        //                 .translate_mut(Point::new(0, -STEP))
        //                 .draw(&mut disp)
        //                 .unwrap();
        //             horiz_line.translate_mut(Point::new(0, -STEP));
        //         }
        //         vert_line.draw(&mut disp).unwrap();
        //     }
        // }
    }

    loop {
        continue;
    }
}
