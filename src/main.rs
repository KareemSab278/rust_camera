mod camera_linux;
mod camera_macos;
use std::io::{Error};

// if timeout = 0 then limit to 10 else if timeout > 10 then limit to 10 else use timeout

fn main() -> Result<(), Error> {
    #[cfg(target_os = "macos")]
    camera_macos::camera_process(0)?;

    #[cfg(target_os = "linux")]
    camera_linux::camera_process(0)?;
    Ok(())
}