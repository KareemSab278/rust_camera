mod camera;
use std::io::{Error};


fn main() -> Result<(), Error> {
    // if arg == 0 then default to MAX_RECORDING_TIMEOUT_SECONDS = 60
    camera::camera_process(10)?;
    Ok(())
}