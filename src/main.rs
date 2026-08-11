mod camera;

use std::io;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), io::Error> {
    let cameras = camera::list_cameras()?;

    if cameras.is_empty() {
        println!("No cameras found.");
        return Ok(());
    }

    println!("Available cameras:");

    for (index, camera) in cameras.iter().enumerate() {
        println!("{}: {}", index + 1, camera);
    }

    println!();
    println!("Select a camera:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let selection: usize = input.trim().parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Please enter a number",
        )
    })?;

    if selection == 0 || selection > cameras.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid camera selection",
        ));
    }

    let camera = (selection - 1) as u8;

    camera::start_camera(camera)?;

    println!();
    println!("Recording started.");
    println!("Press Enter to stop recording.");
    println!("Maximum recording time: 1 minute.");

    let input_thread = thread::spawn(|| {
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
    });

    for _ in 0..5 {
        if input_thread.is_finished() {
            break;
        }

        thread::sleep(Duration::from_secs(1));
    }

    camera::stop_camera()?;

    println!("Recording stopped.");

    Ok(())
}