mod camera;
use std::io::{Error};


fn main() -> Result<(), Error> {
    camera::camera_process()?;
    Ok(())
}