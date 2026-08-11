mod camera;

use std::io;
use std::thread;
use std::time::Duration;

const MAX_RECORDING_TIME: u8 = 6; // seconds
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
    println!("Select cameras (e.g. 1,2,3):");


    let mut input = String::new();
    io::stdin().read_line(&mut input)?;


    let selected_cameras: Vec<u8> = input
        .trim()
        .split(',')
        .map(|value| {
            value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Please enter camera numbers separated by commas",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|selection| {
            if selection == 0 || selection > cameras.len() {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Invalid camera selection",
                ))
            } else {
                Ok((selection - 1) as u8)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;


    if selected_cameras.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No cameras selected",
        ));
    }


    camera::start_recording(&selected_cameras)?;


    println!();
    println!("Recording started.");
    println!("Cameras: {:?}", selected_cameras);
    println!("Press Enter to stop recording.");
    println!("Maximum recording time: {} seconds.", MAX_RECORDING_TIME);

    let input_thread = thread::spawn(|| {
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
    });


    for _ in 0..MAX_RECORDING_TIME {
        if input_thread.is_finished() {
            break;
        }


        thread::sleep(Duration::from_secs(1));
    }


    camera::stop_recording()?;


    println!("Recording stopped.");


    Ok(())
}
