/*
    simulate real world tauri cam use here
    start recording with camera identifier and timeout
    show recording in terminal
    stop_recording after timeout (mimicking frontend stop_recording call)
    return output path of the recording
*/

// all pass up to 60 seconds on isolated tests - most effieicnt build yet: 9177ff49924539e9470977b8e5cf501f2234d9db

mod cam;

fn main() {
    let camera = cam::list_cameras().expect("Failed to list cameras");
    println!("Available cameras: {:?}", camera);
    let found_cam = camera.first().expect("No cameras found").clone();
    println!("Using camera: {}", found_cam);

    // 0 = manual stop only; stop_recording() controls when recording ends.
    let _ = cam::start_recording(&found_cam, 0).expect("Failed to start recording");

    std::thread::sleep(std::time::Duration::from_secs(25));
    cam::stop_recording().expect("Failed to stop recording");

    let recordings = cam::list_recordings().expect("Failed to list recordings");
    
    println!("Recordings: {:?}", recordings);
    println!("Recording stopped.");
}
