mod camera;

use std::io;

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
    println!("Select cameras (e.g. 1,2,3) or 'a' for all:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let selected_cameras: Vec<u8> =
        if input.trim().eq_ignore_ascii_case("a") {
            (0..cameras.len()).map(|i| i as u8).collect()
        } else {
            input
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
                .collect::<Result<Vec<_>, _>>()?
        };

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
    println!("Maximum recording time: 6 seconds.");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    camera::stop_recording()?;

    println!("Recording stopped.");

    Ok(())
}