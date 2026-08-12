use chrono::Local;
use std::fs;
use std::io::{self, Error, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};

const MAX_RECORDING_TIMEOUT_SECONDS: u8 = 30;
const RECORDINGS_DIR: &str = "Videos/Recordings";

struct CameraRecorder {
    device: String,
    process: Child,
    stdin: ChildStdin,
    is_recording: Arc<AtomicBool>,
}

static RECORDING_PROCESSES: Mutex<Vec<Option<CameraRecorder>>> = Mutex::new(Vec::new());

// commit 2093a713b5aade52e543fdd1596c1d0c747a2b06 is most stable version of repo
// ===============================
// MAIN FUCNCTION
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

    start_recording(&selected_cameras, timeout)?;

    println!();
    println!("Recording started.");
    println!("Cameras: {:?}", selected_cameras);
    println!("Maximum recording time: {} seconds.", timeout);
    println!("Commands:");
    println!("  stop <n>      - stop camera number n");
    println!("  stop <n,m>    - stop multiple cameras");
    println!("  status        - show active recordings");
    println!("  all / exit    - stop all recordings and quit");

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
            println!("All recordings stopped.");
            break;
        }

        if command.eq_ignore_ascii_case("status") {
            print_recording_status();
            continue;
        }

        if let Some(indices) = parse_stop_command(command) {
            stop_recording_selected(&indices)?;
            if RECORDING_PROCESSES
                .lock()
                .unwrap()
                .iter()
                .all(|entry| entry.is_none())
            {
                println!("All selected cameras have stopped.");
                break;
            }
            continue;
        }

        println!("Unknown command. Use stop <n>, status, all, or exit.");
    }

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

        let (process, stdin) = start_ffmpeg(device, &output, timeout)?;
        let is_recording = Arc::new(AtomicBool::new(true));

        let recorder = CameraRecorder {
            device: device.clone(),
            process,
            stdin,
            is_recording,
        };

        RECORDING_PROCESSES.lock().unwrap().push(Some(recorder));
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
    let mut recordings = RECORDING_PROCESSES.lock().unwrap();
    let processes = recordings
        .iter_mut()
        .filter_map(|entry| entry.take())
        .collect::<Vec<_>>();

    drop(recordings);

    for mut recorder in processes {
        recorder.is_recording.store(false, Ordering::Release);
        let _ = recorder.stdin.write_all(b"q\n");
        let _ = recorder.stdin.flush();
        let _ = recorder.process.wait();
    }

    Ok(())
}

pub fn stop_recording_camera(camera_number: usize) -> Result<(), io::Error> {
    let mut recordings = RECORDING_PROCESSES.lock().unwrap();
    let mut recorder = recordings
        .get_mut(camera_number)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid camera number"))?
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Camera is not recording"))?;

    recorder.is_recording.store(false, Ordering::Release);
    let _ = recorder.stdin.write_all(b"q\n");
    let _ = recorder.stdin.flush();
    let _ = recorder.process.wait();

    Ok(())
}

pub fn stop_recording_selected(camera_numbers: &[usize]) -> Result<(), io::Error> {
    for &camera_number in camera_numbers {
        if let Err(err) = stop_recording_camera(camera_number.saturating_sub(1)) {
            eprintln!("Failed to stop camera {}: {}", camera_number, err);
        }
    }
    Ok(())
}

fn parse_stop_command(command: &str) -> Option<Vec<usize>> {
    let lower = command.to_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("stop ") {
        rest
    } else if let Some(rest) = lower.strip_prefix("kill ") {
        rest
    } else {
        return None;
    };

    let indices = rest
        .split(',')
        .map(|part| part.trim().parse::<usize>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    if indices.is_empty() {
        return None;
    }

    Some(indices)
}

fn print_recording_status() {
    let recordings = RECORDING_PROCESSES.lock().unwrap();
    for (index, entry) in recordings.iter().enumerate() {
        match entry {
            Some(recorder) => println!("Camera {} ({}) recording: {}", index + 1, recorder.device, recorder.is_recording.load(Ordering::Acquire)),
            None => println!("Camera {} stopped", index + 1),
        }
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

#[cfg(target_os = "linux")]
fn generate_ffmpeg_command(device: &str, output: &Path, timeout: u8) -> Vec<String> {
    vec![
        "-f".into(),
        "v4l2".into(),
        "-framerate".into(),
        "10".into(),
        "-video_size".into(),
        "640x480".into(),
        "-i".into(),
        device.into(),
        "-c:v".into(),
        "h264_v4l2m2m".into(),
        "-b:v".into(),
        "2M".into(),
        "-an".into(),
        "-y".into(),
        "-t".into(),
        timeout.to_string().into(),
        output.to_string_lossy().into_owned(),
    ]
}

// Previous CPU-heavy software encode command (keep commented for easy revert):
// #[cfg(target_os = "linux")]
// fn generate_ffmpeg_command(device: &str, output: &Path, timeout: u8) -> Vec<String> {
//     vec![
//         "-f".into(),
//         "v4l2".into(),
//         "-i".into(),
//         device.into(),
//         "-c:v".into(),
//         "libx264".into(),
//         "-preset".into(),
//         "ultrafast".into(),
//         "-crf".into(),
//         "32".into(),
//         "-an".into(),
//         "-y".into(),
//         "-t".into(),
//         timeout.to_string().into(),
//         output.to_string_lossy().into_owned(),
//     ]
// }

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

// Previous CPU-heavy software encode command (keep commented for easy revert):
// #[cfg(target_os = "macos")]
// fn generate_ffmpeg_command(device: &str, output: &Path, timeout: u8) -> Vec<String> {
//     vec![
//         "-f".into(),
//         "avfoundation".into(),
//         "-video_size".into(),
//         "640x480".into(),
//         "-framerate".into(),
//         "30".into(),
//         "-i".into(),
//         device.into(),
//         "-vf".into(),
//         "fps=30".into(),
//         "-c:v".into(),
//         "libx264".into(),
//         "-preset".into(),
//         "ultrafast".into(),
//         "-crf".into(),
//         "32".into(),
//         "-an".into(),
//         "-y".into(),
//         "-t".into(),
//         timeout.to_string().into(),
//         output.to_string_lossy().into_owned(),
//     ]
// }
