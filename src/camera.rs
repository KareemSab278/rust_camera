use chrono::Local;
use std::fs;
use std::io::{self, Error, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

const MAX_RECORDING_TIMEOUT_SECONDS: u8 = 60;
const RECORDINGS_DIR: &str = "Videos/Recordings";
static RECORDING_PROCESSES: Mutex<Vec<(Child, ChildStdin)>> = Mutex::new(Vec::new());

// ===============================
// MAIN FUCNCTION
// ===============================

pub fn camera_process() -> Result<(), Error> {
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
    println!("Select cameras (e.g. 1,2,3) or 'a' for all:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let selected_cameras: Vec<String> = if input.trim().eq_ignore_ascii_case("a") {
        cameras.clone()
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
                    Ok(cameras[selection - 1].clone())
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

    start_recording(&selected_cameras, MAX_RECORDING_TIMEOUT_SECONDS)?;

    println!();
    println!("Recording started.");
    println!("Cameras: {:?}", selected_cameras);
    println!("Press Enter to stop recording.");
    println!("Maximum recording time: {} seconds.", MAX_RECORDING_TIMEOUT_SECONDS);

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    stop_recording()?;

    println!("Recording stopped.");

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn list_cameras() -> Result<Vec<String>, io::Error> {
    let mut cameras = Vec::new();

    for i in 0..32 {
        let device = format!("/dev/video{}", i);

        if !Path::new(&device).exists() {
            continue;
        }

        let output = Command::new("v4l2-ctl")
            .args(["-d", &device, "--all"])
            .output();

        let output = match output {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };

        let text = String::from_utf8_lossy(&output.stdout);

        if text.contains("Driver name      : uvcvideo")
            && text.contains("Capabilities     : timeperframe")
        {
            cameras.push(device);
        }
    }

    Ok(cameras)
}

#[cfg(target_os = "macos")]
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

pub fn start_recording(cameras: &[String], timeout: u8) -> Result<(), io::Error> {
    if timeout < 1 || timeout > 120 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Timeout must be between 1 and 120 seconds",
        ));
    }

    fs::create_dir_all(RECORDINGS_DIR)?;

    stop_recording()?;

    for (index, device) in cameras.iter().enumerate() {
        let timestamp = Local::now().format("%d-%m-%Y-%H-%M-%S");

        let filename = format!("{}-camera-{}.mp4", timestamp, index + 1);

        let output = Path::new(RECORDINGS_DIR).join(filename);

        let recording = start_ffmpeg(device, &output, timeout)?;

        RECORDING_PROCESSES.lock().unwrap().push(recording);
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn start_ffmpeg(
    device: &str,
    output: &Path,
    timeout: u8,
) -> Result<(Child, ChildStdin), io::Error> {
    let mut process = Command::new("ffmpeg")
        .args(generate_ffmpeg_command(device, output, timeout))
        .stdin(Stdio::piped())
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

#[cfg(target_os = "macos")]
fn start_ffmpeg(
    device: &str,
    output: &Path,
    timeout: u8,
) -> Result<(Child, ChildStdin), io::Error> {
    let device = format!("{}:none", device);

    let mut process = Command::new("ffmpeg")
        .args(generate_ffmpeg_command(&device, output, timeout))
        .stdin(Stdio::piped())
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
    let mut recordings = RECORDING_PROCESSES
        .lock()
        .unwrap()
        .drain(..)
        .collect::<Vec<_>>();

    // for (_, stdin) in &mut recordings {
    //     stdin.write_all(b"q\n")?;
    //     stdin.flush()?;
    // }

    for (_, stdin) in &mut recordings {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }

    // for (mut process, _) in recordings {
    //     process.wait()?;
    // }

    for (mut process, _) in recordings {
        let _ = process.wait();
    }

    Ok(())
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

#[cfg(target_os = "linux")]
fn generate_ffmpeg_command(device: &str, output: &Path, timeout: u8) -> Vec<String> {
    vec![
        "-f".into(),
        "v4l2".into(),
        "-i".into(),
        device.into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "32".into(),
        "-an".into(),
        "-y".into(),
        "-t".into(),
        timeout.to_string().into(),
        output.to_string_lossy().into_owned(),
    ]
}

#[cfg(target_os = "macos")]
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
        "libx264".into(),
        "-preset".into(),
        "ultrafast".into(),
        "-crf".into(),
        "32".into(),
        "-an".into(),
        "-y".into(),
        "-t".into(),
        timeout.to_string().into(),
        output.to_string_lossy().into_owned(),
    ]
}
