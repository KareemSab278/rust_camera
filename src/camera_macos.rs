use chrono::Local;
use std::fs;
use std::io::{self, Error, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

const MAX_RECORDING_TIMEOUT_SECONDS: u8 = 10;
const RECORDINGS_DIR: &str = "Videos/Recordings";

struct CameraRecorder {
    device: String,
    process: Child,
    stdin: ChildStdin,
    is_recording: Arc<AtomicBool>,
}

static RECORDING_PROCESS: Mutex<Option<CameraRecorder>> = Mutex::new(None);

// commit 2093a713b5aade52e543fdd1596c1d0c747a2b06 is most stable version of repo
// ===============================
// MAIN FUNCTION
// ===============================

pub fn camera_process(set_timeout: u8) -> Result<(), Error> {
    let timeout = if set_timeout == 0 || set_timeout > MAX_RECORDING_TIMEOUT_SECONDS {
        MAX_RECORDING_TIMEOUT_SECONDS
    } else {
        set_timeout
    };

    let cameras = list_cameras()?;

    if cameras.is_empty() {
        println!("No cameras found.");
        return Ok(());
    }

    println!("Available cameras:");

    for (index, camera) in cameras.iter().enumerate() {
        println!("{}: {}", index + 1, camera);
    }

    println!();
    println!("Select a camera by number:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let selected_camera = input
        .trim()
        .parse::<usize>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Please enter a camera number"))?;

    if selected_camera == 0 || selected_camera > cameras.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid camera selection",
        ));
    }

    let selected_camera = cameras[selected_camera - 1].clone();

    start_recording(&selected_camera, timeout)?;

    println!();
    println!("Recording started.");
    println!("Camera: {}", selected_camera);
    println!("Maximum recording time: {} seconds.", timeout);
    println!("Commands:");
    println!("  stop          - stop recording");
    println!("  status        - show active recording status");
    println!("  all / exit    - stop recording and quit");

    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let command = input.trim();

        if command.is_empty() {
            continue;
        }

        if command.eq_ignore_ascii_case("all")
            || command.eq_ignore_ascii_case("exit")
            || command.eq_ignore_ascii_case("quit")
        {
            stop_recording()?;
            println!("Recording stopped.");
            break;
        }

        if command.eq_ignore_ascii_case("status") {
            print_recording_status();
            continue;
        }

        if command.eq_ignore_ascii_case("stop") {
            stop_recording()?;
            println!("Recording stopped.");
            break;
        }

        println!("Unknown command. Use stop, status, all, or exit.");
    }

    Ok(())
}


pub fn list_cameras() -> Result<Vec<String>, io::Error> {
    let output = Command::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .output()
        .map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("Failed to run FFmpeg: {}", e))
        })?;

    let output = String::from_utf8_lossy(&output.stderr);

    let mut cameras = Vec::new();
    let mut video_devices = false;

    for line in output.lines() {
        if line.contains("AVFoundation video devices") {
            video_devices = true;
            continue;
        }

        if line.contains("AVFoundation audio devices") {
            break;
        }

        if video_devices {
            if let Some(end) = line.find(']') {
                let name = line[end + 1..].trim();

                if !name.is_empty() {
                    cameras.push(name.to_string());
                }
            }
        }
    }

    Ok(cameras
        .iter()
        .filter(|c| c.contains("USB"))
        .cloned()
        .collect::<Vec<_>>())
}

pub fn start_recording(camera: &String, timeout: u8) -> Result<(), io::Error> {
    if timeout < 1 || timeout > 120 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Timeout must be between 1 and 120 seconds",
        ));
    }

    fs::create_dir_all(RECORDINGS_DIR)?;

    stop_recording()?;

    let timestamp = Local::now().format("%d-%m-%Y-%H-%M-%S");
    let filename = format!("{}-camera.mp4", timestamp);
    let output = Path::new(RECORDINGS_DIR).join(filename);

    let (process, stdin) = start_ffmpeg(camera, &output, timeout)?;
    let is_recording = Arc::new(AtomicBool::new(true));

    let recorder = CameraRecorder {
        device: camera.clone(),
        process,
        stdin,
        is_recording,
    };

    *RECORDING_PROCESS.lock().unwrap() = Some(recorder);

    Ok(())
}

fn start_ffmpeg(
    device: &str,
    output: &Path,
    timeout: u8,
) -> Result<(Child, ChildStdin), io::Error> {
    let device = format!("{}:none", device);

    let mut process = Command::new("ffmpeg")
        .args(generate_ffmpeg_command(&device, output, timeout))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to start FFmpeg: {}", e),
            )
        })?;

    let stdin = process
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to access FFmpeg stdin"))?;

    Ok((process, stdin))
}

pub fn stop_recording() -> Result<(), io::Error> {
    let mut recording = RECORDING_PROCESS.lock().unwrap();
    let recorder = recording.take();

    drop(recording);

    if let Some(mut recorder) = recorder {
        recorder.is_recording.store(false, Ordering::Release);
        let _ = recorder.stdin.write_all(b"q\n");
        let _ = recorder.stdin.flush();
        let _ = recorder.process.wait();
    }

    Ok(())
}


fn print_recording_status() {
    let recording = RECORDING_PROCESS.lock().unwrap();

    match recording.as_ref() {
        Some(recorder) => println!(
            "Camera ({}) recording: {}",
            recorder.device,
            recorder.is_recording.load(Ordering::Acquire)
        ),
        None => println!("No active recording."),
    }
}

pub fn delete_video(video_name: &str) -> Result<(), io::Error> {
    let path = Path::new(RECORDINGS_DIR).join(video_name);

    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Video not found"));
    }

    fs::remove_file(path)?;

    Ok(())
}

pub fn list_recordings() -> Vec<String> {
    let entries = match fs::read_dir(RECORDINGS_DIR) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".mp4"))
        .collect()
}

fn generate_ffmpeg_command(device: &str, output: &Path, timeout: u8) -> Vec<String> {
    vec![
        "-f".into(),
        "avfoundation".into(),
        "-video_size".into(),
        "640x480".into(),
        "-framerate".into(),
        "30".into(),
        "-i".into(),
        device.into(),
        "-vf".into(),
        "fps=30".into(),
        "-c:v".into(),
        "h264_videotoolbox".into(),
        "-b:v".into(),
        "2500k".into(),
        "-an".into(),
        "-y".into(),
        "-t".into(),
        timeout.to_string().into(),
        output.to_string_lossy().into_owned(),
    ]
}
